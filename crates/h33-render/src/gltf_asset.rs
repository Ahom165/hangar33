//! Chargement des assets GLB (glTF 2.0 binaire) exportés depuis Blender
//! via le pipeline MCP.
//!
//! Convention HANGAR 33 :
//! - unités : mètres ;
//! - up : +Y (l'export Blender glTF convertit Z-up -> Y-up) ;
//! - une machine regarde vers +Z (le modèle Blender pointe vers -Y) ;
//! - un seul `GltfMesh` par fichier : toutes les primitives des nodes sont
//!   fusionnées, transforms de nodes appliqués, couleurs matériaux cuites
//!   dans les teintes de sommets (fake AO par orientation de normale) —
//!   le shader scène reste unique et sans texture.

use glam::Mat4;

/// Mesh fusionné prêt à être envoyé au GPU (`Mesh::from_raw`).
pub struct GltfMesh {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

/// Teinte "fake AO" par orientation : dessus clair, dessous sombre,
/// côtés moyens — même logique que le cube procédural (mesh.rs).
fn face_tint(n: [f32; 3]) -> [f32; 3] {
    let y = n[1];
    if y > 0.7 {
        [1.0, 1.0, 1.0]
    } else if y < -0.4 {
        [0.52, 0.52, 0.56]
    } else {
        // Côtés : légère variation N/S vs E/O pour casser l'uniformité.
        let base = 0.80 + 0.10 * y.max(0.0);
        if n[0].abs() > n[2].abs() {
            [base + 0.04, base + 0.04, base + 0.07]
        } else {
            [base, base, base + 0.03]
        }
    }
}

fn parse_glb(bytes: &[u8]) -> Result<GltfMesh, String> {
    let (document, buffers, _images) =
        gltf::import_slice(bytes).map_err(|e| format!("import_slice: {e:?}"))?;

    let mut out = GltfMesh {
        vertices: Vec::new(),
        normals: Vec::new(),
        colors: Vec::new(),
        indices: Vec::new(),
    };

    let scene = document
        .default_scene()
        .or_else(|| document.scenes().next())
        .ok_or("GLB sans scène")?;

    fn walk(
        node: gltf::Node,
        parent: Mat4,
        out: &mut GltfMesh,
        buffers: &[gltf::buffer::Data],
    ) {
        let local = Mat4::from_cols_array_2d(&node.transform().matrix());
        let model = parent * local;

        if let Some(mesh) = node.mesh() {
            for prim in mesh.primitives() {
                let reader = prim.reader(|buffer| buffers.get(buffer.index()).map(|b| b.0.as_slice()));
                let pos_iter = match reader.read_positions() {
                    Some(p) => p,
                    None => continue, // primitive sans position : ignorer
                };
                let norm_iter = reader.read_normals();
                let base_color: [f32; 4] = prim
                    .material()
                    .pbr_metallic_roughness()
                    .base_color_factor();
                let emissive: [f32; 3] = prim.material().emissive_factor();
                // Couleur matériau : base, rehaussée de l'émissif (lampes).
                let mut mat_rgb = [base_color[0], base_color[1], base_color[2]];
                for i in 0..3 {
                    mat_rgb[i] = mat_rgb[i].max(emissive[i] * 0.9);
                }

                let base = out.vertices.len() as u32;
                let mut prim_pos: Vec<[f32; 3]> = Vec::new();
                for p in pos_iter {
                    let wp = model.transform_point3(glam::Vec3::from(p));
                    prim_pos.push(wp.to_array());
                }
                let count = prim_pos.len();
                if count == 0 {
                    continue;
                }
                let prim_norm: Vec<[f32; 3]> = match norm_iter {
                    Some(it) => {
                        let mut v: Vec<[f32; 3]> = Vec::with_capacity(count);
                        for n in it {
                            let nw = model.transform_vector3(glam::Vec3::from(n));
                            v.push(nw.normalize_or_zero().to_array());
                        }
                        v
                    }
                    None => vec![[0.0, 1.0, 0.0]; count],
                };

                // Teinte par sommet : matériau × fake AO (via normale).
                for (i, p) in prim_pos.into_iter().enumerate() {
                    let n_world = prim_norm[i];
                    let tint = face_tint(n_world);
                    out.vertices.push(p);
                    out.normals.push(n_world);
                    out.colors.push([
                        (mat_rgb[0] * tint[0]).clamp(0.0, 4.0),
                        (mat_rgb[1] * tint[1]).clamp(0.0, 4.0),
                        (mat_rgb[2] * tint[2]).clamp(0.0, 4.0),
                    ]);
                }

                match reader.read_indices() {
                    Some(idx) => {
                        for i in idx.into_u32() {
                            out.indices.push(base + i);
                        }
                    }
                    None => {
                        // Triangles implicites (fan 0-1-2, 0-2-3...).
                        if count >= 3 {
                            for t in 0..(count - 2) as u32 {
                                out.indices.extend_from_slice(&[base, base + t + 1, base + t + 2]);
                            }
                        }
                    }
                }
            }
        }

        for child in node.children() {
            walk(child, model, out, buffers);
        }
    }

    for node in scene.nodes() {
        walk(node, Mat4::IDENTITY, &mut out, &buffers);
    }

    if out.vertices.is_empty() {
        return Err("GLB sans géométrie".into());
    }
    Ok(out)
}

/// Charge un GLB embarqué (include_bytes!) — panic en cas d'asset corrompu
/// (les GLB sont compilés dans le binaire : une erreur est un bug de build).
pub fn load_glb_or_panic(bytes: &[u8], name: &str) -> GltfMesh {
    match parse_glb(bytes) {
        Ok(m) => m,
        Err(e) => panic!("GLB '{name}' invalide : {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_unitaire_manuel() {
        // Le loader ne dépend pas d'asset embarqué : test du tint uniquement.
        let t = face_tint([0.0, 1.0, 0.0]);
        assert!(t[0] > 0.99);
        let t = face_tint([0.0, -1.0, 0.0]);
        assert!(t[0] < 0.6);
    }
}
