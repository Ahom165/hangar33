//! ============================================================
//!  HANGAR 33 — Constantes d'équilibrage (single source of truth)
//! ============================================================
//! Toutes les valeurs "design" sont ici : itérer = modifier CE fichier.
//! (En v0.2 elles passeront en data files chargées au boot, mais pour
//! itérer avec le game designer, du Rust simple est plus rapide.)

/// Prix d'achat d'un colis mystère (€).
pub const PACKAGE_PRICE_EUR: f64 = 12.0;

/// Prix d'un tapis roulant par cellule (€).
pub const BELT_COST_EUR: f64 = 2.0;

/// Fraction remboursée à la démolition (50 %).
pub const DEMOLISH_REFUND: f64 = 0.5;

/// Puissance électrique gratuite produite par un incinérateur en train de
/// brûler (kW). L'électricité "risquée" : gratuite tant que le tri est bon.
pub const INCINERATOR_FREE_KW: f32 = 2.0;

/// Prix de revente "à l'aveugle" d'un colis fermé (sans l'ouvrir).
pub const PACKAGE_BLIND_SELL_EUR: f64 = 7.0;

/// Électricité réseau (€ / kWh). L'option "sûre" : aucun risque, un coût.
pub const GRID_PRICE_EUR_PER_KWH: f64 = 0.15;

/// Énergie produite par carton vide incinéré (kWh). L'option "risquée".
pub const CARDBOARD_ENERGY_KWH: f64 = 0.05;

/// Fraction du cash perdu si la BAGUE est vendue par erreur.
/// (règle game design validée : 1/4 du capital)
pub const RING_SELL_PENALTY_FRACTION: f64 = 0.25;

/// Nombre de colis total de la partie — choisi au démarrage.
pub const MIN_PACKAGES: u64 = 1_000_000;
pub const MAX_PACKAGES: u64 = 500_000_000;

/// Argent de départ (€).
pub const STARTING_MONEY_EUR: f64 = 500.0;

/// Dimensions du hangar en cellules (grille 1 m x 1 m).
pub const HANGAR_HALF_SIZE: i32 = 24;

/// --- Tapis roulants ---
/// Vitesse d'un tapis (cellules / seconde).
pub const BELT_SPEED_CELLS_PER_S: f32 = 1.0;
/// Consommation d'un tapis (kW) — un tapis seul est négligeable,
/// 200 tapis, ça se sent.
pub const BELT_KW: f32 = 0.05;

/// --- Machines : (prix €, puissance kW, durée de cycle s) ---
pub const UNPACKER: MachineSpec = MachineSpec { cost: 250.0, kw: 1.5, cycle_s: 2.5, name: "Dépaqueteur" };
pub const SELLER: MachineSpec = MachineSpec { cost: 300.0, kw: 1.0, cycle_s: 0.5, name: "Guichet de vente" };
pub const INCINERATOR: MachineSpec = MachineSpec { cost: 150.0, kw: 0.2, cycle_s: 1.0, name: "Incinérateur" };
pub const SPLITTER: MachineSpec = MachineSpec { cost: 120.0, kw: 0.0, cycle_s: 0.0, name: "Trieur 2 voies" };
pub const RING_SCANNER: MachineSpec = MachineSpec { cost: 800.0, kw: 2.0, cycle_s: 0.3, name: "Scanner à bague" };
pub const AUTO_BUYER: MachineSpec = MachineSpec { cost: 600.0, kw: 3.0, cycle_s: 2.0, name: "Auto-Acheteur" };
/// Bras robot : manipulateur universel — accepte TOUT, tourne dans les
/// coins, insère direct dans les machines. Un tapis rapide en format 1x1.
pub const ROBOT_ARM: MachineSpec = MachineSpec { cost: 180.0, kw: 0.8, cycle_s: 0.25, name: "Bras robot" };
/// Générateur diesel : kW gratuits en continu (pas de facture réseau),
/// mais consomme un peu pour son propre balayage.
pub const GENERATOR: MachineSpec = MachineSpec { cost: 400.0, kw: 0.5, cycle_s: 0.0, name: "Générateur" };
/// Puissance gratuite fournie par chaque générateur actif (kW).
pub const GENERATOR_FREE_KW: f32 = 6.0;

/// Intervalle de rachat de l'Auto-Acheteur (bornes du réglage in-game, en secondes).
pub const AUTO_BUY_PERIOD_MIN_S: f32 = 0.5;
pub const AUTO_BUY_PERIOD_MAX_S: f32 = 10.0;

/// Limite de colis posés dans la zone de palettes (déballage manuel).
pub const FLOOR_PACKAGE_LIMIT: usize = 24;

/// Position Z (profondeur) de la ZONE DE PALETTES : DANS le hangar, contre
/// le mur sud (les murs sont à ±HANGAR_HALF_SIZE). Avant v0.3.7 le dock
/// était À L'EXTÉRIEUR (z = half + 3) : les colis « livrés » étaient
/// invisibles derrière le mur — d'où « les colis ne se livrent pas ».
pub const DOCK_ZONE_Z: f32 = 20.5;

/// Cycle de traitement par la machine : distance visuelle [0..1] par seconde
/// d'avancement du bras / de l'animation.
pub const BELT_SLOT_VIS_SPEED: f32 = 1.0;

/// Rayon (m) d'interaction avec l'ordinateur du hangar (touche E).
pub const COMPUTER_INTERACT_RADIUS_M: f32 = 2.6;

#[derive(Debug, Clone, Copy)]
pub struct MachineSpec {
    pub cost: f64,
    pub kw: f32,
    pub cycle_s: f32,
    pub name: &'static str,
}
