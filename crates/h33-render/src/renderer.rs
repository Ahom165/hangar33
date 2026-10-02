//! Renderer wgpu : contexte GPU, pipeline instancié unique, rendu sur
//! surface (fenêtre) ou texture offscreen (tests + screenshots).
//!
//! Perf note : TOUT le hangar (tapis, machines, colis, objets) est dessiné
//! en UN SEUL draw call instancié. La limite pratique est le fill-rate /
//! la taille du buffer d'instances (~80 o/instance), pas le CPU.

use glam::Vec3;
use wgpu::util::{DeviceExt, initialize_adapter_from_env_or_default};

use crate::mesh::Mesh;
use crate::types::{CameraUniform, InstanceRaw};

// ======================================================================
//  Contexte GPU
// ======================================================================

pub struct GpuCtx {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl GpuCtx {
    /// Init du contexte. Le backend/adaptateur peut être forcé par env :
    ///   WGPU_BACKEND=vulkan  WGPU_ADAPTER_NAME=lavapipe
    /// (utile pour tester sur lavapipe / software rendering, cf. README).
    pub async fn new(compatible_surface: Option<&wgpu::Surface<'static>>) -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::from_env().unwrap_or(wgpu::Backends::PRIMARY),
            ..Default::default()
        });

        let adapter =
            match initialize_adapter_from_env_or_default(&instance, compatible_surface).await {
                Some(a) => a,
                None => instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::HighPerformance,
                        compatible_surface,
                        force_fallback_adapter: false,
                    })
                    .await
                    .ok_or_else(|| "aucun adaptateur GPU disponible".to_string())?,
            };

        let info = adapter.get_info();
        log_adapter(&info);

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("h33.device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .map_err(|e| format!("request_device: {e}"))?;

        Ok(Self { instance, adapter, device, queue })
    }
}

fn log_adapter(info: &wgpu::AdapterInfo) {
    println!(
        "[h33-render] adapter: {} ({:?}) — backend {:?}",
        info.name, info.device_type, info.backend
    );
}

// ======================================================================
//  Renderer de scène
// ======================================================================

pub struct SceneRenderer {
    pub camera_buf: wgpu::Buffer,
    camera_bg: wgpu::BindGroup,
    scene_pipeline: wgpu::RenderPipeline,
    floor_pipeline: wgpu::RenderPipeline,
    cube: Mesh,
    floor: Mesh,
    instance_buf: wgpu::Buffer,
    instance_capacity: usize,
    color_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
}

const INSTANCE_SIZE: wgpu::BufferAddress = std::mem::size_of::<InstanceRaw>() as wgpu::BufferAddress;

/// Un lot de dessin : un mesh (cube procédural ou GLB Blender) + ses
/// instances. Les GLB ont des couleurs matériaux cuites dans les sommets,
/// l'instance ne porte donc que la matrice + un tint (highlight...).
pub struct MeshBatch<'a> {
    pub mesh: &'a Mesh,
    pub instances: &'a [InstanceRaw],
}

impl SceneRenderer {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let depth_format = wgpu::TextureFormat::Depth32Float;

