//! Simulation du hangar : fixed-timestep, systèmes dans un ordre
//! déterministe (pré-requis multijoueur serveur autoritaire).
//!
//! Ordre des systèmes à chaque tick :
//!   1. machines (fin de cycles + vidage des files de sortie)
//!   2. tapis (avancement + transferts belt->belt / belt->machine)
//!   3. auto-acheteurs
//!   4. électricité (facturation réseau, blackout)
//!
//! Blackout : si la facture électrique amène le solde à 0, tout s'arrête
//! (tapis, machines) SAUF les actions manuelles — le joueur doit revenir
//! vendre à la main pour redémarrer. C'est la punition "réseau électrique".

use std::collections::HashMap;

use crate::balance;
use crate::economy::Economy;
use crate::grid::{cell_key, BeltState, CarriedItem, CellContent, Dir};
use crate::items::{ItemKind, PACKAGE_ITEM_COUNT_WEIGHTS, PACKAGE_ROLL_TABLE};
use crate::machines::{MachineKind, MachineState};
use crate::rng::GameRng;
use crate::shop::{Delivery, DeliveryKind, ShopItem, ALL_SHOP_ITEMS};

// ======================================================================
//  Événements de jeu (drainés par l'UI chaque frame)
// ======================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warn,
    Danger,
}

#[derive(Debug, Clone)]
pub enum GameEvent {
    Toast { text: String, severity: Severity },
    /// La bague a été vendue par erreur : pénalité + re-masquage.
    RingSold { penalty_eur: f64 },
    /// La bague est partie en fumée dans l'incinérateur.
    RingBurned,
    /// La bague est sécurisée -> VICTOIRE.
    RingSecured,
    Blackout(bool),
    NoMoney,
}

// ======================================================================
//  Colis en zone de palettes (déballage manuel)
// ======================================================================

#[derive(Debug, Clone)]
pub struct FloorPackage {
    pub id: u64,
    pub contains_ring: bool,
    /// None = colis fermé ; Some(liste) = ouvert.
    pub items: Option<Vec<ItemKind>>,
}

// ======================================================================
//  Sim
// ======================================================================

pub struct Sim {
    pub rng: GameRng,
    pub eco: Economy,

    /// Tapis, ordre d'insertion = ordre de tick (déterministe).
    pub belts: Vec<BeltState>,
    /// Cellule de chaque tapis (aligné sur `belts`).
    pub belt_cell: Vec<u64>,
    /// Machines, ordre d'insertion = ordre de tick.
    pub machines: Vec<MachineState>,
    /// Cellule de chaque machine (aligné sur `machines`).
    pub machine_cell: Vec<u64>,
    /// Occupation des cellules : clé -> index tapis/machine.
    pub grid: HashMap<u64, CellContent>,

    /// Colis en zone de palettes (déballage manuel, début de partie).
    pub floor: Vec<FloorPackage>,

    /// Stock livré par l'ordinateur, aligné sur `shop::ALL_SHOP_ITEMS`.
    /// Le placement sur la grille CONSOMME le stock (l'argent est payé à
    /// la commande, style Find The Needle).
    pub stock: [u32; ALL_SHOP_ITEMS.len()],
    /// Livraisons en cours (commandées à l'ordinateur).
    pub deliveries: Vec<Delivery>,
    /// Colis payés en attente d'un slot de palette libre.
    pub pending_packages: u64,

    pub events: Vec<GameEvent>,
    pub elapsed_s: f64,
    pub blackout: bool,

    /// Anti-spam des toasts "objet détruit".
    last_destroy_toast_s: f64,
}

/// Résultat d'un routage de fin de tapis.
enum Route {
    ToBelt(usize),
    ToMachine(usize),
    Blocked,
}

impl Sim {
    // ------------------------------------------------------------------
    //  Construction
    // ------------------------------------------------------------------

