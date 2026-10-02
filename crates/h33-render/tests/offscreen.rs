//! Test GPU offscreen : init wgpu (backend forçable via env), rendu d'une
//! scène mini, readback RGBA8, assertions de contenu et écriture d'un PNG.
//!
//! Exécution avec lavapipe (software Vulkan, aucun GPU requis) :
//! ```sh
//! VK_ICD_FILENAMES=/chemin/lvp_icd.json WGPU_BACKEND=vulkan \
//!   cargo test -p h33-render --test offscreen -- --nocapture
//! ```

use h33_render::camera::OrbitCamera;
use h33_render::renderer::{GpuCtx, SceneRenderer, render_offscreen_rgba8, save_png};
use h33_render::types::{CameraUniform, InstanceRaw};

#[test]
fn rendu_offscreen_produit_des_pixels_non_vides() {
    // Nécessite un driver (lavapipe OK, voir scripts/dev-env.sh).
    // Sans driver : SKIP volontaire (pas un échec CI).
    let gpu = match pollster::block_on(GpuCtx::new(None)) {
        Ok(g) => g,
        Err(e) => {
            println!("SKIP — pas d'adaptateur GPU dispo ({e}). Lance scripts/test-gpu.sh avec lavapipe.");
            return;
        }
    };

    let mut scene = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);

    // Scène : un anneau de cubes colorés + quelques gros blocs centraux.
    let mut instances = Vec::new();
    let palette: [[f32; 4]; 5] = [
        [0.9, 0.4, 0.2, 1.0],
        [0.3, 0.8, 0.4, 1.0],
        [0.3, 0.5, 0.95, 1.0],
        [0.8, 0.8, 0.2, 1.0],
        [0.7, 0.3, 0.9, 1.0],
    ];
    for i in 0..120 {
        let angle = (i as f32) / 120.0 * std::f32::consts::TAU;
        let radius = 6.0 + (i % 7) as f32 * 2.0;
        let color = palette[i % palette.len()];
        instances.push(InstanceRaw::new(
            glam::Vec3::new(angle.cos() * radius, 0.4 + (i % 3) as f32 * 0.4, angle.sin() * radius),
            angle,
            glam::Vec3::splat(0.6),
            color,
        ));
    }
    // Bloc "machine" central pour vérifier les gros volumes.
    instances.push(InstanceRaw::flat(
        glam::Vec3::new(0.0, 0.5, 0.0),
        glam::Vec3::new(1.0, 1.0, 1.0),
        [0.85, 0.75, 0.35, 1.0],
    ));

    let (w, h) = (640u32, 480u32);
    let cam = OrbitCamera { aspect: w as f32 / h as f32, ..Default::default() };
    let cam_uniform = CameraUniform::from_mat(cam.view_proj_mat(), cam.eye());

    let pixels = render_offscreen_rgba8(&gpu, &mut scene, w, h, &cam_uniform, &instances, &[]);
    assert_eq!(pixels.len(), (w * h * 4) as usize);

    // --- Assertions de contenu ----------------------------------------
    // 1) Pas une image unie.
    let first = &pixels[0..4];
    let uniform = pixels.chunks_exact(4).all(|px| px == first);
    assert!(!uniform, "image 100 % uniforme : rien n'a été dessiné ?");

    // 2) Beaucoup de pixels diffèrent clairement de la couleur de fond.
    let clear = [0.055, 0.065, 0.085];
    let clear_bgr = [
        (clear[0] * 255.0f32).round() as u8,
        (clear[1] * 255.0f32).round() as u8,
        (clear[2] * 255.0f32).round() as u8,
    ];
    let mut differ = 0u64;
    for px in pixels.chunks_exact(4) {
        let dr = (px[0] as i32 - clear_bgr[0] as i32).abs();
        let dg = (px[1] as i32 - clear_bgr[1] as i32).abs();
        let db = (px[2] as i32 - clear_bgr[2] as i32).abs();
        if dr + dg + db > 24 {
            differ += 1;
        }
    }
    let ratio = differ as f64 / ((w * h) as f64);
    println!("pixels hors fond : {differ} ({:.1} %)", ratio * 100.0);
    assert!(ratio > 0.10, "seulement {:.1} % de pixels hors fond — scène trop vide", ratio * 100.0);

    // 3) On sauve le rendu pour vérification humaine.
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/screenshots/offscreen-test.png");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    save_png(&out, w, h, &pixels).unwrap();
    println!("PNG écrit : {}", out.display());
}
