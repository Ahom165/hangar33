//! Mini-ECS (sparse-set) pour la couche "props & VFX" (particules de
//! déballage, projections d'objets, effets).
//!
//! ⚠️ Arbitrage d'architecture assumé : la LOGIQUE DE JEU (tapis, machines,
//! économie) n'utilise PAS cet ECS générique — elle repose sur des `Vec`
//! typés + slots par tapis (voir `grid.rs`), ce qui est l'optimisation
//! correcte pour de la logistique type Factorio (zéro collision, zéro
//! indirection). L'ECS sert aux couches où les entités sont hétérogènes et
//! nombreuses : effets visuels, sons, projections physiques cosmétiques,
//! et tout ce que le serveur multijoueur n'a pas besoin de répliquer.

use std::collections::HashMap;
use std::marker::PhantomData;

/// Identifiant d'entité : index + génération (évite les use-after-free
/// logiques quand une entité est recyclée).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Entity {
    pub index: u32,
    pub generation: u32,
}

/// World : registre de composants + gestion des entités.
pub struct World {
    next_index: u32,
    generation: Vec<u32>,
    free: Vec<u32>,
    storages: HashMap<&'static str, Box<dyn Storage>>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    pub fn new() -> Self {
        Self {
            next_index: 0,
            generation: Vec::new(),
            free: Vec::new(),
            storages: HashMap::new(),
        }
    }

    /// Crée une entité (recycle les indices morts avec une génération ++).
    pub fn spawn(&mut self) -> Entity {
        if let Some(i) = self.free.pop() {
            self.generation[i as usize] += 1;
            Entity { index: i, generation: self.generation[i as usize] }
        } else {
            let i = self.next_index;
            self.next_index += 1;
            self.generation.push(0);
            Entity { index: i, generation: 0 }
        }
    }

    /// Détruit une entité : retire ses composants de tous les storages.
    pub fn despawn(&mut self, e: Entity) {
        if !self.is_alive(e) {
            return;
        }
        for st in self.storages.values_mut() {
            st.remove(e);
        }
        self.free.push(e.index);
        // NOTE : generation++ a lieu au respawn (spawn) — pattern simple.
    }

    #[inline]
    pub fn is_alive(&self, e: Entity) -> bool {
        (self.generation.get(e.index as usize) == Some(&e.generation))
            && !self.free.contains(&e.index)
    }

    /// Insère un composant sur une entité.
    pub fn insert<C: 'static>(&mut self, e: Entity, comp: C) {
        self.storage::<C>().insert(e, comp);
    }

    /// Récupère un composant (lecture).
    pub fn get<C: 'static>(&self, e: Entity) -> Option<&C> {
        self.storages
            .get(std::any::type_name::<C>())
            .and_then(|s| s.get(e))
            .and_then(|any| any.downcast_ref::<C>())
    }

    /// Récupère un composant (écriture).
    pub fn get_mut<C: 'static>(&mut self, e: Entity) -> Option<&mut C> {
        self.storages
            .get_mut(std::any::type_name::<C>())
            .and_then(|s| s.get_mut(e))
            .and_then(|any| any.downcast_mut::<C>())
    }

    /// Itère sur toutes les entités portant le composant C.
    pub fn iter<C: 'static>(&self) -> impl Iterator<Item = (Entity, &C)> {
        self.storages
            .get(std::any::type_name::<C>())
            .map(|s| s.entities())
            .unwrap_or_default()
            .into_iter()
            .filter_map(move |e| {
                let c = self.get::<C>(e)?;
                Some((e, c))
            })
    }

    fn storage<C: 'static>(&mut self) -> &mut SparseSet<C> {
        self.storages
            .entry(std::any::type_name::<C>())
            .or_insert_with(|| Box::new(SparseSet::<C>::new()))
            .as_any_mut()
            .downcast_mut::<SparseSet<C>>()
            .expect("type de storage invalide")
    }
}

// ---------------------------------------------------------------------
//  Storage interne : type-erased
// ---------------------------------------------------------------------

trait Storage {
    #[allow(dead_code)] // réservé pour l'insertion dynamique (VFX v0.2)
    fn insert_any(&mut self, e: Entity, comp: Box<dyn std::any::Any>);
    fn remove(&mut self, e: Entity);
    fn get(&self, e: Entity) -> Option<&dyn std::any::Any>;
    fn get_mut(&mut self, e: Entity) -> Option<&mut dyn std::any::Any>;
    fn entities(&self) -> Vec<Entity>;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

struct SparseSet<C> {
    sparse: HashMap<Entity, usize>,
    dense: Vec<(Entity, C)>,
    _marker: PhantomData<C>,
}

impl<C: 'static> SparseSet<C> {
    fn new() -> Self {
        Self { sparse: HashMap::new(), dense: Vec::new(), _marker: PhantomData }
    }

    fn insert(&mut self, e: Entity, c: C) {
        if let Some(&i) = self.sparse.get(&e) {
            self.dense[i].1 = c;
        } else {
            self.sparse.insert(e, self.dense.len());
            self.dense.push((e, c));
        }
    }
}

impl<C: 'static> Storage for SparseSet<C> {
    fn insert_any(&mut self, e: Entity, comp: Box<dyn std::any::Any>) {
        if let Ok(c) = comp.downcast::<C>() {
            self.insert(e, *c);
        }
    }

    fn remove(&mut self, e: Entity) {
        if let Some(i) = self.sparse.remove(&e) {
            self.dense.swap_remove(i);
            if let Some(&(last_e, _)) = self.dense.get(i) {
                self.sparse.insert(last_e, i);
            }
        }
    }

    fn get(&self, e: Entity) -> Option<&dyn std::any::Any> {
        self.sparse.get(&e).map(|&i| &self.dense[i].1 as &dyn std::any::Any)
    }

    fn get_mut(&mut self, e: Entity) -> Option<&mut dyn std::any::Any> {
        let i = *self.sparse.get(&e)?;
        Some(&mut self.dense[i].1 as &mut dyn std::any::Any)
    }

    fn entities(&self) -> Vec<Entity> {
        self.dense.iter().map(|(e, _)| *e).collect()
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ---------------------------------------------------------------------
//  Tests
// ---------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(PartialEq, Debug)]
    struct Position(f32, f32);
    struct Particle(u8);

    #[test]
    fn cycle_de_vie_complet() {
        let mut w = World::new();
        let a = w.spawn();
        w.insert(a, Position(1.0, 2.0));
        w.insert(a, Particle(3));

        assert!(w.is_alive(a));
        assert_eq!(w.get::<Position>(a), Some(&Position(1.0, 2.0)));

        w.despawn(a);
        assert!(!w.is_alive(a));
        assert_eq!(w.get::<Position>(a), None);

        // Recyclage : nouvelle entité, nouvelle génération.
        let b = w.spawn();
        assert_ne!(a.generation, b.generation);
        assert_eq!(w.iter::<Particle>().count(), 0);
    }

    #[test]
    fn iteration_par_composant() {
        let mut w = World::new();
        let e1 = w.spawn();
        let e2 = w.spawn();
        let e3 = w.spawn();
        w.insert(e1, Position(0.0, 0.0));
        w.insert(e2, Position(1.0, 0.0));
        w.insert(e3, Particle(1)); // e3 n'a pas de Position
        let positions: Vec<_> = w.iter::<Position>().map(|(_, p)| p.0).collect();
        assert_eq!(positions, vec![0.0, 1.0]);
    }
}
