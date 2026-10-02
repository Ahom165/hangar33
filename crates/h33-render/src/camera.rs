//! Caméras : orbitale (style éditeur, gardée pour debug/tests) et
//! première personne (le mode de jeu — hauteur des yeux, mouse look).

use glam::{Mat4, Vec3, Vec4};

#[derive(Debug, Clone, Copy)]
pub struct FpCamera {
    /// Position des YEUX (1,7 m au-dessus du sol).
    pub pos: Vec3,
    /// Rotation horizontale (rad). 0 -> regarde +X ; croît en tournant
    /// vers +Z (sens horaire vu de dessus).
    pub yaw: f32,
    /// Tangage (rad) : positif = regarde vers le haut.
    pub pitch: f32,
    pub aspect: f32,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

impl FpCamera {
    pub const EYE_HEIGHT: f32 = 1.7;

    pub fn new_spawn(pos: Vec3, yaw: f32) -> Self {
        Self {
            pos,
            yaw,
            pitch: 0.0,
            aspect: 16.0 / 9.0,
            fov_y: 72.0_f32.to_radians(),
            near: 0.08,
            far: 220.0,
        }
    }

    /// Direction de vue (normalisée).
    pub fn forward(&self) -> Vec3 {
        let (sin_p, cos_p) = self.pitch.sin_cos();
        let (sin_y, cos_y) = self.yaw.sin_cos();
        Vec3::new(cos_p * cos_y, sin_p, cos_p * sin_y)
    }

    /// Vecteur "droite" horizontal (pour le déplacement ZQSD).
    /// Convention look_at_rh : right = forward_h × up.
    pub fn right(&self) -> Vec3 {
        let (sin_y, cos_y) = self.yaw.sin_cos();
        Vec3::new(-sin_y, 0.0, cos_y)
    }

    pub fn view_proj(&self) -> (Mat4, Mat4) {
        let view = Mat4::look_at_rh(self.pos, self.pos + self.forward(), Vec3::Y);
        // perspective_rh : NDC z ∈ [0,1] — le format attendu par wgpu.
        let proj = Mat4::perspective_rh(self.fov_y, self.aspect, self.near, self.far);
        (view, proj)
    }

    pub fn view_proj_mat(&self) -> Mat4 {
        let (view, proj) = self.view_proj();
        proj * view
    }

    /// Rayon monde depuis des coordonnées écran en pixels.
    pub fn screen_ray(&self, px: f32, py: f32, width: f32, height: f32) -> (Vec3, Vec3) {
        let ndc_x = (px / width) * 2.0 - 1.0;
        let ndc_y = 1.0 - (py / height) * 2.0;
        let vp = self.view_proj_mat();
        let inv = vp.inverse();
        let near = inv * Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
        let far = inv * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
        let a = near.truncate() / near.w;
        let b = far.truncate() / far.w;
        (a, (b - a).normalize())
    }

    /// Rayon au centre de l'écran (crosshair, mode pointer lock).
    pub fn center_ray(&self) -> (Vec3, Vec3) {
        (self.pos, self.forward())
    }
}

impl Default for FpCamera {
    fn default() -> Self {
        // Vue de spawn : allée est, regard vers le centre de l'atelier.
        Self::new_spawn(Vec3::new(14.0, Self::EYE_HEIGHT, -12.0), std::f32::consts::FRAC_PI_4 * 3.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OrbitCamera {
    /// Point visé (centre du hangar au départ).
    pub target: Vec3,
    pub distance: f32,
    /// Angle horizontal (rad) et vertical (rad, borné).
    pub yaw: f32,
    pub pitch: f32,
    pub aspect: f32,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 34.0,
            yaw: 0.6,
            pitch: 0.9,
            aspect: 16.0 / 9.0,
            fov_y: 50.0_f32.to_radians(),
            near: 0.1,
            far: 300.0,
        }
    }
}

impl OrbitCamera {
    pub fn eye(&self) -> Vec3 {
        let (sin_p, cos_p) = self.pitch.sin_cos();
        let (sin_y, cos_y) = self.yaw.sin_cos();
        let dir = Vec3::new(cos_p * sin_y, sin_p, cos_p * cos_y);
        self.target + dir * self.distance
    }

    pub fn view_proj(&self) -> (Mat4, Mat4) {
        let eye = self.eye();
        let view = Mat4::look_at_rh(eye, self.target, Vec3::Y);
        // perspective_rh : NDC z ∈ [0,1] — le format attendu par wgpu.
        let proj = Mat4::perspective_rh(self.fov_y, self.aspect, self.near, self.far);
        (view, proj)
    }

    pub fn view_proj_mat(&self) -> Mat4 {
        let (view, proj) = self.view_proj();
        proj * view
    }

    /// Rayon monde depuis des coordonnées écran en pixels.
    pub fn screen_ray(&self, px: f32, py: f32, width: f32, height: f32) -> (Vec3, Vec3) {
        let ndc_x = (px / width) * 2.0 - 1.0;
        let ndc_y = 1.0 - (py / height) * 2.0;
        let vp = self.view_proj_mat();
        let inv = vp.inverse();
        let near = inv * Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
        let far = inv * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
        let a = near.truncate() / near.w;
        let b = far.truncate() / far.w;
        let origin = a;
        let dir = (b - a).normalize();
        (origin, dir)
    }
}
