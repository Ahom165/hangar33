//! Données GPU : uniform caméra + instances.
//!
//! Toutes les structures sont `Pod`/`Zeroable` et alignées sur le layout
//! WGSL — le rendu instancié envoie TOUT le hangar en un seul draw call.

use glam::{Mat4, Vec3};

/// Uniform caméra (bind group 0). `cam_pos.w` non utilisé (alignement 16 o).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub cam_pos: [f32; 4],
}

impl CameraUniform {
    pub fn from_mat(view_proj: Mat4, eye: Vec3) -> Self {
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            cam_pos: [eye.x, eye.y, eye.z, 1.0],
        }
    }
}

/// Une instance = une matrice modèle + teinte RGBA.
/// ~80 octets ; 10 000 objets = 800 Ko/frame, largement re-uploadable.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    pub model: [[f32; 4]; 4],
    pub color: [f32; 4],
}

impl InstanceRaw {
    /// Construit une instance à partir d'une position, d'un yaw, d'un
    /// échelle et d'une couleur — cas d'usage 100 % du jeu v0.1.
    pub fn new(pos: Vec3, yaw_rad: f32, scale: Vec3, color: [f32; 4]) -> Self {
        let model = Mat4::from_translation(pos)
            * Mat4::from_rotation_y(yaw_rad)
            * Mat4::from_scale(scale);
        Self { model: model.to_cols_array_2d(), color }
    }

    /// Instance sans rotation (cases, sols, cartons).
    pub fn flat(pos: Vec3, scale: Vec3, color: [f32; 4]) -> Self {
        Self::new(pos, 0.0, scale, color)
    }
}
