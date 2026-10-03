//! Mesh procédural : cube unitaire centré, ombres de faces pré-cuites dans
//! la couleur du sommet (pas de texture, pas d'asset externe en v0.1 —
//! le pipeline Blender MCP arrivera en v0.3, voir ROADMAP).

use wgpu::util::DeviceExt;

/// Sommet : position + normale + teinte de face (faux AO).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
}

impl Vertex {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32x3 },
        ],
    };
}

/// Buffers mesh prêts à dessiner.
pub struct Mesh {
    pub vertex_buf: wgpu::Buffer,
    pub index_buf: wgpu::Buffer,
    pub index_count: u32,
    pub index_format: wgpu::IndexFormat,
}

/// Cube unitaire centré sur l'origine (côté 1), faces teintées :
/// dessus clair, côtés moyens, dessous sombre -> lisibilité sans lumière complexe.
pub fn cube_vertices() -> (Vec<Vertex>, Vec<u16>) {
    let mut v = Vec::with_capacity(24);
    let mut idx = Vec::with_capacity(36);

    // (normale, 4 coins, teinte)
    let faces: [([f32; 3], [[f32; 3]; 4], [f32; 3]); 6] = [
        ([0.0, 1.0, 0.0], [[-0.5, 0.5, -0.5], [-0.5, 0.5, 0.5], [0.5, 0.5, 0.5], [0.5, 0.5, -0.5]], [1.0, 1.0, 1.0]),   // top
        ([0.0, -1.0, 0.0], [[-0.5, -0.5, 0.5], [-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, -0.5, 0.5]], [0.45, 0.45, 0.5]), // bottom
        ([0.0, 0.0, 1.0], [[-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5]], [0.92, 0.92, 0.95]), // +Z (sud, face caméra par défaut)
        ([0.0, 0.0, -1.0], [[0.5, -0.5, -0.5], [-0.5, -0.5, -0.5], [-0.5, 0.5, -0.5], [0.5, 0.5, -0.5]], [0.80, 0.80, 0.85]), // -Z
        ([1.0, 0.0, 0.0], [[0.5, -0.5, 0.5], [0.5, -0.5, -0.5], [0.5, 0.5, -0.5], [0.5, 0.5, 0.5]], [0.86, 0.86, 0.9]),  // +X (est)
        ([-1.0, 0.0, 0.0], [[-0.5, -0.5, -0.5], [-0.5, -0.5, 0.5], [-0.5, 0.5, 0.5], [-0.5, 0.5, -0.5]], [0.74, 0.74, 0.8]), // -X (ouest)
    ];

    for (normal, corners, tint) in faces {
        let base = v.len() as u16;
        for c in corners {
            v.push(Vertex { pos: c, normal, color: tint });
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (v, idx)
}

impl Mesh {
    pub fn cube(device: &wgpu::Device) -> Self {
        let (verts, idx) = cube_vertices();
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh.cube.vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh.cube.indices"),
            contents: bytemuck::cast_slice(&idx),
            usage: wgpu::BufferUsages::INDEX,
        });
        Self {
            vertex_buf,
            index_buf,
            index_count: idx.len() as u32,
            index_format: wgpu::IndexFormat::Uint16,
        }
    }

    /// Mesh GPU depuis des sommets/indices quelconques (assets GLB).
    /// Indices u32 : les hangars dépassent largement les 65 535 sommets.
    pub fn from_raw(
        device: &wgpu::Device,
        label: &str,
        vertices: &[[f32; 3]],
        normals: &[[f32; 3]],
        colors: &[[f32; 3]],
        indices: &[u32],
    ) -> Self {
        assert_eq!(vertices.len(), normals.len(), "{label}: normales != sommets");
        assert_eq!(vertices.len(), colors.len(), "{label}: couleurs != sommets");
        let mut verts: Vec<Vertex> = Vec::with_capacity(vertices.len());
        for i in 0..vertices.len() {
            verts.push(Vertex { pos: vertices[i], normal: normals[i], color: colors[i] });
        }
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("mesh.{label}.vertices")),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("mesh.{label}.indices")),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        Self {
            vertex_buf,
            index_buf,
            index_count: indices.len() as u32,
            index_format: wgpu::IndexFormat::Uint32,
        }
    }
}

