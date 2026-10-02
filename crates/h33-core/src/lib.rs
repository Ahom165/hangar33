//! HANGAR 33 — cœur de simulation.
//!
//! Crate 100 % logique, SANS dépendance graphique ni temps réel :
//!  - testable headless (CI, tests de balance),
//!  - réutilisable côté serveur pour le multijoueur futur
//!    (le serveur exécute `Sim::tick` et rejoue les commandes joueurs),
//!  - le rendu (h33-render) et l'app (h33-app) ne font que la consommer.
//!
//! Modules :
//!  - `rng`     : TRNG (seed) + ChaCha8 (gameplay)
//!  - `items`   : catalogue objets & raretés
//!  - `grid`    : grille, tapis, directions
//!  - `machines`: définitions machines
//!  - `economy` : argent, pool de colis, règles de la BAGUE
//!  - `sim`     : tick fixed-timestep, systèmes, actions manuelles
//!  - `ecs`     : mini-ECS pour la couche VFX (voir doc du module)
//!  - `balance` : TOUTES les constantes d'équilibrage

pub mod balance;
pub mod ecs;
pub mod economy;
pub mod grid;
pub mod items;
pub mod machines;
pub mod rng;
pub mod shop;
pub mod sim;

pub use economy::RingState;
pub use rng::GameRng;
pub use sim::{GameEvent, Severity, Sim};