    /// Crée une partie. `seed` vient du TRNG (voir `rng::generate_master_seed`),
    /// `total_packages` du choix du menu (1 M .. 500 M).
    /// Le PREMIER tirage du PRNG place la bague — tout le reste du jeu
    /// découle ensuite de ce générateur unique (reproductible via --seed).
    pub fn new(seed: [u8; 32], total_packages: u64) -> Self {
        let mut rng = GameRng::from_seed(seed);
        let ring_slot = rng.range_u64(0..total_packages);
        let eco = Economy::new(balance::STARTING_MONEY_EUR, total_packages, ring_slot);
        Self {
            rng,
            eco,
            belts: Vec::new(),
            belt_cell: Vec::new(),
            machines: Vec::new(),
            machine_cell: Vec::new(),
            grid: HashMap::new(),
            floor: Vec::new(),
            stock: [0; ALL_SHOP_ITEMS.len()],
            deliveries: Vec::new(),
            pending_packages: 0,
            events: Vec::new(),
            elapsed_s: 0.0,
            blackout: false,
            last_destroy_toast_s: -10.0,
        }
    }

    // ------------------------------------------------------------------
    //  Boucle principale
    // ------------------------------------------------------------------

    pub fn tick(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        self.elapsed_s += dt as f64;
        if self.blackout {
            // Rien ne bouge sans courant. L'UI reste active (vente manuelle).
            self.system_power(dt);
            return;
        }
        self.system_machines(dt);
        self.system_belts(dt);
        self.system_auto_buyers(dt);
        self.system_deliveries(dt);
        self.system_power(dt);
    }

    // ------------------------------------------------------------------
    //  Système 1 bis : livraisons de l'ordinateur
    // ------------------------------------------------------------------

    fn system_deliveries(&mut self, dt: f32) {
        let _ = dt;
        let now = self.elapsed_s;
        let mut arrived: Vec<DeliveryKind> = Vec::new();
        self.deliveries.retain(|d| {
            if d.eta_s <= now {
                arrived.push(d.item);
                false
            } else {
                true
            }
        });
        for item in arrived {
            match item {
                DeliveryKind::Stock(si, qty) => {
                    self.stock[si.stock_index()] += qty;
                    self.events.push(GameEvent::Toast {
                        text: format!(
                            "Livraison : {}× {} — pose-les avec les touches 1-9.",
                            qty,
                            si.name()
                        ),
                        severity: Severity::Info,
                    });
                }
                DeliveryKind::Packages(n) => {
                    // Autant que possible tout de suite, le reste attend un slot.
                    let space = (balance::FLOOR_PACKAGE_LIMIT - self.floor.len()) as u64;
                    let drop = n.min(space);
                    for _ in 0..drop {
                        if let Some(p) = self.eco.buy_package() {
                            self.floor.push(FloorPackage { id: p.id, contains_ring: p.contains_ring, items: None });
                        } else {
                            break;
                        }
                    }
                    self.pending_packages += n - drop;
                    self.events.push(GameEvent::Toast {
                        text: format!(
                            "Livraison : {drop} colis sur la zone de palettes (mur SUD, marqueur doré)."
                        ),
                        severity: Severity::Info,
                    });
                }
            }
        }
        // Écoulement des colis en attente (1 slot libéré = 1 colis livré).
        if self.pending_packages > 0 {
            let space = (balance::FLOOR_PACKAGE_LIMIT - self.floor.len()) as u64;
            let drop = self.pending_packages.min(space);
            for _ in 0..drop {
                if let Some(p) = self.eco.buy_package() {
                    self.floor.push(FloorPackage { id: p.id, contains_ring: p.contains_ring, items: None });
                } else {
                    break;
                }
            }
            self.pending_packages -= drop;
        }
    }

    // ------------------------------------------------------------------
    //  Système 1 : machines
    // ------------------------------------------------------------------

    fn system_machines(&mut self, dt: f32) {
        for mi in 0..self.machines.len() {
            if self.machines[mi].removed {
                continue;
            }
            // Fin de cycle ?
            let mut done: Option<CarriedItem> = None;
            {
                let m = &mut self.machines[mi];
                if let Some(item) = m.busy_with.take() {
                    m.cooldown_s -= dt;
                    if m.cooldown_s <= 0.0 {
                        m.cooldown_s = 0.0;
                        done = Some(item);
                    } else {
                        m.busy_with = Some(item);
                    }
                }
            }
            if let Some(item) = done {
                self.on_machine_cycle_done(mi, item);
            }
            self.try_drain_output(mi);
        }
    }