/// Quad au sol 2 triangles (grand plan XZ), pour le shader de grille.
/// Légèrement au-dessus de la dalle Blender (top à y=0) : la grille se lit
/// comme une peinture au sol, sans z-fighting.
pub fn floor_mesh(device: &wgpu::Device, half_size: f32) -> Mesh {
    let y = 0.02;
    let verts = [
        Vertex { pos: [-half_size, y, -half_size], normal: [0.0, 1.0, 0.0], color: [1.0; 3] },
        Vertex { pos: [-half_size, y, half_size], normal: [0.0, 1.0, 0.0], color: [1.0; 3] },
        Vertex { pos: [half_size, y, half_size], normal: [0.0, 1.0, 0.0], color: [1.0; 3] },
        Vertex { pos: [half_size, y, -half_size], normal: [0.0, 1.0, 0.0], color: [1.0; 3] },
    ];
    let idx: [u16; 6] = [0, 1, 2, 0, 2, 3];
    let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mesh.floor.vertices"),
        contents: bytemuck::cast_slice(&verts),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mesh.floor.indices"),
        contents: bytemuck::cast_slice(&idx),
        usage: wgpu::BufferUsages::INDEX,
    });
    Mesh {
        vertex_buf,
        index_buf,
        index_count: 6,
        index_format: wgpu::IndexFormat::Uint16,
    }
}

// ======================================================================
//  Meshes procéduraux « objets » (v0.3.10) — sommets BLANCS : la couleur
//  vient du tint de l'instance, donc un seul mesh sert à tous les objets.
// ======================================================================

/// Constructeur générique (indices u16 — tous ces meshes sont petits).
fn mesh_from_u16(device: &wgpu::Device, label: &str, verts: &[Vertex], idx: &[u16]) -> Mesh {
    let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("mesh.{label}.vertices")),
        contents: bytemuck::cast_slice(verts),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("mesh.{label}.indices")),
        contents: bytemuck::cast_slice(idx),
        usage: wgpu::BufferUsages::INDEX,
    });
    Mesh {
        vertex_buf,
        index_buf,
        index_count: idx.len() as u32,
        index_format: wgpu::IndexFormat::Uint16,
    }
}

/// Pousse un quad (2 tris). Le pipeline n'a pas de culling : un seul quad
/// est visible des deux côtés. ⚠️ Ne JAMAIS pousser deux quads coplanaires
/// (même avec normales différentes) : z-fighting = moiré.
fn push_quad(
    v: &mut Vec<Vertex>,
    idx: &mut Vec<u16>,
    corners: [[f32; 3]; 4],
    normal: [f32; 3],
    tint: [f32; 3],
) {
    let base = v.len() as u16;
    for c in corners {
        v.push(Vertex { pos: c, normal, color: tint });
    }
    idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// Sommets du carton : parois avec VRAIE épaisseur (2 cm) — face externe,
/// face interne décalée et bandeau supérieur. Sans épaisseur, les deux
/// faces coplanaires se battent en z-fight.
fn v_add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] { [a[0] + b[0], a[1] + b[1], a[2] + b[2]] }
fn v_mul(a: [f32; 3], s: f32) -> [f32; 3] { [a[0] * s, a[1] * s, a[2] * s] }

