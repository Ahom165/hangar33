//! Machines du hangar : état + règles d'acceptation.
//!
//! Chaque machine occupe UNE cellule (v0.1), a une orientation `dir` :
//!   - entrée  = cellule amont, située du côté `dir.opposite()`,
//!   - sortie  = cellule du côté `dir` (ou latérales pour le Trieur).
//!
//! Le comportement de tick est implémenté dans `sim.rs` (systèmes),
//! ici on ne décrit que l'état et le contrat d'acceptation.

use crate::balance;
use crate::grid::{CarriedItem, Dir};
use crate::items::ItemKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MachineKind {
    /// Ouvre les colis : consomme `ColisFerme`, produit 1-3 objets + carton.
    Unpacker,
    /// Vend tout objet qui passe. ⚠️ VENDRA LA BAGUE si elle n'est pas interceptée.
    Seller,
    /// Brûle tout objet : carton -> énergie gratuite, autre -> détruit,
    /// Bague -> catastrophe.
    Incinerator,
    /// Passif : répartit alternativement vers ses deux sorties latérales.
    Splitter,
    /// Détecte la Bague et la place EN SÉCURITÉ -> victoire. Laisse passer le reste.
    RingScanner,
    /// Achète des colis tout seul et les pose devant lui.
    AutoBuyer,
    /// Manipulateur universel : accepte TOUT ce qui arrive (tapis ou
    /// machine amont) et le pousse devant lui (tapis OU machine aval).
    /// Cycle très court : un "tapis coin/inserteur" en 1 cellule.
    RobotArm,
    /// Passif : produit des kW gratuits (voir balance::GENERATOR_FREE_KW).
    Generator,
}

impl MachineKind {
    pub fn spec(self) -> balance::MachineSpec {
        match self {
            MachineKind::Unpacker => balance::UNPACKER,
            MachineKind::Seller => balance::SELLER,
            MachineKind::Incinerator => balance::INCINERATOR,
            MachineKind::Splitter => balance::SPLITTER,
            MachineKind::RingScanner => balance::RING_SCANNER,
            MachineKind::AutoBuyer => balance::AUTO_BUYER,
            MachineKind::RobotArm => balance::ROBOT_ARM,
            MachineKind::Generator => balance::GENERATOR,
        }
    }
}

/// État runtime d'une machine instanciée.
#[derive(Debug, Clone)]
pub struct MachineState {
    pub kind: MachineKind,
    pub dir: Dir,
    /// Temps restant avant fin du cycle en cours (s). 0 = inactif.
    pub cooldown_s: f32,
    /// Objet en cours de traitement (le flag bague voyage avec lui).
    pub busy_with: Option<CarriedItem>,
    /// File de sortie (l'Unpacker produit plusieurs objets d'un coup).
    /// Des CarriedItem complets : le flag bague d'un colis fermé DOIT
    /// survivre au transit (Bras robot, Scanner passe-sauf).
    pub out_queue: [Option<CarriedItem>; 4],
    /// Trieur : 0 = sortie droite, 1 = sortie gauche (alterne).
    pub toggle: u8,
    /// Période de rachat de l'Auto-Acheteur (s) — réglable dans l'UI.
    pub auto_buy_period_s: f32,
    /// Statistiques de vie (affichage panneau machine).
    pub processed: u64,
    /// Tombe de démolition : l'entrée reste pour la stabilité des indices.
    pub removed: bool,
}

impl MachineState {
    pub fn new(kind: MachineKind, dir: Dir) -> Self {
        Self {
            kind,
            dir,
            cooldown_s: 0.0,
            busy_with: None,
            out_queue: [None; 4],
            toggle: 0,
            auto_buy_period_s: balance::AUTO_BUYER.cycle_s,
            processed: 0,
            removed: false,
        }
    }

    /// Un objet de type `kind` peut-il entrer maintenant ?
    /// (appelé par le système de transfert des tapis)
    pub fn accepts(&self, kind: ItemKind) -> bool {
        if self.busy_with.is_some() {
            return false;
        }
        match self.kind {
            MachineKind::Unpacker => kind == ItemKind::ColisFerme,
            // ⚠️ Le guichet vend TOUT, y compris la Bague si elle arrive
            // jusqu'ici (pénalité -25 %). C'est volontaire : le Scanner à
            // bague doit être placé EN AMONT du guichet. Une ligne mal
            // conçue doit pouvoir être coûteuse — c'est le jeu.
            MachineKind::Seller => kind != ItemKind::ColisFerme,
            MachineKind::Incinerator => true,
            MachineKind::Splitter => false, // passif : route directe belt->belt
            MachineKind::RingScanner => true,
            MachineKind::AutoBuyer => false,
            // Le bras robot est un simple manipulateur : tout ce qui arrive,
            // il le re-dirige. (Il peut porter un colis fermé comme une pièce.)
            MachineKind::RobotArm => true,
            MachineKind::Generator => false, // passif
        }
    }
}
