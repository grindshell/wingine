use std::sync::Arc;

use winit::{dpi::PhysicalSize, event_loop::OwnedDisplayHandle, window::Window};

use crate::Result;

/// The wgpu device plus the surface that presents to the game window.
pub(crate) struct Gpu {
    instance: wgpu::Instance,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    pub async fn new(display: OwnedDisplayHandle, window: Arc<Window>) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(display),
        ));
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await?;
        log::info!("using GPU adapter: {:?}", adapter.get_info());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await?;

        // An adapter chosen with `compatible_surface` always supports at least one format.
        let format = surface.get_capabilities(&adapter).formats[0];
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
        surface.configure(&device, &config);

        Ok(Self {
            instance,
            window,
            surface,
            config,
            device,
            queue,
        })
    }

    /// The format render pipelines targeting the surface must use.
    pub fn view_format(&self) -> wgpu::TextureFormat {
        self.config.format.add_srgb_suffix()
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        // A zero-sized surface is invalid; this happens while the window is minimized.
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }

    /// Acquires the next surface texture, or `None` if this frame should be skipped.
    pub fn acquire_frame(&mut self) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => Some(texture),
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => None,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                drop(texture);
                self.surface.configure(&self.device, &self.config);
                None
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                None
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                unreachable!("no error scope is registered, so validation errors panic")
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                match self.instance.create_surface(self.window.clone()) {
                    Ok(surface) => {
                        self.surface = surface;
                        self.surface.configure(&self.device, &self.config);
                    }
                    Err(err) => log::error!("failed to recreate lost surface: {err}"),
                }
                None
            }
        }
    }

    pub fn present(&self, frame: wgpu::SurfaceTexture) {
        self.window.pre_present_notify();
        self.queue.present(frame);
    }
}