/// Carton OUVERT : fond + 4 parois épaisses + 4 rabats rabattus vers
/// l'extérieur (tilt ~50°). Base 1×1 m, parois 0,18 m, rabats 0,34 m —
/// l'instance le scale. Le tint donne la couleur kraft.
pub fn open_crate_vertices() -> (Vec<Vertex>, Vec<u16>) {
    let mut v = Vec::new();
    let mut idx = Vec::new();
    const TOP: f32 = 0.18;
    const FLAP: f32 = 0.34;
    const TH: f32 = 0.02; // épaisseur des parois

    // Fond (vu de dessus ; la face de dessous n'existe pas : posé au sol).
    push_quad(
        &mut v, &mut idx,
        [[-0.5, 0.0, -0.5], [-0.5, 0.0, 0.5], [0.5, 0.0, 0.5], [0.5, 0.0, -0.5]],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 1.0],
    );

    // Parois : (direction externe, tangente, teinte). Face externe à 0,5,
    // face interne à 0,5 - TH, bandeau horizontal en haut.
    let up = [0.0_f32, 1.0, 0.0];
    let sides: [([f32; 3], [f32; 3], f32); 4] = [
        ([0.0, 0.0, -1.0], [1.0, 0.0, 0.0], 0.84), // nord
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], 0.92),  // sud
        ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.88),  // est
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.78), // ouest
    ];
    for (outward, along, tint) in sides {
        // Coins de la face externe (bas) et interne (bas).
        let a_out = v_add(v_mul(outward, 0.5), v_mul(along, -0.5));
        let b_out = v_add(v_mul(outward, 0.5), v_mul(along, 0.5));
        let a_in = v_add(v_mul(outward, 0.5 - TH), v_mul(along, -(0.5 - TH)));
        let b_in = v_add(v_mul(outward, 0.5 - TH), v_mul(along, 0.5 - TH));
        let a_out_t = v_add(a_out, v_mul(up, TOP));
        let b_out_t = v_add(b_out, v_mul(up, TOP));
        let a_in_t = v_add(a_in, v_mul(up, TOP));
        let b_in_t = v_add(b_in, v_mul(up, TOP));

        // Face externe.
        push_quad(&mut v, &mut idx, [b_out, a_out, a_out_t, b_out_t], outward, [tint; 3]);
        // Face interne (décalée de TH : PAS coplanaire).
        let inward = v_mul(outward, -1.0);
        push_quad(&mut v, &mut idx, [a_in, b_in, b_in_t, a_in_t], inward, [tint * 0.85; 3]);
        // Bandeau supérieur (couronne, normale vers le haut).
        push_quad(&mut v, &mut idx, [a_in_t, b_in_t, b_out_t, a_out_t], [0.0, 1.0, 0.0], [tint * 0.95; 3]);

        // Rabat : part du bord supérieur EXTERNE, bascule vers l'extérieur (50°).
        let cos = 0.64_f32; // cos(50°)
        let sin = 0.77_f32;
        let fa = v_add(a_out_t, v_mul(outward, FLAP * cos));
        let fb = v_add(b_out_t, v_mul(outward, FLAP * cos));
        let fa = [fa[0], fa[1] + FLAP * sin, fa[2]];
        let fb = [fb[0], fb[1] + FLAP * sin, fb[2]];
        let fnorm = [outward[0] * sin, cos, outward[2] * sin];
        push_quad(&mut v, &mut idx, [b_out_t, a_out_t, fa, fb], fnorm, [tint * 0.92; 3]);
    }

    (v, idx)
}

