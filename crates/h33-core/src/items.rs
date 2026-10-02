//! Catalogue des objets pouvant sortir des colis mystères.
//!
//! Design : 12 objets ordinaires + 1 objet unique (la Bague).
//! Les valeurs sont en euros et resservent pour :
//!  - la vente (Guichet ou manuel),
//!  - le réemballage en gros (v0.2),
//!  - l'équilibrage du ratio achat colis / marge.

/// Rareté d'un objet (influence l'affichage, le son et le prix).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rarity {
    Commun,
    PeuCommun,
    Rare,
    Epique,
    Legendaire,
}

impl Rarity {
    /// Couleur de mise en avant UI (RGBA 0..1) — cohérente entre l'UI et le monde 3D.
    pub fn color(self) -> [f32; 4] {
        match self {
            Rarity::Commun => [0.75, 0.75, 0.78, 1.0],
            Rarity::PeuCommun => [0.30, 0.85, 0.45, 1.0],
            Rarity::Rare => [0.30, 0.55, 0.95, 1.0],
            Rarity::Epique => [0.70, 0.35, 0.95, 1.0],
            Rarity::Legendaire => [1.00, 0.72, 0.15, 1.0],
        }
    }
}

/// Identifiant d'objet. `repr(u8)` : compact en mémoire et sérialisable tel quel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ItemKind {
    CartonVide,
    Chaussette,
    Journal,
    Tournevis,
    Bougie,
    Montre,
    ManetteRetro,
    Figurine,
    JeuVideoRare,
    ConsoleRetro,
    VaseAncien,
    BijouFantaisie,
    /// Objet unique de la partie. NE DOIT JAMAIS passer dans un `Seller`
    /// ou un `Incinerator` sans conséquence — c'est le cœur du risque.
    Bague,
    /// Colis fermé circulant sur les tapis (consommé par le Dépaqueteur).
    ColisFerme,
}

/// Définition statique d'un objet.
#[derive(Debug, Clone, Copy)]
pub struct ItemDef {
    pub kind: ItemKind,
    pub name_fr: &'static str,
    pub rarity: Rarity,
    /// Valeur de vente (€). La Bague affiche "INESTIMABLE" côté UI.
    pub value_eur: f32,
    /// Un objet brûlable peut alimenter l'incinérateur (énergie gratuite).
    pub burnable: bool,
}

/// Table statique : index = discriminant de `ItemKind`.
pub const ITEMS: [ItemDef; 14] = [
    ItemDef { kind: ItemKind::CartonVide,    name_fr: "Carton vide",     rarity: Rarity::Commun,     value_eur: 0.2,   burnable: true  },
    ItemDef { kind: ItemKind::Chaussette,    name_fr: "Chaussette orpheline", rarity: Rarity::Commun, value_eur: 0.5,   burnable: true  },
    ItemDef { kind: ItemKind::Journal,       name_fr: "Journal de 1997", rarity: Rarity::Commun,     value_eur: 0.8,   burnable: true  },
    ItemDef { kind: ItemKind::Tournevis,     name_fr: "Tournevis bicolore", rarity: Rarity::Commun,  value_eur: 2.5,   burnable: false },
    ItemDef { kind: ItemKind::Bougie,        name_fr: "Bougie parfumée", rarity: Rarity::PeuCommun,  value_eur: 3.0,   burnable: true  },
    ItemDef { kind: ItemKind::Montre,        name_fr: "Montre à gousset", rarity: Rarity::PeuCommun, value_eur: 18.0,  burnable: false },
    ItemDef { kind: ItemKind::ManetteRetro,  name_fr: "Manette rétro",   rarity: Rarity::PeuCommun,  value_eur: 35.0,  burnable: false },
    ItemDef { kind: ItemKind::Figurine,      name_fr: "Figurine de collection", rarity: Rarity::Rare, value_eur: 60.0, burnable: false },
    ItemDef { kind: ItemKind::JeuVideoRare,  name_fr: "Jeu vidéo rare",  rarity: Rarity::Rare,       value_eur: 140.0, burnable: false },
    ItemDef { kind: ItemKind::ConsoleRetro,  name_fr: "Console rétro",   rarity: Rarity::Epique,     value_eur: 320.0, burnable: false },
    ItemDef { kind: ItemKind::VaseAncien,    name_fr: "Vase ancien",     rarity: Rarity::Epique,     value_eur: 750.0, burnable: false },
    ItemDef { kind: ItemKind::BijouFantaisie,name_fr: "Bijou fantaisie doré", rarity: Rarity::Legendaire, value_eur: 1500.0, burnable: false },
    ItemDef { kind: ItemKind::Bague,         name_fr: "LA BAGUE",        rarity: Rarity::Legendaire, value_eur: f32::INFINITY, burnable: false },
    ItemDef { kind: ItemKind::ColisFerme,    name_fr: "Colis fermé",     rarity: Rarity::Commun,     value_eur: 0.0,   burnable: false },
];

impl ItemKind {
    #[inline]
    pub fn def(self) -> &'static ItemDef {
        &ITEMS[self as usize]
    }

    /// Valeur de vente. La Bague n'a pas de prix : la vendre déclenche la pénalité.
    #[inline]
    pub fn sell_value(self) -> f32 {
        self.def().value_eur
    }
}

/// Table de loot pondérée d'un colis ORDINAIRE (sans la bague).
/// (poids, ItemKind) — les poids n'ont pas besoin de sommer à 1.
pub const PACKAGE_ROLL_TABLE: [(u32, ItemKind); 11] = [
    (22,   ItemKind::Chaussette),
    (18,   ItemKind::Journal),
    (12,   ItemKind::Tournevis),
    (10,   ItemKind::Bougie),
    (6,    ItemKind::Montre),
    (4,    ItemKind::ManetteRetro),
    (2,    ItemKind::Figurine),
    (1,    ItemKind::JeuVideoRare),
    (1,    ItemKind::ConsoleRetro),
    (1,    ItemKind::VaseAncien),
    (1,    ItemKind::BijouFantaisie),
];

/// Probabilités du nombre d'objets par colis (hors carton).
/// 70 % : 1 objet, 25 % : 2 objets, 5 % : 3 objets.
pub const PACKAGE_ITEM_COUNT_WEIGHTS: [(u32, u8); 3] = [(70, 1), (25, 2), (5, 3)];
