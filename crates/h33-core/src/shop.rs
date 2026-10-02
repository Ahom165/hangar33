//! Boutique HANGAR-OS : catalogue commandable depuis l'ordinateur du hangar.
//!
//! Modèle économique "Find The Needle" : l'argent est dépensé AU MOMENT DE
//! LA COMMANDE (sur l'ordinateur), la pièce est LIVRÉE après un délai, et
//! le placement sur la grille consomme le stock livré (gratuit).
//! La démolition rembourse toujours 50 % en argent.

use crate::machines::MachineKind;

/// Article commandable à l'ordinateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShopItem {
    Belt,
    Machine(MachineKind),
}

impl ShopItem {
    pub fn cost(self) -> f64 {
        match self {
            ShopItem::Belt => crate::balance::BELT_COST_EUR,
            ShopItem::Machine(k) => k.spec().cost,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ShopItem::Belt => "Tapis roulant",
            ShopItem::Machine(k) => k.spec().name,
        }
    }

    /// Clé stable dans le stock (`Sim::stock[stock_index()]`).
    pub fn stock_index(self) -> usize {
        ALL_SHOP_ITEMS.iter().position(|i| *i == self).expect("item hors catalogue")
    }
}

/// Catalogue complet (ordre du build bar / du stock).
pub const ALL_SHOP_ITEMS: [ShopItem; 9] = [
    ShopItem::Belt,
    ShopItem::Machine(MachineKind::AutoBuyer),
    ShopItem::Machine(MachineKind::Unpacker),
    ShopItem::Machine(MachineKind::Seller),
    ShopItem::Machine(MachineKind::Splitter),
    ShopItem::Machine(MachineKind::Incinerator),
    ShopItem::Machine(MachineKind::RingScanner),
    ShopItem::Machine(MachineKind::RobotArm),
    ShopItem::Machine(MachineKind::Generator),
];

/// Une livraison en cours : article + quantité + heure d'arrivée (sim).
#[derive(Debug, Clone)]
pub struct Delivery {
    pub item: DeliveryKind,
    /// Instant (elapsed_s) d'arrivée.
    pub eta_s: f64,
}

/// Contenu d'une livraison : pièce du catalogue, ou colis mystères.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryKind {
    Stock(ShopItem, u32),
    /// Colis payés d'avance, déposés sur la zone de palettes.
    Packages(u64),
}

/// Délais de livraison (s) — l'ordinateur affiche "livraison prévue".
pub const DELIVERY_MACHINE_S: f64 = 8.0;
pub const DELIVERY_BELT_BATCH_S: f64 = 6.0;
/// Délai de base d'une commande de colis + délai PAR colis (convoyage).
pub const DELIVERY_PACKAGE_BASE_S: f64 = 3.0;
pub const DELIVERY_PACKAGE_PER_UNIT_S: f64 = 0.8;
