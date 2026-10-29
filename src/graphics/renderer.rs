use std::mem::size_of;

use bytemuck::{Pod, Zeroable};

use super::canvas::{Canvas, Vertex};

/// Initial GPU buffer room, in quads. Buffers grow as needed.
const INITIAL_QUADS: u64 = 1024;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
}

/// Turns a recorded [`Canvas`] into GPU draw calls.
pub(crate) struct Renderer {
    pipeline: wgpu::RenderPipeline,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shape.wgsl"));

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

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shape"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shape"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
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

        let vertex_buffer = create_buffer(
            device,
            "canvas vertices",
            wgpu::BufferUsages::VERTEX,
            INITIAL_QUADS * 4 * size_of::<Vertex>() as u64,
        );
        let index_buffer = create_buffer(
            device,
            "canvas indices",
            wgpu::BufferUsages::INDEX,
            INITIAL_QUADS * 6 * size_of::<u32>() as u64,
        );

        Self {
            pipeline,
            globals_buffer,
            globals_bind_group,
            vertex_buffer,
            index_buffer,
        }
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
        let indices: &[u8] = bytemuck::cast_slice(&canvas.indices);
        if !indices.is_empty() {
            ensure_capacity(
                device,
                &mut self.vertex_buffer,
                "canvas vertices",
                vertices.len(),
            );
            ensure_capacity(
                device,
                &mut self.index_buffer,
                "canvas indices",
                indices.len(),
            );
            queue.write_buffer(&self.vertex_buffer, 0, vertices);
            queue.write_buffer(&self.index_buffer, 0, indices);
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
            if !indices.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.globals_bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..vertices.len() as u64));
                pass.set_index_buffer(
                    self.index_buffer.slice(..indices.len() as u64),
                    wgpu::IndexFormat::Uint32,
                );
                pass.draw_indexed(0..canvas.indices.len() as u32, 0, 0..1);
            }
        }
        queue.submit([encoder.finish()]);
    }
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