    /// Une machine a terminé de traiter un objet.
    fn on_machine_cycle_done(&mut self, mi: usize, item: CarriedItem) {
        let kind = item.kind;
        match self.machines[mi].kind {
            MachineKind::Unpacker => {
                // Ouvre le colis : contenu + carton (max 4 slots de sortie).
                let contents = self.roll_package_contents(item.contains_ring);
                let m = &mut self.machines[mi];
                for (slot, k) in m.out_queue.iter_mut().zip(contents) {
                    *slot = Some(CarriedItem { kind: k, progress: 0.0, contains_ring: false });
                }
                m.processed += 1;
            }
            MachineKind::Seller => {
                if kind == ItemKind::Bague {
                    // 💍⚠️ Vendue par la ligne : -1/4 du capital + re-masquage.
                    let penalty = self.eco.on_ring_sold(&mut self.rng);
                    self.events.push(GameEvent::RingSold { penalty_eur: penalty });
                    self.events.push(GameEvent::Toast {
                        text: format!("LA BAGUE a été vendue au guichet ! Pénalité : −{penalty:.0} €. Elle rejoint les colis non achetés…"),
                        severity: Severity::Danger,
                    });
                } else {
                    let v = kind.sell_value() as f64;
                    self.eco.earn(v);
                }
                self.machines[mi].processed += 1;
            }
            MachineKind::Incinerator => {
                let def = kind.def();
                if def.burnable {
                    // Carton -> énergie gratuite (statistique ; la puissance
                    // instantanée de l'incinérateur est comptée dans system_power).
                    self.eco.credit_energy(balance::CARDBOARD_ENERGY_KWH);
                } else if kind == ItemKind::Bague {
                    self.eco.on_ring_burned(&mut self.rng);
                    self.events.push(GameEvent::RingBurned);
                    self.events.push(GameEvent::Toast {
                        text: "LA BAGUE est partie en fumée ! Le fournisseur la remplacera dans un colis non acheté…".into(),
                        severity: Severity::Danger,
                    });
                } else if self.elapsed_s - self.last_destroy_toast_s > 4.0 {
                    self.last_destroy_toast_s = self.elapsed_s;
                    self.events.push(GameEvent::Toast {
                        text: format!("{} détruit dans l'incinérateur ({} € perdus)", def.name_fr, def.value_eur),
                        severity: Severity::Warn,
                    });
                }
                self.machines[mi].processed += 1;
            }
            MachineKind::RingScanner => {
                if kind == ItemKind::Bague {
                    self.eco.on_ring_secured();
                    self.events.push(GameEvent::RingSecured);
                    self.events.push(GameEvent::Toast {
                        text: "💍 LA BAGUE EST SÉCURISÉE AU COFFRE — VICTOIRE !".into(),
                        severity: Severity::Info,
                    });
                } else {
                    // Passe-sauf : l'objet continue sa route (flag préservé).
                    // Sortie bloquée ? On RE-TIENT l'objet (pas d'écrasement).
                    if self.machines[mi].out_queue[0].is_some() {
                        self.machines[mi].busy_with = Some(item);
                        self.machines[mi].cooldown_s = 0.05;
                        return;
                    }
                    self.machines[mi].out_queue[0] = Some(item);
                }
                self.machines[mi].processed += 1;
            }
            MachineKind::Splitter | MachineKind::AutoBuyer | MachineKind::Generator => {
                // passifs — inatteignable (accepts() == false)
            }
            MachineKind::RobotArm => {
                // Simple manipulateur : l'objet repart devant lui (tapis OU
                // machine — voir try_drain_output). Le colis garde son flag
                // bague : c'est un colis, pas un déballage.
                if self.machines[mi].out_queue[0].is_some() {
                    // Sortie bloquée : on retient l'objet et on retente
                    // bientôt (écraser la file perdrait l'objet).
                    self.machines[mi].busy_with = Some(item);
                    self.machines[mi].cooldown_s = 0.05;
                    return;
                }
                self.machines[mi].out_queue[0] = Some(item);
                self.machines[mi].processed += 1;
            }
        }
    }

