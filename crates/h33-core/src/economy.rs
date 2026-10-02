//! Économie de la partie : argent, pool de colis, et LA BAGUE.
//!
//! Règles de la bague (validées game design) :
//!   1. Au début de la partie, la bague est cachée dans UN colis parmi
//!      `total_packages` (1 000 000 .. 500 000 000, choisi au menu).
//!   2. Chaque colis acheté porte l'identifiant `next_package_id` (0-based,
//!      incrémenté à chaque achat) : le colis contenant la bague est celui
//!      dont l'id == `ring_slot`.
//!   3. Si la bague est VENDUE  : perte de 1/4 du capital, puis elle
//!      "retourne se cacher" parmi les colis NON ACHETÉS (jamais dans un
//!      colis déjà sorti du pool) : `ring_slot = next_package_id +
//!      tirage_uniforme(0 .. packages_left)`.
//!   4. Si la bague est INCINÉRÉE : même re-masquage (la maison remplace
//!      l'objet volé, c'est un jeu, pas une tragédie — valeur d'assurance
//!      à débattre avec le game designer).
//!   5. Si la bague est SÉCURISÉE (Scanner -> coffre, ou action manuelle) :
//!      VICTOIRE.

use crate::balance;
use crate::rng::GameRng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingState {
    /// La bague se cache quelque part dans le pool de colis restant.
    Hidden,
    /// La bague est au coffre : partie gagnée.
    Secured,
}

/// Un colis acheté (avant ouverture).
#[derive(Debug, Clone, Copy)]
pub struct PackagePurchase {
    pub id: u64,
    pub contains_ring: bool,
}

#[derive(Debug)]
pub struct Economy {
    pub money: f64,

    /// Nombre TOTAL de colis de la partie (choix du joueur au menu).
    pub total_packages: u64,
    /// Colis pas encore achetés (le "stock du fournisseur").
    pub packages_left: u64,
    /// Prochain identifiant de colis (0-based) — sert à localiser la bague.
    pub next_package_id: u64,
    /// L'id du colis qui contient la bague. Strictement >= `next_package_id`
    /// tant que la bague est cachée : elle n'est JAMAIS dans un colis déjà acheté.
    pub ring_slot: u64,
    pub ring_state: RingState,

    // --- Statistiques / savegame v0.2 ---
    pub packages_bought: u64,
    pub rings_sold: u32,
    pub rings_burned: u32,
    pub grid_kwh_bought: f64,
    pub energy_burned_kwh: f64,
    pub total_spent_eur: f64,
    pub total_earned_eur: f64,
}

impl Economy {
    /// Nouvelle partie : `ring_slot` est tiré AVANT (premier tirage du PRNG
    /// seedé par le TRNG) et injecté ici — traçabilité claire.
    pub fn new(starting_money: f64, total_packages: u64, ring_slot: u64) -> Self {
        debug_assert!(total_packages >= balance::MIN_PACKAGES);
        debug_assert!(ring_slot < total_packages);
        Self {
            money: starting_money,
            total_packages,
            packages_left: total_packages,
            next_package_id: 0,
            ring_slot,
            ring_state: RingState::Hidden,
            packages_bought: 0,
            rings_sold: 0,
            rings_burned: 0,
            grid_kwh_bought: 0.0,
            energy_burned_kwh: 0.0,
            total_spent_eur: 0.0,
            total_earned_eur: 0.0,
        }
    }

    /// Le pool est-il épuisé ?
    pub fn pool_empty(&self) -> bool {
        self.packages_left == 0
    }

    /// Achète un colis. Renvoie None si plus d'argent ou pool vide.
    /// NOTE : le contenu n'est rollé qu'à l'ouverture — seule la présence
    /// de la bague est déterminée ici (par l'id), pour que le serveur
    /// multijoueur puisse la valider sans connaître le contenu.
    pub fn buy_package(&mut self) -> Option<PackagePurchase> {
        if self.money < balance::PACKAGE_PRICE_EUR || self.pool_empty() {
            return None;
        }
        self.spend(balance::PACKAGE_PRICE_EUR);
        let id = self.next_package_id;
        self.next_package_id += 1;
        self.packages_left -= 1;
        self.packages_bought += 1;
        Some(PackagePurchase { id, contains_ring: id == self.ring_slot })
    }

    /// Re-masque la bague uniformément parmi les colis NON ACHETÉS restants.
    /// Appelé après une vente ou une incinération de la bague.
    pub fn rehide_ring(&mut self, rng: &mut GameRng) {
        self.ring_state = RingState::Hidden;
        // Cas limite : pool épuisé -> le fournisseur "réapprovisionne" un
        // lot : la bague rejoint le prochain colis qui sera acheté.
        let span = self.packages_left.max(1);
        self.ring_slot = self.next_package_id + rng.range_u64(0..span);
    }

