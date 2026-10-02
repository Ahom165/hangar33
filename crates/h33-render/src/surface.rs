//! Rendu sur surface fenêtrée (winit) : configuration du swapchain,
//! redimensionnement, depth buffer. La boucle réelle vit dans h33-app.

use std::sync::Arc;

use winit::window::Window;

use crate::renderer::{GpuCtx, SceneRenderer, depth_texture};

pub struct SurfaceRenderer {
    pub gpu: GpuCtx,
    surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub scene: SceneRenderer,
    pub depth: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

fn create_instance() -> wgpu::Instance {
    wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::from_env().unwrap_or(wgpu::Backends::PRIMARY),
        ..Default::default()
    })
}

impl SurfaceRenderer {
    pub async fn new(window: Arc<Window>) -> Result<Self, String> {
        let instance = create_instance();
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("create_surface: {e}"))?;

        // Adaptateur : env d'abord (WGPU_BACKEND / WGPU_ADAPTER_NAME),
        // sinon le premier compatible avec la surface.
        let adapter = match wgpu::util::initialize_adapter_from_env_or_default(&instance, Some(&surface)).await {
            Some(a) => a,
            None => instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                })
                .await
                .ok_or_else(|| "aucun adaptateur GPU disponible".to_string())?,
        };
        let info = adapter.get_info();
        println!(
            "[h33-render] adapter: {} ({:?}) — backend {:?}",
            info.name, info.device_type, info.backend
        );

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

        let caps = surface.get_capabilities(&adapter);
        // Format sRGB si dispo (couleurs cohérentes avec l'offscreen sRGB).
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo, // vsync, pas de tearing
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let scene = SceneRenderer::new(&device, format);
        let depth = depth_texture(&device, width, height);

        Ok(Self {
            gpu: GpuCtx { instance, adapter, device, queue },
            surface,
            config,
            scene,
            depth,
            width,
            height,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if width == self.width && height == self.height {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.width = width;
        self.height = height;
        self.surface.configure(&self.gpu.device, &self.config);
        self.depth = depth_texture(&self.gpu.device, width, height);
    }

    pub fn get_current_texture(&self) -> Result<wgpu::SurfaceTexture, wgpu::SurfaceError> {
        self.surface.get_current_texture()
    }

    /// Reconfigure après une erreur de surface (Lost / Outdated).
    pub fn force_reconfigure(&mut self) {
        self.surface.configure(&self.gpu.device, &self.config);
        self.depth = depth_texture(&self.gpu.device, self.width, self.height);
    }

    /// Rendu offscreen (screenshots, CI) réutilisant le même device.
    pub fn render_offscreen(
        &mut self,
        width: u32,
        height: u32,
        camera: &crate::types::CameraUniform,
        instances: &[crate::types::InstanceRaw],
        batches: &[crate::renderer::MeshBatch],
    ) -> Vec<u8> {
        crate::renderer::render_offscreen_rgba8(&self.gpu, &mut self.scene, width, height, camera, instances, batches)
    }

    pub fn depth_view(&self) -> &wgpu::TextureView {
        &self.depth
    }
}