    /// Tente de vider le premier objet de la file de sortie vers la cellule
    /// devant la machine : tapis libre OU machine aval qui accepte
    /// (le Bras robot s'insère donc directement dans une ligne).
    fn try_drain_output(&mut self, mi: usize) {
        let Some(carried) = self.machines[mi].out_queue[0] else { return };
        let kind = carried.kind;
        let cell = self.machine_cell[mi];
        let dir = self.machines[mi].dir;
        if let Some(bi) = self.belt_in_front(cell, dir) {
            if self.belts[bi].item.is_none() {
                self.belts[bi].item = Some(CarriedItem { progress: 0.0, ..carried });
                self.shift_out_queue(mi);
            }
            return;
        }
        // Pas de tapis : insertion directe dans une machine aval ?
        if let Some(mj) = self.machine_in_front(cell, dir) {
            if self.machines[mj].busy_with.is_none() && self.machines[mj].accepts(kind) {
                let cycle = self.machines[mj].kind.spec().cycle_s;
                self.machines[mj].busy_with = Some(carried);
                self.machines[mj].cooldown_s = cycle;
                self.shift_out_queue(mi);
            }
        }
    }

    fn shift_out_queue(&mut self, mi: usize) {
        let q = &mut self.machines[mi].out_queue;
        q[0] = q[1];
        q[1] = q[2];
        q[2] = q[3];
        q[3] = None;
    }

    /// Index de la machine située devant (cellule + dir), s'il existe.
    fn machine_in_front(&self, cell: u64, dir: Dir) -> Option<usize> {
        let (x, y) = cell_xy(cell);
        let (dx, dy) = dir.delta();
        match self.grid.get(&cell_key(x + dx, y + dy)) {
            Some(CellContent::Machine(mi)) => Some(*mi),
            _ => None,
        }
    }

    /// Index du tapis situé devant la machine (cellule + dir), s'il existe.
    fn belt_in_front(&self, cell: u64, dir: Dir) -> Option<usize> {
        let (x, y) = cell_xy(cell);
        let (dx, dy) = dir.delta();
        match self.grid.get(&cell_key(x + dx, y + dy)) {
            Some(CellContent::Belt(bi)) => Some(*bi),
            _ => None,
        }
    }

    // ------------------------------------------------------------------
    //  Système 2 : tapis
    // ------------------------------------------------------------------

    fn system_belts(&mut self, dt: f32) {
        let speed = balance::BELT_SPEED_CELLS_PER_S;
        let mut routes: Vec<(usize, Route)> = Vec::new();

        for bi in 0..self.belts.len() {
            if self.belts[bi].removed {
                continue;
            }
            let Some(item) = self.belts[bi].item.as_mut() else { continue };
            item.progress += speed * dt;
            if item.progress < 1.0 {
                continue;
            }
            // Fin de cellule : calcule la route (sans muter encore).
            let (x, y) = cell_xy(self.belt_cell[bi]);
            let (dx, dy) = self.belts[bi].dir.delta();
            let kind = self.belts[bi].item.as_ref().unwrap().kind;
            let route = self.route_from(bi, x + dx, y + dy, kind);
            routes.push((bi, route));
        }

        // Application APRÈS la passe de lecture -> premier de la file gagne,
        // ordre d'insertion = arbitrage stable (multijoueur-friendly).
        for (bi, route) in routes {
            match route {
                Route::ToBelt(bj) => {
                    if let Some(mut item) = self.belts[bi].item.take() {
                        item.progress = 0.0;
                        self.belts[bj].item = Some(item);
                    }
                }
                Route::ToMachine(mi) => {
                    if let Some(item) = self.belts[bi].item.take() {
                        let m = &mut self.machines[mi];
                        m.busy_with = Some(item);
                        m.cooldown_s = m.kind.spec().cycle_s;
                    }
                }
                Route::Blocked => {
                    // Embouteillage : l'objet attend en bout de tapis.
                }
            }
        }
    }