/// Prisme octogonal (cylindre low-poly) : rayon 0,5, hauteur 1, centré.
/// Sert aux vases, bougies, tournevis — la scale par instance fait le reste.
pub fn prism_vertices() -> (Vec<Vertex>, Vec<u16>) {
    const SIDES: usize = 8;
    let mut v = Vec::new();
    let mut idx = Vec::new();

    // Contour : 8 sommets hauts + 8 bas.
    let mut ring = [[0.0f32; 3]; SIDES];
    for i in 0..SIDES {
        let a = (i as f32) / SIDES as f32 * std::f32::consts::TAU;
        ring[i] = [0.5 * a.cos(), 0.0, 0.5 * a.sin()];
    }
    let shade = |i: usize| {
        // Faux éclairage de face, comme le cube (sel en fonction de l'angle).
        let a = (i as f32) / SIDES as f32 * std::f32::consts::TAU;
        0.72 + 0.22 * (0.5 + 0.5 * a.cos())
    };
    for i in 0..SIDES {
        let j = (i + 1) % SIDES;
        let p0 = ring[i];
        let p1 = ring[j];
        let n = [(p0[0] + p1[0]) * 0.5, 0.0, (p0[2] + p1[2]) * 0.5];
        let nl = (n[0] * n[0] + n[2] * n[2]).sqrt();
        let n = [n[0] / nl, 0.0, n[2] / nl];
        let t = shade(i);
        let base = v.len() as u16;
        // Winding CCW vue de l'extérieur : bas A, haut A, haut B, bas B.
        for (p, y) in [(p0, -0.5), (p0, 0.5), (p1, 0.5), (p1, -0.5)] {
            v.push(Vertex { pos: [p[0], y, p[2]], normal: n, color: [t, t, t] });
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    // Caps (fans).
    let top_c = v.len() as u16;
    v.push(Vertex { pos: [0.0, 0.5, 0.0], normal: [0.0, 1.0, 0.0], color: [1.0, 1.0, 1.0] });
    let bot_c = v.len() as u16;
    v.push(Vertex { pos: [0.0, -0.5, 0.0], normal: [0.0, -1.0, 0.0], color: [0.45, 0.45, 0.5] });
    for i in 0..SIDES {
        let j = (i + 1) % SIDES;
        let a = ring[i];
        let b = ring[j];
        let base_top = v.len() as u16;
        v.push(Vertex { pos: [a[0], 0.5, a[2]], normal: [0.0, 1.0, 0.0], color: [1.0, 1.0, 1.0] });
        v.push(Vertex { pos: [b[0], 0.5, b[2]], normal: [0.0, 1.0, 0.0], color: [1.0, 1.0, 1.0] });
        idx.extend_from_slice(&[top_c, base_top + 1, base_top]);
        let base_bot = v.len() as u16;
        v.push(Vertex { pos: [a[0], -0.5, a[2]], normal: [0.0, -1.0, 0.0], color: [0.45, 0.45, 0.5] });
        v.push(Vertex { pos: [b[0], -0.5, b[2]], normal: [0.0, -1.0, 0.0], color: [0.45, 0.45, 0.5] });
        idx.extend_from_slice(&[bot_c, base_bot, base_bot + 1]);
    }

    (v, idx)
}

/// Sphère UV low-poly (10 segments × 6 anneaux), rayon 0,5, centrée,
/// normales lisses. Chaussettes, montres, bijoux, figurines.
pub fn sphere_vertices() -> (Vec<Vertex>, Vec<u16>) {
    const SEG: usize = 10; // longitude
    const RING: usize = 6; // latitude
    let mut v = Vec::new();
    let mut idx = Vec::new();

    for r in 0..=RING {
        let phi = std::f32::consts::FRAC_PI_2 * (-1.0 + 2.0 * r as f32 / RING as f32);
        let (sp, cp) = phi.sin_cos();
        for s in 0..=SEG {
            let th = s as f32 / SEG as f32 * std::f32::consts::TAU;
            let (st, ct) = th.sin_cos();
            let n = [cp * ct, sp, cp * st];
            v.push(Vertex { pos: [0.5 * n[0], 0.5 * n[1], 0.5 * n[2]], normal: n, color: [1.0; 3] });
        }
    }
    for r in 0..RING {
        for s in 0..SEG {
            let a = (r * (SEG + 1) + s) as u16;
            let b = (r * (SEG + 1) + s + 1) as u16;
            let c = ((r + 1) * (SEG + 1) + s) as u16;
            let d = ((r + 1) * (SEG + 1) + s + 1) as u16;
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    (v, idx)
}

/// Tore (anneau) dans le plan XZ : R = 0,35, tube = 0,13, 16×8 segments.
/// LE mesh de LA BAGUE.
pub fn torus_vertices() -> (Vec<Vertex>, Vec<u16>) {
    const SEG: usize = 16; // autour de l'anneau
    const TUBE: usize = 8; // autour du tube
    let (big_r, small_r) = (0.35_f32, 0.13_f32);
    let mut v = Vec::new();
    let mut idx = Vec::new();

    for i in 0..=SEG {
        let th = i as f32 / SEG as f32 * std::f32::consts::TAU;
        let (ct, st) = th.sin_cos();
        for j in 0..=TUBE {
            let ph = j as f32 / TUBE as f32 * std::f32::consts::TAU;
            let (cp, sp) = ph.sin_cos();
            let n = [cp * ct, sp, cp * st];
            v.push(Vertex {
                pos: [big_r * ct + small_r * n[0], small_r * n[1], big_r * st + small_r * n[2]],
                normal: n,
                color: [1.0; 3],
            });
        }
    }
    for i in 0..SEG {
        for j in 0..TUBE {
            let a = (i * (TUBE + 1) + j) as u16;
            let b = (i * (TUBE + 1) + j + 1) as u16;
            let c = ((i + 1) * (TUBE + 1) + j) as u16;
            let d = ((i + 1) * (TUBE + 1) + j + 1) as u16;
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    (v, idx)
}

impl Mesh {
    /// Carton ouvert (déballage manuel).
    pub fn open_crate(device: &wgpu::Device) -> Self {
        let (v, i) = open_crate_vertices();
        mesh_from_u16(device, "open_crate", &v, &i)
    }

    /// Cylindre 8 pans (vases, bougies, tournevis).
    pub fn prism(device: &wgpu::Device) -> Self {
        let (v, i) = prism_vertices();
        mesh_from_u16(device, "prism", &v, &i)
    }

    /// Sphère low-poly (objets mous / ronds).
    pub fn sphere(device: &wgpu::Device) -> Self {
        let (v, i) = sphere_vertices();
        mesh_from_u16(device, "sphere", &v, &i)
    }

    /// Tore (LA BAGUE).
    pub fn torus(device: &wgpu::Device) -> Self {
        let (v, i) = torus_vertices();
        mesh_from_u16(device, "torus", &v, &i)
    }
}
