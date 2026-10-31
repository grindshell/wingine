use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::{
    Texture,
    canvas::{Canvas, Vertex},
};

/// Initial GPU buffer room, in quads. Buffers grow as needed.
const INITIAL_QUADS: u32 = 1024;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
}

/// Turns a recorded [`Canvas`] into GPU draw calls, and creates the textures it draws with.
pub(crate) struct Renderer {
    pipeline: wgpu::RenderPipeline,
    /// The format of the targets `pipeline` renders to.
    target_format: wgpu::TextureFormat,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    /// Nearest-neighbor, so pixel art stays crisp when scaled.
    sampler: wgpu::Sampler,
    /// 1x1 white texture used for solid fills.
    white: Texture,
    vertex_buffer: wgpu::Buffer,
    /// Indices for the two triangles of each of `index_quads` quads. Every quad's are the same,
    /// so they're written only when there are more quads than ever before.
    index_buffer: wgpu::Buffer,
    index_quads: u32,
}

impl Renderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("quad.wgsl"));

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals_buffer = create_buffer(
            device,
            "globals",
            wgpu::BufferUsages::UNIFORM,
            size_of::<Globals>() as u64,
        );
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quad"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quad"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2,
                        1 => Float32x2,
                        2 => Unorm8x4,
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let white = create_texture(device, queue, &texture_layout, &sampler, 1, 1, &[255; 4]);

        let vertex_buffer = create_buffer(
            device,
            "canvas vertices",
            wgpu::BufferUsages::VERTEX,
            u64::from(INITIAL_QUADS) * 4 * size_of::<Vertex>() as u64,
        );

        Self {
            pipeline,
            target_format,
            globals_buffer,
            globals_bind_group,
            texture_layout,
            sampler,
            white,
            vertex_buffer,
            index_buffer: create_index_buffer(device, INITIAL_QUADS),
            index_quads: INITIAL_QUADS,
        }
    }

    /// Uploads tightly packed RGBA8 pixels (sRGB) as a texture.
    pub fn create_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Texture {
        create_texture(
            device,
            queue,
            &self.texture_layout,
            &self.sampler,
            width,
            height,
            rgba,
        )
    }

    /// Creates a texture this renderer can both render into and draw with.
    pub fn create_render_target(
        &self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> RenderTarget {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // The pipeline only renders to this format.
            format: self.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let texture = bind_texture(
            device,
            &self.texture_layout,
            &self.sampler,
            &view,
            width,
            height,
        );
        RenderTarget { view, texture }
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        canvas: &Canvas,
        target: &wgpu::TextureView,
    ) {
        let globals = Globals {
            view_proj: ortho(canvas.width(), canvas.height()),
        };
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));

        let vertices: &[u8] = bytemuck::cast_slice(&canvas.vertices);
        let quads = canvas.quad_count();
        if quads > 0 {
            ensure_capacity(
                device,
                &mut self.vertex_buffer,
                "canvas vertices",
                vertices.len(),
            );
            queue.write_buffer(&self.vertex_buffer, 0, vertices);
            if quads > self.index_quads {
                self.index_quads = quads.next_power_of_two();
                self.index_buffer = create_index_buffer(device, self.index_quads);
            }
        }

        let [r, g, b, a] = canvas.clear_color.to_linear().map(f64::from);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("canvas"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if quads > 0 {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.globals_bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..vertices.len() as u64));
                pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                for batch in &canvas.batches {
                    let texture = batch.texture.as_ref().unwrap_or(&self.white);
                    pass.set_bind_group(1, texture.bind_group(), &[]);
                    pass.draw_indexed(batch.quads.start * 6..batch.quads.end * 6, 0, 0..1);
                }
            }
        }
        queue.submit([encoder.finish()]);
    }
}

fn create_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Texture {
    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        rgba,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    bind_texture(device, layout, sampler, &view, width, height)
}

/// Wraps a texture view as a [`Texture`] the quad pipeline can draw with.
fn bind_texture(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    view: &wgpu::TextureView,
    width: u32,
    height: u32,
) -> Texture {
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    Texture::new(bind_group, width, height)
}

/// An offscreen texture to render a canvas into, then draw like any other texture.
pub(crate) struct RenderTarget {
    pub view: wgpu::TextureView,
    pub texture: Texture,
}

fn create_buffer(
    device: &wgpu::Device,
    label: &str,
    usage: wgpu::BufferUsages,
    size: u64,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// An index buffer drawing `quads` quads, each four vertices in a row, as two triangles.
fn create_index_buffer(device: &wgpu::Device, quads: u32) -> wgpu::Buffer {
    let indices: Vec<u32> = (0..quads)
        .flat_map(|quad| [0, 1, 2, 0, 2, 3].map(|i| quad * 4 + i))
        .collect();
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("quad indices"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    })
}

/// Replaces `buffer` with a larger one if it can't hold `size` bytes. Contents are not kept.
fn ensure_capacity(device: &wgpu::Device, buffer: &mut wgpu::Buffer, label: &str, size: usize) {
    let size = size as u64;
    if buffer.size() < size {
        *buffer = create_buffer(device, label, buffer.usage(), size.next_power_of_two());
    }
}

/// Column-major orthographic projection mapping `(0, 0)..(width, height)`, `y` down, to clip space.
fn ortho(width: f32, height: f32) -> [[f32; 4]; 4] {
    [
        [2.0 / width, 0.0, 0.0, 0.0],
        [0.0, -2.0 / height, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0, 1.0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(m: [[f32; 4]; 4], x: f32, y: f32) -> [f32; 2] {
        [m[0][0] * x + m[3][0], m[1][1] * y + m[3][1]]
    }

    #[test]
    fn ortho_maps_corners_to_clip_space() {
        let m = ortho(800.0, 600.0);
        assert_eq!(project(m, 0.0, 0.0), [-1.0, 1.0]);
        assert_eq!(project(m, 800.0, 600.0), [1.0, -1.0]);
        assert_eq!(project(m, 400.0, 300.0), [0.0, 0.0]);
    }
}