    /// Où l'objet de type `kind` porté par le tapis `bi` peut-il aller en
    /// (tx, ty) ? Ne mute que le toggle du trieur (routage passif).
    fn route_from(&mut self, _bi: usize, tx: i32, ty: i32, kind: ItemKind) -> Route {
        let Some(content) = self.grid.get(&cell_key(tx, ty)) else {
            return Route::Blocked;
        };
        match content {
            CellContent::Belt(bj) => {
                if !self.belts[*bj].removed && self.belts[*bj].item.is_none() {
                    Route::ToBelt(*bj)
                } else {
                    Route::Blocked
                }
            }
            CellContent::Machine(mi) => {
                let m = &self.machines[*mi];
                if m.removed {
                    return Route::Blocked;
                }
                match m.kind {
                    // Le trieur est passif : il route immédiatement vers la
                    // sortie latérale courante si le tapis y est libre.
                    MachineKind::Splitter => {
                        let side = if m.toggle == 0 { m.dir.right() } else { m.dir.left() };
                        let (x, y) = cell_xy(self.machine_cell[*mi]);
                        let (dx, dy) = side.delta();
                        if let Some(CellContent::Belt(bj)) = self.grid.get(&cell_key(x + dx, y + dy)) {
                            let bj = *bj;
                            if !self.belts[bj].removed && self.belts[bj].item.is_none() {
                                self.machines[*mi].toggle ^= 1;
                                return Route::ToBelt(bj);
                            }
                        }
                        Route::Blocked
                    }
                    _ => {
                        if m.busy_with.is_none() && m.accepts(kind) {
                            Route::ToMachine(*mi)
                        } else {
                            Route::Blocked
                        }
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    //  Système 3 : auto-acheteurs
    // ------------------------------------------------------------------

    fn system_auto_buyers(&mut self, dt: f32) {
        for mi in 0..self.machines.len() {
            let (removed, kind, period, dir) = {
                let m = &self.machines[mi];
                (m.removed, m.kind, m.auto_buy_period_s, m.dir)
            };
            if removed || kind != MachineKind::AutoBuyer {
                continue;
            }
            self.machines[mi].cooldown_s -= dt;
            if self.machines[mi].cooldown_s > 0.0 {
                continue;
            }
            // N'achète que si le tapis devant est libre (zéro gaspillage).
            let cell = self.machine_cell[mi];
            match self.belt_in_front(cell, dir) {
                Some(bi) if self.belts[bi].item.is_none() => {
                    if let Some(p) = self.eco.buy_package() {
                        self.belts[bi].item = Some(CarriedItem {
                            kind: ItemKind::ColisFerme,
                            progress: 0.0,
                            contains_ring: p.contains_ring,
                        });
                        self.machines[mi].cooldown_s = period;
                        self.machines[mi].processed += 1;
                    } else if self.eco.money < balance::PACKAGE_PRICE_EUR {
                        // Pas d'argent : retente dans 0,5 s (pas de spam).
                        self.machines[mi].cooldown_s = 0.5;
                    } else {
                        self.machines[mi].cooldown_s = 0.1; // pool vide
                    }
                }
                _ => {
                    // Sortie bloquée : retente bientôt.
                    self.machines[mi].cooldown_s = 0.2;
                }
            }
        }
    }

    // ------------------------------------------------------------------
    //  Système 4 : électricité
    // ------------------------------------------------------------------

    /// Résumé puissance : (demande kW, kW gratuits via incinérateurs).
    /// Utilisé par le HUD et par `system_power`.
    pub fn power_summary(&self) -> (f32, f32) {
        let mut demand = self.belts.iter().filter(|b| !b.removed).count() as f32 * balance::BELT_KW;
        let mut free = 0.0;
        for m in &self.machines {
            if m.removed {
                continue;
            }
            demand += m.kind.spec().kw;
            if m.kind == MachineKind::Incinerator && m.busy_with.is_some() {
                free += balance::INCINERATOR_FREE_KW;
            }
            if m.kind == MachineKind::Generator {
                free += balance::GENERATOR_FREE_KW;
            }
        }
        (demand, free)
    }

    fn system_power(&mut self, dt: f32) {
        let (demand, free) = self.power_summary();
        let was_solvent = self.eco.money > 0.0;
        let kwh = self.eco.charge_power(demand, free, dt);
        if was_solvent && kwh > 0.0 && self.eco.money <= 0.0 {
            self.blackout = true;
            self.events.push(GameEvent::Blackout(true));
            self.events.push(GameEvent::Toast {
                text: "BLACKOUT ! Facture impayée : les machines sont à l'arrêt. Vends à la main pour redémarrer.".into(),
                severity: Severity::Danger,
            });
        } else if self.blackout && self.eco.money > 0.001 {
            self.blackout = false;
            self.events.push(GameEvent::Blackout(false));
            self.events.push(GameEvent::Toast {
                text: "Courant rétabli. La fabrique repart !".into(),
                severity: Severity::Info,
            });
        }
    }

    // ------------------------------------------------------------------
    //  Actions manuelles (zone de palettes) — disponibles même en blackout
    // ------------------------------------------------------------------

    /// Achète un colis livré sur les palettes.
    pub fn manual_buy_package(&mut self) -> bool {
        if self.floor.len() >= balance::FLOOR_PACKAGE_LIMIT {
            self.events.push(GameEvent::Toast {
                text: "Zone de palettes pleine ! Ouvre ou revends tes colis.".into(),
                severity: Severity::Warn,
            });
            return false;
        }
        let Some(p) = self.eco.buy_package() else {
            self.events.push(GameEvent::NoMoney);
            return false;
        };
        self.floor.push(FloorPackage { id: p.id, contains_ring: p.contains_ring, items: None });
        true
    }

    /// Ouvre un colis fermé des palettes -> liste d'objets (+ carton).
    pub fn manual_open(&mut self, pi: usize) -> Option<Vec<ItemKind>> {
        let contains_ring = {
            let pkg = self.floor.get_mut(pi)?;
            if pkg.items.is_some() {
                return None;
            }
            pkg.contains_ring
        }; // <- fin de l'emprunt mutable avant de tirer le contenu
        let contents = self.roll_package_contents(contains_ring);
        let pkg = self.floor.get_mut(pi)?;
        pkg.items = Some(contents.clone());
        Some(contents)
    }

    /// Vend un objet ouvert. Si c'est LA BAGUE : -1/4 du capital + re-masquage.
    pub fn manual_sell_item(&mut self, pi: usize, ii: usize) -> Option<f64> {
        let kind = *self.floor.get(pi)?.items.as_ref()?.get(ii)?;
        if kind == ItemKind::Bague {
            let penalty = self.eco.on_ring_sold(&mut self.rng);
            self.events.push(GameEvent::RingSold { penalty_eur: penalty });
            self.events.push(GameEvent::Toast {
                text: format!("Tu as VENDU la bague ! Pénalité : −{penalty:.0} €. Elle se recache chez le fournisseur…"),
                severity: Severity::Danger,
            });
            self.remove_floor_item(pi, ii);
            return Some(0.0);
        }
        let v = kind.sell_value() as f64;
        self.eco.earn(v);
        self.remove_floor_item(pi, ii);
        Some(v)
    }

    /// Securise la bague au coffre -> VICTOIRE (action manuelle).
    pub fn manual_secure_ring(&mut self, pi: usize, ii: usize) -> bool {
        let Some(kind) = self.floor.get(pi).and_then(|p| p.items.as_ref()).and_then(|v| v.get(ii)).copied() else {
            return false;
        };
        if kind != ItemKind::Bague {
            return false;
        }
        self.eco.on_ring_secured();
        self.events.push(GameEvent::RingSecured);
        self.remove_floor_item(pi, ii);
        true
    }

    /// Brûle un objet des palettes : carton -> énergie ; bague -> catastrophe ;
    /// autre -> détruit (bête et méchant).
    pub fn manual_burn_item(&mut self, pi: usize, ii: usize) -> bool {
        let Some(kind) = self.floor.get(pi).and_then(|p| p.items.as_ref()).and_then(|v| v.get(ii)).copied() else {
            return false;
        };
        let def = kind.def();
        if def.burnable {
            // Crédit d'énergie : réduit le coût futur (statistique ici,
            // l'incinérateur physique reste le vrai producteur en ligne).
            self.eco.credit_energy(balance::CARDBOARD_ENERGY_KWH);
        } else if kind == ItemKind::Bague {
            self.eco.on_ring_burned(&mut self.rng);
            self.events.push(GameEvent::RingBurned);
            self.events.push(GameEvent::Toast {
                text: "Tu as BRÛLÉ la bague dans un élan de rangement… Elle repart chez le fournisseur.".into(),
                severity: Severity::Danger,
            });
        }
        self.remove_floor_item(pi, ii);
        true
    }

    /// Revend un colis FERMÉ sans l'ouvrir (prix dérisoire).
    /// S'il contenait la bague : elle rejoint les colis non achetés.
    pub fn manual_blind_sell(&mut self, pi: usize) -> bool {
        let Some(pkg) = self.floor.get(pi) else { return false };
        let contains_ring = pkg.contains_ring;
        self.floor.remove(pi);
        self.eco.earn(balance::PACKAGE_BLIND_SELL_EUR);
        if contains_ring {
            self.eco.rehide_ring(&mut self.rng);
            self.events.push(GameEvent::Toast {
                text: "Tu as revendu SANS OUVRIR le colis qui contenait la bague… Elle s'est recachée chez le fournisseur !".into(),
                severity: Severity::Danger,
            });
        }
        true
    }

    /// Retire un objet d'un colis ouvert ; si le colis devient vide
    /// (tout vendu/brûlé), il est automatiquement évacué de la zone.
    fn remove_floor_item(&mut self, pi: usize, ii: usize) {
        let now_empty = {
            let Some(pkg) = self.floor.get_mut(pi) else { return };
            let Some(items) = pkg.items.as_mut() else { return };
            if ii < items.len() {
                items.remove(ii);
            }
            items.is_empty()
        };
        if now_empty {
            self.floor.remove(pi);
        }
    }

    // ------------------------------------------------------------------
    //  Commandes à l'ordinateur (boutique HANGAR-OS)
    // ------------------------------------------------------------------

    /// Commande `qty` exemplaires d'un article : payé MAINTENANT, livré
    /// après le délai catalogue. Renvoie Err si fonds insuffisants.
    pub fn order(&mut self, item: ShopItem, qty: u32) -> Result<(), &'static str> {
        if qty == 0 {
            return Err("Quantité nulle");
        }
        let total = item.cost() * qty as f64;
        if !self.eco.spend(total) {
            self.events.push(GameEvent::NoMoney);
            return Err("Fonds insuffisants");
        }
        let eta = self.elapsed_s + crate::shop::DELIVERY_MACHINE_S;
        self.deliveries.push(Delivery { item: DeliveryKind::Stock(item, qty), eta_s: eta });
        Ok(())
    }

    /// Commande `n` colis payés d'avance, livrés sur la zone de palettes.
    pub fn order_packages(&mut self, n: u64) -> Result<(), &'static str> {
        if n == 0 {
            return Err("Quantité nulle");
        }
        let total = balance::PACKAGE_PRICE_EUR * n as f64;
        if !self.eco.spend(total) {
            self.events.push(GameEvent::NoMoney);
            return Err("Fonds insuffisants");
        }
        let eta = self.elapsed_s
            + crate::shop::DELIVERY_PACKAGE_BASE_S
            + crate::shop::DELIVERY_PACKAGE_PER_UNIT_S * n as f64;
        self.deliveries.push(Delivery { item: DeliveryKind::Packages(n), eta_s: eta });
        Ok(())
    }

    /// (debug/CI) Force toutes les livraisons en cours à arriver maintenant.
    pub fn debug_force_deliveries(&mut self) {
        for d in &mut self.deliveries {
            d.eta_s = self.elapsed_s - 0.01;
        }
    }

    // ------------------------------------------------------------------
    //  Construction / destruction (consomme le stock livré)
    // ------------------------------------------------------------------

    pub fn place_belt(&mut self, x: i32, y: i32, dir: Dir) -> Result<(), &'static str> {
        self.place_shop_item(x, y, ShopItem::Belt, dir)
    }

    pub fn place_shop_item(&mut self, x: i32, y: i32, item: ShopItem, dir: Dir) -> Result<(), &'static str> {
        self.check_placeable(x, y)?;
        let si = item.stock_index();
        if self.stock[si] == 0 {
            return Err("Pas de stock — commande-le à l'ordinateur !");
        }
        let key = cell_key(x, y);
        match item {
            ShopItem::Belt => {
                self.belts.push(BeltState { dir, item: None, removed: false });
                self.belt_cell.push(key);
            }
            ShopItem::Machine(kind) => {
                self.machines.push(MachineState::new(kind, dir));
                self.machine_cell.push(key);
            }
        }
        self.grid.insert(key, match item {
            ShopItem::Belt => CellContent::Belt(self.belts.len() - 1),
            ShopItem::Machine(_) => CellContent::Machine(self.machines.len() - 1),
        });
        self.stock[si] -= 1;
        Ok(())
    }

    /// Compat : place une machine par son type (consomme le stock machine).
    pub fn place_machine(&mut self, x: i32, y: i32, kind: MachineKind, dir: Dir) -> Result<(), &'static str> {
        self.place_shop_item(x, y, ShopItem::Machine(kind), dir)
    }

