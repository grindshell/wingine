use std::sync::Arc;

use winit::{dpi::PhysicalSize, window::Window};

use super::gpu::Gpu;

/// The surface that presents rendered frames to the game window.
pub(crate) struct WindowSurface {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

impl WindowSurface {
    /// Configures `surface`, which must have been created for `window` from `gpu`'s instance.
    pub fn new(gpu: &Gpu, window: Arc<Window>, surface: wgpu::Surface<'static>) -> Self {
        // The adapter was chosen to be compatible with this surface, so it supports some format.
        let format = surface.get_capabilities(&gpu.adapter).formats[0];
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            // Rendering goes through an sRGB view so blending happens in linear space.
            view_formats: vec![format.add_srgb_suffix()],
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::AutoVsync,
        };
        surface.configure(&gpu.device, &config);
        Self {
            window,
            surface,
            config,
        }
    }

    /// The format render pipelines targeting the surface must use.
    pub fn view_format(&self) -> wgpu::TextureFormat {
        self.config.format.add_srgb_suffix()
    }

    pub fn resize(&mut self, gpu: &Gpu, size: PhysicalSize<u32>) {
        // A zero-sized surface is invalid; this happens while the window is minimized.
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&gpu.device, &self.config);
    }

    /// Acquires the next surface texture, or `None` if this frame should be skipped.
    pub fn acquire_frame(&mut self, gpu: &Gpu) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => Some(texture),
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => None,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                drop(texture);
                self.surface.configure(&gpu.device, &self.config);
                None
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&gpu.device, &self.config);
                None
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                unreachable!("no error scope is registered, so validation errors panic")
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                match gpu.instance.create_surface(self.window.clone()) {
                    Ok(surface) => {
                        self.surface = surface;
                        self.surface.configure(&gpu.device, &self.config);
                    }
                    Err(err) => log::error!("failed to recreate lost surface: {err}"),
                }
                None
            }
        }
    }

    pub fn present(&self, gpu: &Gpu, frame: wgpu::SurfaceTexture) {
        self.window.pre_present_notify();
        gpu.queue.present(frame);
    }
}
