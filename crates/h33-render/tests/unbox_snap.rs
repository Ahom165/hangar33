//! Test GPU offscreen du DÉBALLAGE (v0.3.10) : carton ouvert + objets
//! formés (vase-cylindre, sphère, tore-bague) rendus de près, PNG écrit
//! pour vérification visuelle + assertions de pixels.
//!
//! ```sh
//! source scripts/dev-env.sh
//! cargo test -p h33-render --test unbox_snap -- --nocapture
//! ```

use glam::Vec3;
use h33_render::camera::OrbitCamera;
use h33_render::mesh::Mesh;
use h33_render::renderer::{GpuCtx, MeshBatch, SceneRenderer, render_offscreen_rgba8, save_png};
use h33_render::types::{CameraUniform, InstanceRaw};

#[test]
fn deballage_offscreen_carton_ouvert_et_formes() {
    let gpu = match pollster::block_on(GpuCtx::new(None)) {
        Ok(g) => g,
        Err(e) => {
            println!("SKIP — pas d'adaptateur GPU dispo ({e}).");
            return;
        }
    };
    let mut scene = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);

    let open_crate = Mesh::open_crate(&gpu.device);
    let prism = Mesh::prism(&gpu.device);
    let sphere = Mesh::sphere(&gpu.device);
    let torus = Mesh::torus(&gpu.device);

    // Palette en bois sous le carton.
    let mut cubes = vec![InstanceRaw::flat(
        Vec3::new(0.0, 0.05, 0.0),
        Vec3::new(1.2, 0.1, 1.2),
        [0.45, 0.33, 0.2, 1.0],
    )];
    // Carton ouvert (kraft), posé 6 mm au-dessus de la palette (anti z-fight).
    let mut open_crate_inst = vec![InstanceRaw::new(
        Vec3::new(0.0, 0.106, 0.0),
        0.3,
        Vec3::splat(0.92),
        [0.85, 0.66, 0.44, 1.0],
    )];
    // Vase (cylindre bleu), montre (sphère plate), LA BAGUE (tore doré).
    let mut prism_inst = vec![InstanceRaw::new(
        Vec3::new(-0.2, 0.1 + 0.15, 0.0),
        0.4,
        Vec3::new(0.18, 0.3, 0.18),
        [0.55, 0.78, 0.88, 1.0],
    )];
    let mut sphere_inst = vec![InstanceRaw::new(
        Vec3::new(0.08, 0.1 + 0.035, 0.12),
        0.0,
        Vec3::new(0.15, 0.07, 0.15),
        [0.85, 0.87, 0.9, 1.0],
    )];
    let mut torus_inst = vec![InstanceRaw::new(
        Vec3::new(0.22, 0.106 + 0.13 * 0.45, -0.1),
        0.8,
        Vec3::splat(0.45),
        [1.3, 1.05, 0.2, 1.0],
    )];

    let (w, h) = (640u32, 480u32);
    let cam = OrbitCamera {
        aspect: w as f32 / h as f32,
        ..Default::default()
    };
    let _ = cam;
    // Caméra plongée à 1,2 m du carton (vue joueur debout devant la palette).
    let eye = Vec3::new(1.6, 1.2, 1.6);
    let target = Vec3::new(0.0, 0.35, 0.0);
    let view = glam::Mat4::look_at_rh(eye, target, Vec3::Y);
    let proj = glam::Mat4::perspective_rh(1.05, w as f32 / h as f32, 0.1, 200.0);
    let cam_uniform = CameraUniform::from_mat(proj * view, eye);

    let batches = [
        MeshBatch { mesh: &open_crate, instances: &open_crate_inst },
        MeshBatch { mesh: &prism, instances: &prism_inst },
        MeshBatch { mesh: &sphere, instances: &sphere_inst },
        MeshBatch { mesh: &torus, instances: &torus_inst },
    ];

    let pixels = render_offscreen_rgba8(&gpu, &mut scene, w, h, &cam_uniform, &cubes, &batches);
    assert_eq!(pixels.len(), (w * h * 4) as usize);

    // Assertions de contenu : le kraft du carton et l'or de la bague
    // doivent être présents en quantité non triviale.
    let mut kraft = 0u32;
    let mut gold = 0u32;
    for px in pixels.chunks_exact(4) {
        let (r, g, b) = (px[0] as u32, px[1] as u32, px[2] as u32);
        if r > 90 && g > 60 && b > 30 && r > b + 20 && g > b + 10 {
            kraft += 1;
        }
        // Or saturé (r >> b) — exclut le kraft (r/b ≈ 2,3 mais peu lumineux)
        // et le blanc (r ≈ g ≈ b).
        if r > 170 && g > 120 && b < 130 && r > b + 90 && g > b + 40 {
            gold += 1;
        }
    }
    println!("pixels kraft: {kraft}, pixels or: {gold} / {}", w * h);

    let out = std::path::Path::new("../../docs/screenshots/unbox-snap.png");
    save_png(out, w, h, &pixels).expect("écriture PNG");
    println!("PNG écrit : docs/screenshots/unbox-snap.png");
    assert!(kraft > 500, "le carton ouvert ne se voit pas assez ({kraft} px)");
    assert!(gold > 60, "la bague dorée ne se voit pas ({gold} px)");
}
