//! Test GPU de la vue première personne : rendu du hangar complet
//! (assets GLB Blender + sol grille + machines) depuis la hauteur des
//! yeux, assertions sur les pixels (sol sombre quadrillé, pas la dalle
//! béton claire) et PNG d'inspection.
//!
//! Exécution avec lavapipe :
//! ```sh
//! source scripts/dev-env.sh && cargo test -p h33-render --test fp_diag
//! ```

use h33_render::assets::GpuAssets;
use h33_render::camera::FpCamera;
use h33_render::renderer::{GpuCtx, MeshBatch, SceneRenderer, render_offscreen_rgba8, save_png};
use h33_render::types::{CameraUniform, InstanceRaw};

/// Scène FP : hangar + dépaqueteur + colis, spawn par défaut.
fn rendu_fp(gpu: &GpuCtx, scene: &mut SceneRenderer, shell_seul: bool) -> (Vec<u8>, u32, u32) {
    let assets = GpuAssets::load(&gpu.device);

    let shell = vec![InstanceRaw::flat(glam::Vec3::ZERO, glam::Vec3::ONE, [1.0; 4])];
    let unpacker = if shell_seul {
        Vec::new()
    } else {
        vec![InstanceRaw::new(
            glam::Vec3::new(-2.0, 0.0, 0.0),
            (1.0f32).atan2(0.0),
            glam::Vec3::ONE,
            [1.0; 4],
        )]
    };
    let package = if shell_seul {
        Vec::new()
    } else {
        vec![InstanceRaw::new(
            glam::Vec3::new(0.5, 0.12, 0.0),
            0.4,
            glam::Vec3::ONE,
            [1.0; 4],
        )]
    };

    let (w, h) = (960u32, 540u32);
    let cam = FpCamera { aspect: w as f32 / h as f32, ..FpCamera::default() };
    let cam_uniform = CameraUniform::from_mat(cam.view_proj_mat(), cam.pos);

    let batches = [
        MeshBatch { mesh: &assets.shell, instances: &shell },
        MeshBatch { mesh: &assets.machines[0], instances: &unpacker },
        MeshBatch { mesh: &assets.package, instances: &package },
    ];

    let pixels = render_offscreen_rgba8(gpu, scene, w, h, &cam_uniform, &[], &batches);
    (pixels, w, h)
}

fn probe(pixels: &[u8], w: u32, h: u32, fx: f32, fy: f32) -> [u8; 3] {
    let px = (fx * (w - 1) as f32) as usize;
    let py = (fy * (h - 1) as f32) as usize;
    let i = (py * w as usize + px) * 4;
    [pixels[i], pixels[i + 1], pixels[i + 2]]
}

/// Le sol proche doit être le quad de jeu (sombre, ~0,10 linéaire), pas la
/// dalle béton claire (~0,52) — régression du bug de slices d'instances.
#[test]
fn fp_sol_grille_visible_pas_la_dalle() {
    let gpu = match pollster::block_on(GpuCtx::new(None)) {
        Ok(g) => g,
        Err(e) => {
            println!("SKIP — pas d'adaptateur ({e})");
            return;
        }
    };
    let mut scene = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let (pixels, w, h) = rendu_fp(&gpu, &mut scene, false);

    let floor = probe(&pixels, w, h, 0.5, 0.85);
    println!("sol proche : {floor:?}");
    // Sombre = grille de jeu (≤ ~110) ; clair (≥150) = dalle Blender.
    assert!(floor[0] < 130, "sol trop clair ({floor:?}) : la dalle Blender masque le quad de jeu");

    // Le haut de l'écran montre l'intérieur du hangar (murs/toit ≠ fond).
    let sky = probe(&pixels, w, h, 0.5, 0.12);
    println!("haut : {sky:?}");
    let clear = [14u8, 17, 22];
    assert!(
        (sky[0] as i32 - clear[0] as i32).abs() + (sky[1] as i32 - clear[1] as i32).abs() > 20,
        "haut de l'image = couleur de fond : le shell ne se dessine pas"
    );

    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/screenshots/fp-diagnostic.png");
    save_png(&out, w, h, &pixels).unwrap();
    println!("PNG écrit : {}", out.display());
}

/// Sans machines/colis : même rendu, vérifie que le shell seul suffit à
/// couvrir correctement (le quad de jeu reste visible).
#[test]
fn fp_shell_seul() {
    let gpu = match pollster::block_on(GpuCtx::new(None)) {
        Ok(g) => g,
        Err(e) => {
            println!("SKIP — pas d'adaptateur ({e})");
            return;
        }
    };
    let mut scene = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let (pixels, w, h) = rendu_fp(&gpu, &mut scene, true);
    let floor = probe(&pixels, w, h, 0.5, 0.85);
    println!("sol proche (shell seul) : {floor:?}");
    assert!(floor[0] < 130, "shell seul : sol masqué ({floor:?})");
}