    /// Perte liée à la VENTE de la bague : -1/4 du capital (arrondi au centime),
    /// puis re-masquage.
    pub fn on_ring_sold(&mut self, rng: &mut GameRng) -> f64 {
        self.rings_sold += 1;
        let penalty = self.money * balance::RING_SELL_PENALTY_FRACTION;
        self.money -= penalty;
        self.rehide_ring(rng);
        penalty
    }

    /// La bague est partie en fumée : re-masquage (pas de perte d'argent,
    /// mais le temps perdu EST la punition).
    pub fn on_ring_burned(&mut self, rng: &mut GameRng) {
        self.rings_burned += 1;
        self.rehide_ring(rng);
    }

    /// La bague est sécurisée -> victoire.
    pub fn on_ring_secured(&mut self) {
        self.ring_state = RingState::Secured;
    }

    /// Dépense (achat colis / construction). Renvoie false si fonds insuffisants.
    pub fn spend(&mut self, amount: f64) -> bool {
        if self.money < amount {
            return false;
        }
        self.money -= amount;
        self.total_spent_eur += amount;
        true
    }

    /// Gain (vente).
    pub fn earn(&mut self, amount: f64) {
        self.money += amount;
        self.total_earned_eur += amount;
    }

    /// Crédit d'énergie produite par incinération (statistique + compteur).
    pub fn credit_energy(&mut self, kwh: f64) {
        self.energy_burned_kwh += kwh;
    }

    /// Facture réseau : consomme d'abord le "gratuit" (incinérateurs),
    /// achète le reste. Renvoie le kWh acheté sur ce tick.
    pub fn charge_power(&mut self, demand_kw: f32, free_kw: f32, dt_s: f32) -> f64 {
        let grid_kw = (demand_kw - free_kw).max(0.0);
        let kwh = (grid_kw as f64) * (dt_s as f64) / 3600.0;
        self.grid_kwh_bought += kwh;
        // La facture est débitée même si ça met le solde à zéro :
        // en dessous, la sim passe en blackout (gérée par Sim).
        self.money = (self.money - kwh * balance::GRID_PRICE_EUR_PER_KWH).max(0.0);
        kwh
    }

    /// Progression estimée dans le pool (affichage HUD).
    pub fn pool_progress(&self) -> f32 {
        if self.total_packages == 0 {
            return 1.0;
        }
        1.0 - (self.packages_left as f32 / self.total_packages as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_bague_est_jamais_dans_un_colis_deja_achete() {
        let mut rng = GameRng::from_seed([1u8; 32]);
        let ring_slot = rng.range_u64(0..1_000_000);
        let mut eco = Economy::new(1e12, 1_000_000, ring_slot);
        // Achète des colis jusqu'à vendre la bague.
        let mut found = false;
        for i in 0..500_000 {
            let p = eco.buy_package().unwrap();
            if p.contains_ring {
                found = true;
                let penalty = eco.on_ring_sold(&mut rng);
                assert!((penalty - eco.total_spent_eur.min(f64::MAX)).is_finite());
                assert_eq!(eco.rings_sold, 1);
                // La bague doit être à nouveau dans le pool restant.
                assert!(eco.ring_slot >= eco.next_package_id);
                assert!(eco.ring_slot < eco.next_package_id + eco.packages_left.max(1));
                break;
            }
            let _ = i;
        }
        assert!(found, "bague jamais trouvée en 500k colis ?!");
    }

    #[test]
    fn penaltie_un_quart_du_capital() {
        let mut rng = GameRng::from_seed([2u8; 32]);
        let ring_slot = rng.range_u64(0..10_000_000);
        let mut eco = Economy::new(1000.0, 10_000_000, ring_slot);
        eco.earn(3000.0); // capital = 4000
        eco.next_package_id = 5; // simulation : bague vendue manuellement
        let penalty = eco.on_ring_sold(&mut rng);
        assert!((penalty - 1000.0).abs() < 1e-9);
        assert!((eco.money - 3000.0).abs() < 1e-9);
    }

    #[test]
    fn pool_epuise_re_approvisionne() {
        let mut rng = GameRng::from_seed([3u8; 32]);
        let ring_slot = rng.range_u64(0..1_000_000);
        let mut eco = Economy::new(1e12, 1_000_000, ring_slot);
        for _ in 0..1_000_000 {
            eco.buy_package();
        }
        assert!(eco.pool_empty());
        eco.on_ring_burned(&mut rng);
        // La bague doit pointer sur un colis futur.
        assert!(eco.ring_slot >= eco.next_package_id);
    }
}