        // --- Bind group caméra ------------------------------------------
        let camera_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera.uniform"),
            contents: bytemuck::bytes_of(&CameraUniform::from_mat(
                glam::Mat4::IDENTITY,
                Vec3::ONE,
            )),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera.bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera.bg"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera_buf.as_entire_binding() }],
        });

        // --- Pipelines ---------------------------------------------------
        let scene_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/scene.wgsl").into()),
        });
        let floor_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("floor.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/floor.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene.layout"),
            bind_group_layouts: &[&camera_bgl],
            push_constant_ranges: &[],
        });

        let cube = Mesh::cube(device);
        let floor = floor_mesh_holder(device);

        let scene_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &scene_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    crate::mesh::Vertex::LAYOUT,
                    wgpu::VertexBufferLayout {
                        array_stride: INSTANCE_SIZE,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute { offset: 0, shader_location: 3, format: wgpu::VertexFormat::Float32x4 },
                            wgpu::VertexAttribute { offset: 16, shader_location: 4, format: wgpu::VertexFormat::Float32x4 },
                            wgpu::VertexAttribute { offset: 32, shader_location: 5, format: wgpu::VertexFormat::Float32x4 },
                            wgpu::VertexAttribute { offset: 48, shader_location: 6, format: wgpu::VertexFormat::Float32x4 },
                            wgpu::VertexAttribute { offset: 64, shader_location: 7, format: wgpu::VertexFormat::Float32x4 },
                        ],
                    },
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &scene_module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let floor_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("floor.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &floor_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[crate::mesh::Vertex::LAYOUT],
            },
            fragment: Some(wgpu::FragmentState {
                module: &floor_module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // --- Buffer d'instances (croît au besoin) ------------------------
        let initial_capacity = 8192usize;
        let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: INSTANCE_SIZE * initial_capacity as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            camera_buf,
            camera_bg,
            scene_pipeline,
            floor_pipeline,
            cube,
            floor,
            instance_buf,
            instance_capacity: initial_capacity,
            color_format,
            depth_format,
        }
    }

    fn ensure_instance_capacity(&mut self, device: &wgpu::Device, needed: usize) {
        if needed <= self.instance_capacity {
            return;
        }
        let mut new_cap = self.instance_capacity * 2;
        while new_cap < needed {
            new_cap *= 2;
        }
        self.instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: INSTANCE_SIZE * new_cap as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.instance_capacity = new_cap;
    }

    /// Dessine : sol (grille), instances du cube procédural (tapis, items,
    /// marqueurs) puis les lots de meshes statiques (hangar, machines, colis).
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        camera: &CameraUniform,
        instances: &[InstanceRaw],
        batches: &[MeshBatch],
    ) {
        queue.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(camera));
        // ⚠️ UN SEUL upload d'instances, AVANT la passe : `queue.write_buffer`
        // appelé pendant l'enregistrement s'exécuterait AVANT toute la passe
        // et le dernier write écraserait les précédents (bug du shell décalé
        // de +0,12 m). On concatène donc cubes + lots et on découpe en
        // slices par draw.
        let mut all: Vec<InstanceRaw> = Vec::with_capacity(instances.len() + batches.iter().map(|b| b.instances.len()).sum::<usize>());
        all.extend_from_slice(instances);
        let mut ranges: Vec<(u64, u64)> = Vec::with_capacity(batches.len()); // (octet début, octet fin)
        for b in batches {
            let start = (all.len() * INSTANCE_SIZE as usize) as u64;
            all.extend_from_slice(b.instances);
            let end = (all.len() * INSTANCE_SIZE as usize) as u64;
            ranges.push((start, end));
        }
        self.ensure_instance_capacity(device, all.len().max(1));
        if !all.is_empty() {
            queue.write_buffer(&self.instance_buf, 0, bytemuck::cast_slice(&all));
        }

        let clear = wgpu::Color {
            r: 0.055,
            g: 0.065,
            b: 0.085,
            a: 1.0,
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene.pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Scène instanciée (cube procédural) — instances en tête du buffer.
            pass.set_pipeline(&self.scene_pipeline);
            pass.set_bind_group(0, &self.camera_bg, &[]);
            pass.set_vertex_buffer(0, self.cube.vertex_buf.slice(..));
            pass.set_index_buffer(self.cube.index_buf.slice(..), wgpu::IndexFormat::Uint16);
            let count = instances.len() as u32;
            if count > 0 {
                pass.set_vertex_buffer(
                    1,
                    self.instance_buf.slice(0..(instances.len() as u64) * INSTANCE_SIZE),
                );
                pass.draw_indexed(0..self.cube.index_count, 0, 0..count);
            }

            // Lots GLB (hangar, machines, colis) — même pipeline, slice
            // d'instances propre à chaque lot, index U32.
            for (batch, (start, end)) in batches.iter().zip(ranges.iter()) {
                if batch.instances.is_empty() {
                    continue;
                }
                pass.set_vertex_buffer(0, batch.mesh.vertex_buf.slice(..));
                pass.set_vertex_buffer(1, self.instance_buf.slice(start..end));
                pass.set_index_buffer(batch.mesh.index_buf.slice(..), batch.mesh.index_format);
                pass.draw_indexed(0..batch.mesh.index_count, 0, 0..batch.instances.len() as u32);
            }

            // Sol (grille) — dessiné en dernier : quad à y=0,02 au-dessus de
            // la dalle Blender (top y=0), il gagne son depth naturellement,
            // mais pas la peine de lui faire écrire du depth utile avant
            // les gros volumes.
            pass.set_pipeline(&self.floor_pipeline);
            pass.set_bind_group(0, &self.camera_bg, &[]);
            pass.set_vertex_buffer(0, self.floor.vertex_buf.slice(..));
            pass.set_index_buffer(self.floor.index_buf.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..self.floor.index_count, 0, 0..1);
        }
    }

    pub fn color_format(&self) -> wgpu::TextureFormat {
        self.color_format
    }

    pub fn depth_format(&self) -> wgpu::TextureFormat {
        self.depth_format
    }
}