    /// Démolit une cellule : remboursement de 50 %, l'objet porté est perdu.
    pub fn demolish_cell(&mut self, x: i32, y: i32) -> Result<(), &'static str> {
        let key = cell_key(x, y);
        let content = self.grid.get(&key).copied().ok_or("Cellule vide")?;
        match content {
            CellContent::Belt(bi) => {
                if self.belts[bi].item.is_some() {
                    self.events.push(GameEvent::Toast {
                        text: "L'objet sur le tapis est perdu à la démolition.".into(),
                        severity: Severity::Warn,
                    });
                }
                self.belts[bi].removed = true; // tombe destinée : indices stables
                self.eco.earn(balance::BELT_COST_EUR * balance::DEMOLISH_REFUND);
            }
            CellContent::Machine(mi) => {
                let spec = self.machines[mi].kind.spec();
                self.machines[mi].removed = true;
                self.machines[mi].busy_with = None;
                self.eco.earn(spec.cost * balance::DEMOLISH_REFUND);
            }
        }
        self.grid.remove(&key);
        Ok(())
    }

    fn check_placeable(&self, x: i32, y: i32) -> Result<(), &'static str> {
        if x.abs() > balance::HANGAR_HALF_SIZE || y.abs() > balance::HANGAR_HALF_SIZE {
            return Err("Hors hangar");
        }
        if self.grid.contains_key(&cell_key(x, y)) {
            return Err("Cellule occupée");
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    //  Utilitaires
    // ------------------------------------------------------------------

    /// Contenu d'un colis : 1-3 objets tirés de la table + le carton vide
    /// (toujours dernier, brûlable). La bague ne passe JAMAIS par la table
    /// de loot : si le colis la contient, c'est [Bague, CartonVide].
    fn roll_package_contents(&mut self, contains_ring: bool) -> Vec<ItemKind> {
        let mut out = Vec::with_capacity(4);
        if contains_ring {
            out.push(ItemKind::Bague);
        } else {
            let weights_count: Vec<u32> = PACKAGE_ITEM_COUNT_WEIGHTS.iter().map(|(w, _)| *w).collect();
            let n = PACKAGE_ITEM_COUNT_WEIGHTS[self.rng.pick_weighted(&weights_count)].1;
            let weights_items: Vec<u32> = PACKAGE_ROLL_TABLE.iter().map(|(w, _)| *w).collect();
            for _ in 0..n {
                out.push(PACKAGE_ROLL_TABLE[self.rng.pick_weighted(&weights_items)].1);
            }
        }
        out.push(ItemKind::CartonVide);
        out
    }

    /// Vide la file d'événements (l'UI appelle ça chaque frame).
    pub fn drain_events(&mut self) -> std::vec::Drain<'_, GameEvent> {
        self.events.drain(..)
    }

    /// Statut bague pour le HUD.
    pub fn ring_status_text(&self) -> String {
        match self.eco.ring_state {
            crate::economy::RingState::Secured => "SÉCURISÉE AU COFFRE".into(),
            crate::economy::RingState::Hidden => {
                if self.eco.rings_sold > 0 || self.eco.rings_burned > 0 {
                    format!(
                        "recachée (vendue ×{} / brûlée ×{}), ~{} colis restants devant elle",
                        self.eco.rings_sold,
                        self.eco.rings_burned,
                        format_count(self.eco.ring_slot - self.eco.next_package_id + 1)
                    )
                } else {
                    "cachée quelque part dans le stock…".to_string()
                }
            }
        }
    }
}

/// Décodage d'une clé de cellule.
#[inline]
pub fn cell_xy(key: u64) -> (i32, i32) {
    let x = ((key >> 16) as i32) - 4096;
    let y = ((key & 0xFFFF) as i32) - 4096;
    (x, y)
}

/// Formatage compact 1.2 k / 3.4 M / 5.6 G.
pub fn format_count(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2} G", n as f64 / 1e9)
    } else if n >= 1_000_000 {
        format!("{:.2} M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.1} k", n as f64 / 1e3)
    } else {
        format!("{n}")
    }
}
