//! Picking : rayon écran -> cellule de grille.
//!
//! v0.1 : les machines/tapis sont des AABB alignées sur axes, le test
//! slab-method suffit et coûte quelques ns par cellule testée (on ne teste
//! que la cellule sous le curseur via plan sol, donc O(1)).

use glam::Vec3;

use crate::camera::{FpCamera, OrbitCamera};

/// Un rayon monde.
#[derive(Debug, Clone, Copy)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

impl OrbitCamera {
    pub fn ray_at(&self, px: f32, py: f32, width: f32, height: f32) -> Ray {
        let (origin, dir) = self.screen_ray(px, py, width, height);
        Ray { origin, dir }
    }
}

impl FpCamera {
    pub fn ray_at(&self, px: f32, py: f32, width: f32, height: f32) -> Ray {
        let (origin, dir) = self.screen_ray(px, py, width, height);
        Ray { origin, dir }
    }

    /// Rayon du crosshair (centre de l'écran) — mode pointer lock.
    pub fn ray_center(&self) -> Ray {
        let (origin, dir) = self.center_ray();
        Ray { origin, dir }
    }
}

impl Ray {
    /// Intersection avec le plan sol y = 0.
    pub fn ground_hit(&self) -> Option<Vec3> {
        if self.dir.y.abs() < 1e-6 {
            return None;
        }
        let t = -self.origin.y / self.dir.y;
        if t < 0.0 {
            return None;
        }
        Some(self.origin + self.dir * t)
    }

    /// Test rayon-AABB (slab method). Renvoie la distance d'entrée.
    pub fn hit_aabb(&self, min: Vec3, max: Vec3) -> Option<f32> {
        let inv = 1.0 / self.dir;
        let t0 = (min - self.origin) * inv;
        let t1 = (max - self.origin) * inv;
        let tmin = t0.min(t1);
        let tmax = t0.max(t1);
        let t_enter = tmin.max_element().max(0.0);
        let t_exit = tmax.min_element();
        if t_enter <= t_exit && t_exit >= 0.0 {
            Some(t_enter)
        } else {
            None
        }
    }

    /// Cellule de grille visée (le curseur touche le plan sol, on arrondit).
    /// Renvoie (x, y) de cellule.
    pub fn pick_cell(&self, half_size: i32) -> Option<(i32, i32)> {
        let p = self.ground_hit()?;
        let x = p.x.round() as i32;
        let y = p.z.round() as i32;
        if x.abs() <= half_size && y.abs() <= half_size {
            Some((x, y))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rayon_sol_et_cellule() {
        let ray = Ray { origin: Vec3::new(0.0, 10.0, 0.0), dir: Vec3::new(0.0, -1.0, 0.0) };
        let hit = ray.ground_hit().unwrap();
        assert!(hit.x.abs() < 1e-5 && hit.z.abs() < 1e-5);
        assert_eq!(ray.pick_cell(24), Some((0, 0)));
    }

    #[test]
    fn aabb_hit_miss() {
        let ray = Ray { origin: Vec3::new(-10.0, 0.5, 0.0), dir: Vec3::new(1.0, 0.0, 0.0) };
        assert!(ray.hit_aabb(Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 1.0, 0.5)).is_some());
        let ray2 = Ray { origin: Vec3::new(-10.0, 5.0, 0.0), dir: Vec3::new(1.0, 0.0, 0.0) };
        assert!(ray2.hit_aabb(Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 1.0, 0.5)).is_none());
    }
}