/// Petit holder pour éviter un import croisé (le sol est un mesh normal).
fn floor_mesh_holder(device: &wgpu::Device) -> Mesh {
    crate::mesh::floor_mesh(device, 60.0)
}

// ======================================================================
//  Rendu offscreen + readback (tests, screenshots)
// ======================================================================

/// Crée une depth texture prête à l'emploi.
pub fn depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Rend une scène offscreen et renvoie les pixels RGBA8 (bloquant).
/// Utilisé par les tests (lavapipe) et la capture d'écran du smoke-test.
pub fn render_offscreen_rgba8(
    gpu: &GpuCtx,
    scene: &mut SceneRenderer,
    width: u32,
    height: u32,
    camera: &CameraUniform,
    instances: &[InstanceRaw],
    batches: &[MeshBatch],
) -> Vec<u8> {
    let color_tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen.color"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: scene.color_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let color_view = color_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth_texture(&gpu.device, width, height);

    let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("offscreen.encoder"),
    });
    scene.draw(
        &gpu.device,
        &gpu.queue,
        &mut encoder,
        &color_view,
        &depth_view,
        camera,
        instances,
        batches,
    );

    // Copie vers un buffer aligné sur 256 octets par rangée.
    let bytes_per_row = align256(width * 4);
    let out_size = (bytes_per_row * height) as u64;
    let read_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("offscreen.readback"),
        size: out_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        color_tex.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &read_buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    gpu.queue.submit([encoder.finish()]);

    // Attente bloquante (test/offscreen : pas de rendu simultané).
    let (tx, rx) = std::sync::mpsc::channel();
    read_buf.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = gpu.device.poll(wgpu::Maintain::Wait);
    rx.recv()
        .expect("callback de mapping non reçu")
        .expect("mapping buffer échoué");

    let data = read_buf.slice(..).get_mapped_range().to_vec();
    read_buf.unmap();

    // Désaligne les rangées -> RGBA8 compact.
    let row_bytes = (width * 4) as usize;
    let mut out = Vec::with_capacity(row_bytes * height as usize);
    for y in 0..height as usize {
        let start = y * bytes_per_row as usize;
        out.extend_from_slice(&data[start..start + row_bytes]);
    }
    out
}

fn align256(x: u32) -> u32 {
    (x + 255) & !255
}

/// Écrit un PNG RGBA8.
pub fn save_png(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) -> std::io::Result<()> {
    let file = std::fs::File::create(path)?;
    let mut w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(&mut w, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    writer.finish()?;
    Ok(())
}
