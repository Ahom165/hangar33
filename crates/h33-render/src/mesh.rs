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
