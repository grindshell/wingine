//! Rendering without a window, for screenshot tests and offline rendering.

use std::sync::mpsc;

use crate::{
    Context, Error, Result,
    graphics::{Canvas, gpu::Gpu, renderer::Renderer},
};

/// Format of headless render targets. sRGB, matching what the window surface is viewed as.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Renders frames into memory instead of a window.
///
/// The GPU adapter is chosen as for a window, including wgpu's environment variable overrides
/// (`WGPU_BACKEND`, `WGPU_ADAPTER_NAME`, `WGPU_POWER_PREF`). Software adapters such as WARP and
/// lavapipe work, which lets this run on machines without a GPU.
pub struct Headless {
    ctx: Context,
    canvas: Canvas,
}

impl Headless {
    pub fn new() -> Result<Self> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let gpu = pollster::block_on(Gpu::new(instance, None))?;
        let renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
        Ok(Self {
            ctx: Context::new(gpu, renderer),
            canvas: Canvas::new(),
        })
    }

    /// Engine services, for loading the assets a frame draws.
    pub fn context(&mut self) -> &mut Context {
        &mut self.ctx
    }

    /// Name and backend of the GPU adapter rendering the frames, for diagnostics.
    pub fn adapter_name(&self) -> String {
        let info = self.ctx.gpu.adapter.get_info();
        format!("{} ({:?})", info.name, info.backend)
    }

    /// Renders one `width` by `height` frame drawn by `draw` and returns its pixels.
    /// The canvas is `width` by `height` logical pixels, one per physical pixel.
    pub fn render(
        &mut self,
        width: u32,
        height: u32,
        draw: impl FnOnce(&mut Canvas),
    ) -> Result<Image> {
        self.canvas.begin(width as f32, height as f32);
        draw(&mut self.canvas);

        let Context { gpu, renderer, .. } = &mut self.ctx;
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("headless frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        renderer.render(&gpu.device, &gpu.queue, &self.canvas, &view);
        read_texture(gpu, &texture)
    }
}

/// Copies an RGBA8 texture's pixels back to the CPU.
fn read_texture(gpu: &Gpu, texture: &wgpu::Texture) -> Result<Image> {
    let (width, height) = (texture.width(), texture.height());
    let row_bytes = width * 4;
    // Texture-to-buffer copies need rows padded to a fixed alignment.
    let padded_row_bytes = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);

    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("headless readback"),
        size: u64::from(padded_row_bytes) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("headless readback"),
        });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row_bytes),
                rows_per_image: Some(height),
            },
        },
        texture.size(),
    );
    gpu.queue.submit([encoder.finish()]);

    let (sender, receiver) = mpsc::channel();
    buffer.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|err| Error::ReadFrame(err.into()))?;
    receiver
        .recv()
        .expect("map callback runs during poll")
        .map_err(|err| Error::ReadFrame(err.into()))?;

    let mapped = buffer
        .get_mapped_range(..)
        .map_err(|err| Error::ReadFrame(err.into()))?;
    let pixels = mapped
        .chunks_exact(padded_row_bytes as usize)
        .flat_map(|row| &row[..row_bytes as usize])
        .copied()
        .collect();
    Ok(Image {
        width,
        height,
        pixels,
    })
}

/// A rendered frame: tightly packed 8-bit RGBA pixels in sRGB, row by row from the top-left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Image {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }
}
