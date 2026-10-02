//! Grille logistique du hangar : cellules 1 m x 1 m.
//!
//! Choix d'architecture (Factorio-like) : les objets sur tapis NE SONT PAS
//! des entités physiques libres mais des *slots* appartenant à leur tapis
//! (1 slot par cellule en v0.1). C'est l'optimisation historique de Factorio :
//! des tapis denses (des milliers d'objets) sans détection de collisions,
//! avec des itérations cache-friendly.
//!
//! Déterminisme : les tapis et machines sont stockés dans des `Vec` ordonnés
//! par insertion + une `HashMap` d'indexation. Le tick itère TOUJOURS dans
//! le même ordre -> simulation reproductible (pré-requis multijoueur
//! serveur autoritaire).

/// Orientation cardinale. `North` = -Y grille (vers le fond du hangar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Dir {
    North,
    East,
    South,
    West,
}

impl Dir {
    /// Vecteur d'une cellule dans cette direction.
    #[inline]
    pub fn delta(self) -> (i32, i32) {
        match self {
            Dir::North => (0, -1),
            Dir::East => (1, 0),
            Dir::South => (0, 1),
            Dir::West => (-1, 0),
        }
    }

    #[inline]
    pub fn opposite(self) -> Dir {
        match self {
            Dir::North => Dir::South,
            Dir::East => Dir::West,
            Dir::South => Dir::North,
            Dir::West => Dir::East,
        }
    }

    /// Tourne de 90° horaire (touche R).
    #[inline]
    pub fn rotate_cw(self) -> Dir {
        match self {
            Dir::North => Dir::East,
            Dir::East => Dir::South,
            Dir::South => Dir::West,
            Dir::West => Dir::North,
        }
    }

    /// Sortie latérale droite relative à l'orientation.
    #[inline]
    pub fn right(self) -> Dir {
        self.rotate_cw()
    }

    /// Sortie latérale gauche relative à l'orientation.
    #[inline]
    pub fn left(self) -> Dir {
        self.rotate_cw().rotate_cw().rotate_cw()
    }

    pub const ALL: [Dir; 4] = [Dir::North, Dir::East, Dir::South, Dir::West];
}

/// Clé de cellule compacte : grille bornée à ±4096 cellules,
/// (x, y) tient en 16 bits chacun -> clé u64 triable.
#[inline]
pub fn cell_key(x: i32, y: i32) -> u64 {
    debug_assert!(x >= -4096 && x < 4096 && y >= -4096 && y < 4096, "hors grille");
    (((x + 4096) as u64) << 16) | ((y + 4096) as u64)
}

/// Objet transporté sur un tapis.
#[derive(Debug, Clone, Copy)]
pub struct CarriedItem {
    pub kind: crate::items::ItemKind,
    /// Progression dans la cellule [0..1). >= 1 -> tente le transfert.
    pub progress: f32,
    /// Pour `ColisFerme` uniquement : ce colis contient-il LA bague ?
    /// (le flag voyage avec le colis, l'Unpacker le consomme à l'ouverture)
    pub contains_ring: bool,
}

/// État d'une cellule-tapis. `removed` = tombe (démolition) : on garde
/// l'entrée pour préserver la stabilité des indices, elle est sautée par
/// tous les systèmes et invisible au rendu.
#[derive(Debug, Clone)]
pub struct BeltState {
    pub dir: Dir,
    /// Slot unique de la cellule (modèle Factorio).
    pub item: Option<CarriedItem>,
    pub removed: bool,
}

/// Occupation d'une cellule : index dans `Sim::belts` / `Sim::machines`.
#[derive(Debug, Clone, Copy)]
pub enum CellContent {
    Belt(usize),
    Machine(usize),
}
