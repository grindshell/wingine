use std::{
    mem::size_of,
    num::NonZeroU64,
    ops::Range,
    sync::atomic::{AtomicBool, Ordering},
};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::{
    Blend, Color, Light, Mesh, MeshData, Projection, Texture,
    camera3d::{self, Mat4},
    canvas::{Batch, Canvas, Vertex},
    mesh::GpuVertex,
    view3d::{Instance, MeshOrder, Segment3d, lights_of, shadows_of},
};
use crate::math::{Rect, normalize3};

/// Initial GPU buffer room, in quads. Buffers grow as needed.
const INITIAL_QUADS: u32 = 1024;
/// Initial GPU buffer room, in meshes drawn and in 3D views. Buffers grow as needed.
const INITIAL_MESHES: u64 = 64;
const INITIAL_VIEWS: u64 = 4;
/// How many depth textures to keep, one for each size of target 3D views were drawn on lately.
const DEPTH_TEXTURES: usize = 4;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
}

/// What a 3D view's meshes are drawn with, as `mesh.wgsl` reads it.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ViewUniform {
    view_proj: Mat4,
    /// The way the sun's light goes, of length 1, or 0 for no sun.
    light_direction: [f32; 4],
    /// Linear.
    light_color: [f32; 4],
    /// Linear.
    ambient: [f32; 4],
    /// How many of `lights` light it, then nothing.
    light_count: [u32; 4],
    /// Its fog's color, linear, then 1, or 0 for a view without fog.
    fog_color: [f32; 4],
    /// How far from the camera its fog begins, then 1 over how much farther it's whole, then
    /// nothing.
    fog: [f32; 4],
    /// Where the camera is, then nothing.
    eye: [f32; 4],
    /// The way the camera looks, of length 1, then 1 for an orthographic camera, whose fog is by
    /// how far ahead of it things are, or 0.
    eye_forward: [f32; 4],
    lights: [LightUniform; Light::MAX],
    /// What maps the world onto each of its shadow maps.
    shadow_maps: [Mat4; Light::SHADOW_MAPS],
}

/// A light of a 3D view, as `mesh.wgsl` reads it.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LightUniform {
    /// Where it is, then how far it reaches.
    position: [f32; 4],
    /// Linear, times its energy, then the cosine of the angle its cone begins to fade at.
    color: [f32; 4],
    /// The way it shines, of length 1, then the cosine of its cone's angle: under -1 for a
    /// point light.
    direction: [f32; 4],
    /// The first of the view's shadow maps that are its own, or -1 for none, how many are, and
    /// how large their texels are a world unit from the light, then nothing.
    shadow: [f32; 4],
}

impl LightUniform {
    /// `light`, shaded by the view's shadow maps from `first_map` on, if any are its own.
    fn new(light: &Light, first_map: Option<u32>) -> Self {
        let [x, y, z] = light.position;
        let [r, g, b, _] = light.color.to_linear();
        let [outer, inner] = light.cone_cosines();
        let [dx, dy, dz] = normalize3(light.direction).unwrap_or([0.0, 0.0, 1.0]);
        Self {
            position: [x, y, z, light.range],
            color: [r * light.energy, g * light.energy, b * light.energy, inner],
            direction: [dx, dy, dz, outer],
            shadow: match first_map {
                Some(first) => [
                    first as f32,
                    light.shadow_maps() as f32,
                    light.shadow_texel(),
                    0.0,
                ],
                None => [-1.0, 0.0, 0.0, 0.0],
            },
        }
    }
}

/// Whether a view has had more lights than light it at once, which is said once.
static TOO_MANY_LIGHTS: AtomicBool = AtomicBool::new(false);
/// Whether a view's lights have wanted more shadow maps than it has, which is said once.
static TOO_MANY_SHADOWS: AtomicBool = AtomicBool::new(false);

/// Turns a recorded [`Canvas`] into GPU draw calls, and creates the textures it draws with.
pub(crate) struct Renderer {
    /// A pipeline for each [`Blend`], in the order of [`Blend::ALL`].
    pipelines: [wgpu::RenderPipeline; 4],
    /// The format of the targets `pipeline` renders to.
    target_format: wgpu::TextureFormat,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    /// Nearest-neighbor, so pixel art stays crisp when scaled.
    sampler: wgpu::Sampler,
    /// Nearest-neighbor samplers for model files' textures that repeat, by how they wrap across
    /// and down, made as they're first wanted.
    #[cfg(feature = "models")]
    wrapped_samplers: Vec<([wgpu::AddressMode; 2], wgpu::Sampler)>,
    /// 1x1 white texture used for solid fills.
    white: Texture,
    vertex_buffer: wgpu::Buffer,
    /// Indices for the two triangles of each of `index_quads` quads. Every quad's are the same,
    /// so they're written only when there are more quads than ever before.
    index_buffer: wgpu::Buffer,
    index_quads: u32,
    meshes: MeshRenderer,
    /// Where each of the canvas's 3D views is on the target, if anywhere, worked out each frame.
    viewports: Vec<Option<Viewport>>,
}

/// What draws the meshes of 3D views.
struct MeshRenderer {
    /// For meshes showing only the side their triangles face, then for double-sided ones, each
    /// as they are, then mirrored, whose triangles wind the other way.
    pipelines: [[wgpu::RenderPipeline; 2]; 2],
    views_layout: wgpu::BindGroupLayout,
    /// Each view's [`ViewUniform`], `view_stride` bytes apart.
    views_buffer: wgpu::Buffer,
    views_bind_group: wgpu::BindGroup,
    view_stride: u64,
    /// Each mesh drawn's [`Instance`], in the order they draw.
    instance_buffer: wgpu::Buffer,
    /// The most recently used last.
    depth_textures: Vec<DepthTexture>,
    /// The quad billboards are drawn as.
    billboard: Mesh,
    /// Each view's [`ViewUniform`], as they're uploaded.
    views: Vec<u8>,
    /// What works out the order each view's meshes draw in, as they're uploaded.
    order: MeshOrder,
    /// The instances, in the order they draw, as they're uploaded.
    instances: Vec<Instance>,
    /// Each draw call of meshes, the views' in turn.
    runs: Vec<MeshRun>,
    /// Each view's draw calls, in `runs`.
    view_runs: Vec<Range<usize>>,
    /// Which of a view's lights light it, as each view's uniform is made.
    lights: Vec<u32>,
    /// Which of those have shadows, and the first shadow map of each.
    shadowed: Vec<(u32, u32)>,
    /// Draws meshes into a shadow map: every side of them, and no colors.
    shadow_pipeline: wgpu::RenderPipeline,
    shadow_layout: wgpu::BindGroupLayout,
    /// What maps the world onto each shadow map drawn this frame, the views' in turn,
    /// `shadow_stride` bytes apart.
    shadow_buffer: wgpu::Buffer,
    shadow_bind_group: wgpu::BindGroup,
    shadow_stride: u64,
    /// The same, as they're uploaded.
    shadow_views: Vec<u8>,
    /// Each view's shadow maps, among all those drawn this frame.
    view_shadows: Vec<Range<u32>>,
    /// The shadow maps every view draws into in turn, and then reads.
    shadow_maps: ShadowMaps,
    /// Compares a depth with a shadow map's nearest texel.
    shadow_sampler: wgpu::Sampler,
}

/// A view's shadow maps: the layers of one depth texture. Until a light has shadows, it's a
/// single texel, which nothing reads.
struct ShadowMaps {
    /// Every layer, as views read them.
    all: wgpu::TextureView,
    /// Each layer, to draw into.
    layers: Vec<wgpu::TextureView>,
}

impl ShadowMaps {
    /// `layers` maps, each `size` texels across and down.
    fn new(device: &wgpu::Device, size: u32, layers: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow maps"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let all = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let layer = |layer| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            })
        };
        Self {
            all,
            layers: (0..layers).map(layer).collect(),
        }
    }

    /// Whether they're maps to draw into yet.
    fn are_made(&self) -> bool {
        self.layers.len() == Light::SHADOW_MAPS
    }
}

/// Meshes drawn in one draw call, as instances of one mesh.
struct MeshRun {
    /// Where their instances are in the instance buffer.
    instances: Range<u32>,
    /// One of them, in the canvas's `mesh_draws`, whose mesh, texture and way round they share.
    draw: u32,
}

struct DepthTexture {
    size: [u32; 2],
    view: wgpu::TextureView,
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
        // The shader gives premultiplied colors, which every mode blends.
        let pipelines = Blend::ALL.map(|blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                        blend: Some(blend.state()),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        });

        let white = create_texture(device, queue, &texture_layout, &sampler, 1, 1, &[255; 4]);

        let vertex_buffer = create_buffer(
            device,
            "canvas vertices",
            wgpu::BufferUsages::VERTEX,
            u64::from(INITIAL_QUADS) * 4 * size_of::<Vertex>() as u64,
        );

        let meshes = MeshRenderer::new(device, target_format, &texture_layout);
        Self {
            pipelines,
            target_format,
            globals_buffer,
            globals_bind_group,
            texture_layout,
            sampler,
            #[cfg(feature = "models")]
            wrapped_samplers: Vec::new(),
            white,
            vertex_buffer,
            index_buffer: create_index_buffer(device, INITIAL_QUADS),
            index_quads: INITIAL_QUADS,
            meshes,
            viewports: Vec::new(),
        }
    }

    /// The format of the targets this renderer renders to.
    #[cfg_attr(not(feature = "egui"), expect(dead_code))]
    pub fn target_format(&self) -> wgpu::TextureFormat {
        self.target_format
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

    /// Uploads `rgba` as [`create_texture`](Self::create_texture) does, sampled by `address`
    /// across and down where texture coordinates leave it, as model files' textures may be.
    #[cfg(feature = "models")]
    pub fn create_wrapped_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        [width, height]: [u32; 2],
        rgba: &[u8],
        address: [wgpu::AddressMode; 2],
    ) -> Texture {
        let clamped = [wgpu::AddressMode::ClampToEdge; 2];
        let sampler = if address == clamped {
            &self.sampler
        } else {
            let index = match self
                .wrapped_samplers
                .iter()
                .position(|(a, _)| *a == address)
            {
                Some(index) => index,
                None => {
                    let [address_mode_u, address_mode_v] = address;
                    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                        label: Some("nearest, wrapped"),
                        address_mode_u,
                        address_mode_v,
                        mag_filter: wgpu::FilterMode::Nearest,
                        min_filter: wgpu::FilterMode::Nearest,
                        ..Default::default()
                    });
                    self.wrapped_samplers.push((address, sampler));
                    self.wrapped_samplers.len() - 1
                }
            };
            &self.wrapped_samplers[index].1
        };
        create_texture(
            device,
            queue,
            &self.texture_layout,
            sampler,
            width,
            height,
            rgba,
        )
    }

    /// What a vector font makes the pages of its glyphs with, and writes glyphs to them by, as
    /// text first needs them.
    pub fn glyph_pages(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> GlyphPages {
        GlyphPages {
            device: device.clone(),
            queue: queue.clone(),
            layout: self.texture_layout.clone(),
            sampler: self.sampler.clone(),
        }
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
            view_formats: &[self.target_format.remove_srgb_suffix()],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let texture = bind_texture(device, &self.texture_layout, &self.sampler, &texture, &view);
        RenderTarget { view, texture }
    }

    /// Renders `canvas` onto `target`, which is `target_size` pixels: quads in passes of their
    /// own, and between them, each 3D view in a pass of its own with a depth buffer.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        canvas: &Canvas,
        target: &wgpu::TextureView,
        target_size: [u32; 2],
    ) {
        let clear = canvas.clear_color;
        self.render_cleared(device, queue, canvas, target, target_size, clear);
    }

    /// Renders `canvas` as [`render`](Self::render) does, onto a target that's then drawn as
    /// though opaque: its clear color is opaque, so the target's stays so, and drawing it again
    /// shows its colors as a window would, which ignores alpha.
    pub fn render_opaque(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        canvas: &Canvas,
        target: &wgpu::TextureView,
        target_size: [u32; 2],
    ) {
        let clear = Color {
            a: 1.0,
            ..canvas.clear_color
        };
        self.render_cleared(device, queue, canvas, target, target_size, clear);
    }

    fn render_cleared(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        canvas: &Canvas,
        target: &wgpu::TextureView,
        target_size: [u32; 2],
        clear: Color,
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
        let canvas_size = [canvas.width(), canvas.height()];
        self.viewports.clear();
        self.viewports.extend(canvas.segments.iter().map(|segment| {
            (!segment.draws.is_empty())
                .then(|| Viewport::new(segment.view.rect, canvas_size, target_size))
                .flatten()
        }));
        let viewports = &self.viewports;
        let has_3d = viewports.iter().any(Option::is_some);
        if has_3d {
            self.meshes.upload(device, queue, canvas, viewports);
        }
        let depth = has_3d.then(|| self.meshes.depth_view(device, target_size));

        let [r, g, b, a] = clear.to_linear().map(f64::from);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame"),
        });
        let mut load = wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a });
        let mut first_batch = 0;
        for (index, (segment, viewport)) in canvas.segments.iter().zip(viewports).enumerate() {
            let (Some(viewport), Some(depth)) = (viewport, &depth) else {
                continue;
            };
            let before = segment.before_batch as usize;
            self.draw_quads(
                &mut encoder,
                target,
                load,
                &canvas.batches[first_batch..before],
            );
            load = wgpu::LoadOp::Load;
            first_batch = before;
            self.meshes
                .draw_shadows(&mut encoder, index, canvas, &self.white);
            self.meshes.draw(
                &mut encoder,
                target,
                depth,
                index,
                viewport,
                canvas,
                &self.white,
            );
        }
        self.draw_quads(&mut encoder, target, load, &canvas.batches[first_batch..]);
        queue.submit([encoder.finish()]);
    }

    /// Draws `batches` onto `target` in a pass of their own, which begins by `load`ing it.
    fn draw_quads(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        load: wgpu::LoadOp<wgpu::Color>,
        batches: &[Batch],
    ) {
        let mut batches = batches.iter().filter(|batch| !batch.quads.is_empty());
        let first = batches.next();
        if first.is_none() && matches!(load, wgpu::LoadOp::Load) {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("canvas"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let Some(first) = first else {
            return;
        };
        pass.set_bind_group(0, &self.globals_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        let mut blend = None;
        for batch in std::iter::once(first).chain(batches) {
            if blend != Some(batch.blend) {
                pass.set_pipeline(&self.pipelines[batch.blend as usize]);
                blend = Some(batch.blend);
            }
            let texture = batch.texture.as_ref().unwrap_or(&self.white);
            pass.set_bind_group(1, texture.bind_group(), &[]);
            pass.draw_indexed(batch.quads.start * 6..batch.quads.end * 6, 0, 0..1);
        }
    }
}

impl MeshRenderer {
    fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        texture_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("mesh.wgsl"));
        let views_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3d views"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: NonZeroU64::new(size_of::<ViewUniform>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        // A mesh's texture, then the texture its emission is multiplied by.
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[
                Some(&views_layout),
                Some(texture_layout),
                Some(texture_layout),
            ],
            immediate_size: 0,
        });
        let vertex_attributes = wgpu::vertex_attr_array![
            0 => Float32x3,
            1 => Float32x3,
            2 => Float32x2,
            3 => Unorm8x4,
        ];
        let instance_attributes = wgpu::vertex_attr_array![
            4 => Float32x3,
            5 => Float32x3,
            6 => Float32x3,
            7 => Float32x3,
            8 => Float32x3,
            9 => Float32x3,
            10 => Float32x3,
            11 => Float32x4,
            12 => Unorm8x4,
            13 => Unorm8x4,
            14 => Unorm8x4,
        ];
        let buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<GpuVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &vertex_attributes,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<Instance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &instance_attributes,
            }),
        ];
        let pipeline = |front_face, cull_mode| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("mesh"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &buffers,
                },
                primitive: wgpu::PrimitiveState {
                    front_face,
                    cull_mode,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        // Mirrored meshes' triangles face the other way round their corners, which also tells
        // the shader which side of a double-sided mesh shows.
        let pipelines = [Some(wgpu::Face::Back), None].map(|cull_mode| {
            [wgpu::FrontFace::Ccw, wgpu::FrontFace::Cw].map(|front| pipeline(front, cull_mode))
        });

        let shadow_shader = device.create_shader_module(wgpu::include_wgsl!("shadow.wgsl"));
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow map"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(size_of::<Mat4>() as u64),
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow map"),
            bind_group_layouts: &[Some(&shadow_layout), Some(texture_layout)],
            immediate_size: 0,
        });
        // Whichever side of a mesh faces the light shades what's behind it, so nothing is culled.
        // What keeps a surface from shading itself is in `mesh.wgsl`, where the map is read.
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow map"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shadow_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &buffers,
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shadow_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[],
            }),
            multiview_mask: None,
            cache: None,
        });
        let shadow_maps = ShadowMaps::new(device, 1, 1);
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow maps"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment);
        let shadow_stride = (size_of::<Mat4>() as u64).next_multiple_of(alignment);
        let shadow_buffer = create_buffer(
            device,
            "shadow maps",
            wgpu::BufferUsages::UNIFORM,
            shadow_stride * Light::SHADOW_MAPS as u64,
        );
        let shadow_bind_group = shadow_bind_group(device, &shadow_layout, &shadow_buffer);
        let view_stride = (size_of::<ViewUniform>() as u64).next_multiple_of(alignment);
        let views_buffer = create_buffer(
            device,
            "3d views",
            wgpu::BufferUsages::UNIFORM,
            view_stride * INITIAL_VIEWS,
        );
        let views_bind_group = views_bind_group(
            device,
            &views_layout,
            &views_buffer,
            &shadow_maps,
            &shadow_sampler,
        );
        let instance_buffer = create_buffer(
            device,
            "mesh instances",
            wgpu::BufferUsages::VERTEX,
            size_of::<Instance>() as u64 * INITIAL_MESHES,
        );
        Self {
            pipelines,
            views_layout,
            views_buffer,
            views_bind_group,
            view_stride,
            instance_buffer,
            depth_textures: Vec::new(),
            billboard: Mesh::new(device, &MeshData::billboard()).expect("a quad can be drawn"),
            views: Vec::new(),
            order: MeshOrder::default(),
            instances: Vec::new(),
            runs: Vec::new(),
            view_runs: Vec::new(),
            lights: Vec::new(),
            shadowed: Vec::new(),
            shadow_pipeline,
            shadow_layout,
            shadow_buffer,
            shadow_bind_group,
            shadow_stride,
            shadow_views: Vec::new(),
            view_shadows: Vec::new(),
            shadow_maps,
            shadow_sampler,
        }
    }

    /// Writes each view's uniform, and every mesh's instance, for `canvas`'s 3D views placed on
    /// the target at `viewports`.
    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        canvas: &Canvas,
        viewports: &[Option<Viewport>],
    ) {
        let stride = self.view_stride as usize;
        let views = &mut self.views;
        views.clear();
        views.resize(stride * viewports.len(), 0);
        let shadow_stride = self.shadow_stride as usize;
        self.shadow_views.clear();
        self.view_shadows.clear();
        for (index, (segment, viewport)) in canvas.segments.iter().zip(viewports).enumerate() {
            let first_map = (self.shadow_views.len() / shadow_stride) as u32;
            let Some(viewport) = viewport else {
                self.view_shadows.push(first_map..first_map);
                continue;
            };
            let Range { start, end } = segment.lights.clone();
            let lights = &canvas.lights[start as usize..end as usize];
            let (chosen, shadowed) = (&mut self.lights, &mut self.shadowed);
            let (uniform, maps) = view_uniform(segment, viewport, lights, chosen, shadowed);
            let start = index * stride;
            views[start..start + size_of::<ViewUniform>()]
                .copy_from_slice(bytemuck::bytes_of(&uniform));
            for onto_map in &uniform.shadow_maps[..maps as usize] {
                let start = self.shadow_views.len();
                self.shadow_views.resize(start + shadow_stride, 0);
                self.shadow_views[start..start + size_of::<Mat4>()]
                    .copy_from_slice(bytemuck::bytes_of(onto_map));
            }
            self.view_shadows.push(first_map..first_map + maps);
        }
        // The maps are made when a light first has shadows, so a game without any doesn't hold
        // them.
        let mut rebind = false;
        if !self.shadow_views.is_empty() && !self.shadow_maps.are_made() {
            let [size, maps] = [Light::SHADOW_RESOLUTION, Light::SHADOW_MAPS as u32];
            self.shadow_maps = ShadowMaps::new(device, size, maps);
            rebind = true;
        }
        let size = self.views_buffer.size();
        ensure_capacity(device, &mut self.views_buffer, "3d views", self.views.len());
        if rebind || self.views_buffer.size() != size {
            self.views_bind_group = views_bind_group(
                device,
                &self.views_layout,
                &self.views_buffer,
                &self.shadow_maps,
                &self.shadow_sampler,
            );
        }
        queue.write_buffer(&self.views_buffer, 0, &self.views);
        let size = self.shadow_buffer.size();
        let wanted = self.shadow_views.len();
        ensure_capacity(device, &mut self.shadow_buffer, "shadow maps", wanted);
        if self.shadow_buffer.size() != size {
            self.shadow_bind_group =
                shadow_bind_group(device, &self.shadow_layout, &self.shadow_buffer);
        }
        queue.write_buffer(&self.shadow_buffer, 0, &self.shadow_views);

        // Each view's draws of a kind together, so they're one draw call. Until a view's draws
        // aren't in order already, the canvas's instances are uploaded as they are.
        self.instances.clear();
        self.runs.clear();
        self.view_runs.clear();
        let mut in_order = true;
        for segment in &canvas.segments {
            let Range { start, end } = segment.draws.clone();
            self.order
                .arrange(&canvas.mesh_draws[start as usize..end as usize]);
            let first_run = self.runs.len();
            let mut run_start = 0;
            for &run_end in &self.order.ends {
                self.runs.push(MeshRun {
                    instances: start + run_start..start + run_end,
                    draw: start + self.order.draw_at(run_start),
                });
                run_start = run_end;
            }
            self.view_runs.push(first_run..self.runs.len());
            if !self.order.in_order && in_order {
                in_order = false;
                self.instances
                    .extend_from_slice(&canvas.instances[..start as usize]);
            }
            let instances = &canvas.instances[start as usize..end as usize];
            if self.order.in_order && !in_order {
                self.instances.extend_from_slice(instances);
            } else if !self.order.in_order {
                let order = self.order.order().iter();
                self.instances.extend(order.map(|&i| instances[i as usize]));
            }
        }
        let instances = if in_order {
            &canvas.instances
        } else {
            &self.instances
        };
        let instances: &[u8] = bytemuck::cast_slice(instances);
        ensure_capacity(
            device,
            &mut self.instance_buffer,
            "mesh instances",
            instances.len(),
        );
        queue.write_buffer(&self.instance_buffer, 0, instances);
    }

    /// A depth buffer for a target of `size`, made the first time a target of that size draws a
    /// 3D view, and kept while it's among the last few sizes used.
    fn depth_view(&mut self, device: &wgpu::Device, size: [u32; 2]) -> wgpu::TextureView {
        let texture = match self.depth_textures.iter().position(|t| t.size == size) {
            Some(index) => self.depth_textures.remove(index),
            None => {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("depth"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: DEPTH_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                DepthTexture { size, view }
            }
        };
        if self.depth_textures.len() == DEPTH_TEXTURES {
            self.depth_textures.remove(0);
        }
        let view = texture.view.clone();
        self.depth_textures.push(texture);
        view
    }

    /// Draws the shadow maps of the `index`th of `canvas`'s 3D views, as uploaded: for each, in
    /// a pass of its own, the view's meshes that cast shadows, as its light sees them. The maps
    /// are the same ones for every view, so a view's are drawn just before it is.
    fn draw_shadows(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        index: usize,
        canvas: &Canvas,
        white: &Texture,
    ) {
        let maps = self.view_shadows[index].clone();
        let runs = &self.runs[self.view_runs[index].clone()];
        for (layer, map) in maps.enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow map"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_maps.layers[layer],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.shadow_pipeline);
            let offset = (u64::from(map) * self.shadow_stride) as u32;
            pass.set_bind_group(0, &self.shadow_bind_group, &[offset]);
            pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            for run in runs {
                let draw = &canvas.mesh_draws[run.draw as usize];
                let Some(mesh) = draw.mesh.as_ref().filter(|_| draw.casts_shadow) else {
                    continue;
                };
                let texture = draw.texture.as_ref().unwrap_or(white);
                pass.set_bind_group(1, texture.bind_group(), &[]);
                pass.set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
                pass.set_index_buffer(mesh.index_buffer().slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count(), 0, run.instances.clone());
            }
        }
    }

    /// Draws the meshes of the `index`th of `canvas`'s 3D views, as uploaded, onto `target` at
    /// `viewport`, in a pass of its own that keeps what's on the target and clears `depth`.
    #[expect(clippy::too_many_arguments)]
    fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        index: usize,
        viewport: &Viewport,
        canvas: &Canvas,
        white: &Texture,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("3d view"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let [x, y, width, height] = viewport.pixels;
        pass.set_viewport(x as f32, y as f32, width as f32, height as f32, 0.0, 1.0);
        pass.set_scissor_rect(x, y, width, height);
        let offset = (index as u64 * self.view_stride) as u32;
        pass.set_bind_group(0, &self.views_bind_group, &[offset]);
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        let mut pipeline = None;
        for run in &self.runs[self.view_runs[index].clone()] {
            let draw = &canvas.mesh_draws[run.draw as usize];
            let wanted = (draw.double_sided, draw.mirrored);
            if pipeline != Some(wanted) {
                let [sides, winding] = [draw.double_sided, draw.mirrored].map(usize::from);
                pass.set_pipeline(&self.pipelines[sides][winding]);
                pipeline = Some(wanted);
            }
            let texture = draw.texture.as_ref().unwrap_or(white);
            pass.set_bind_group(1, texture.bind_group(), &[]);
            let emission = draw.emission_texture.as_ref().unwrap_or(white);
            pass.set_bind_group(2, emission.bind_group(), &[]);
            let mesh = draw.mesh.as_ref().unwrap_or(&self.billboard);
            pass.set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
            pass.set_index_buffer(mesh.index_buffer().slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count(), 0, run.instances.clone());
        }
    }
}

fn views_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
    shadow_maps: &ShadowMaps,
    shadow_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("3d views"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer,
                    offset: 0,
                    size: NonZeroU64::new(size_of::<ViewUniform>() as u64),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&shadow_maps.all),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(shadow_sampler),
            },
        ],
    })
}

fn shadow_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("shadow maps"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset: 0,
                size: NonZeroU64::new(size_of::<Mat4>() as u64),
            }),
        }],
    })
}

/// What `segment`'s meshes are drawn with, placed at `viewport` and lit by those of `lights`,
/// the view's, that light it, which `chosen` is room to choose, and `shadowed` room to choose
/// those with shadows; and how many shadow maps they take, the first so many of the view's.
fn view_uniform(
    segment: &Segment3d,
    viewport: &Viewport,
    lights: &[Light],
    chosen: &mut Vec<u32>,
    shadowed: &mut Vec<(u32, u32)>,
) -> (ViewUniform, u32) {
    let view = &segment.view;
    let aspect = view.rect.w / view.rect.h;
    let view_proj = camera3d::mul(&viewport.crop, &view.camera.view_projection(aspect));
    let lighting = &view.lighting;
    let [x, y, z] = normalize3(lighting.direction).unwrap_or([0.0; 3]);
    let left_out = lights_of(view, lights, chosen);
    if left_out > 0 && !TOO_MANY_LIGHTS.swap(true, Ordering::Relaxed) {
        log::warn!(
            "a 3D view is lit by {} lights at once, the nearest to its camera: {left_out} more \
             that reach what it sees are left out",
            Light::MAX
        );
    }
    let (fog_color, fog) = match view.fog() {
        Some(fog) => {
            let [r, g, b, _] = fog.color.to_linear();
            let span = 1.0 / (fog.far - fog.near);
            ([r, g, b, 1.0], [fog.near, span, 0.0, 0.0])
        }
        None => ([0.0; 4], [0.0; 4]),
    };
    let [ex, ey, ez] = view.camera.position;
    let [fx, fy, fz] = view.camera.forward();
    let orthographic = matches!(view.camera.projection, Projection::Orthographic { .. });
    let mut uniform = ViewUniform {
        view_proj,
        light_direction: [x, y, z, 0.0],
        light_color: lighting.color.to_linear(),
        ambient: lighting.ambient.to_linear(),
        light_count: [chosen.len() as u32, 0, 0, 0],
        fog_color,
        fog,
        eye: [ex, ey, ez, 0.0],
        eye_forward: [fx, fy, fz, if orthographic { 1.0 } else { 0.0 }],
        lights: Zeroable::zeroed(),
        shadow_maps: Zeroable::zeroed(),
    };
    for (uniform, &light) in uniform.lights.iter_mut().zip(chosen.iter()) {
        *uniform = LightUniform::new(&lights[light as usize], None);
    }
    let [maps, without] = shadows_of(view, lights, chosen, shadowed);
    if without > 0 && !TOO_MANY_SHADOWS.swap(true, Ordering::Relaxed) {
        log::warn!(
            "a 3D view has {} shadow maps, which the lights nearest its camera take first: \
             {without} more lights with shadows light it without them",
            Light::SHADOW_MAPS
        );
    }
    for &(place, first) in shadowed.iter() {
        let light = &lights[chosen[place as usize] as usize];
        uniform.lights[place as usize] = LightUniform::new(light, Some(first));
        for (onto_map, map) in light.shadow_views().zip(first as usize..) {
            uniform.shadow_maps[map] = onto_map;
        }
    }
    (uniform, maps)
}

/// Where on a target a 3D view's rect falls.
#[derive(Debug, PartialEq)]
struct Viewport {
    /// The target's pixels it covers: left, top, width and height.
    pixels: [u32; 4],
    /// Maps the view's clip space, which spans its whole rect, onto `pixels`' clip space, where
    /// the rect is cut to the target.
    crop: Mat4,
}

impl Viewport {
    /// Where `rect`, in canvas pixels, falls on a target `target` pixels in size, showing a
    /// canvas `canvas` in size. `None` if it covers none of the target's pixels.
    fn new(rect: Rect, canvas: [f32; 2], target: [u32; 2]) -> Option<Self> {
        let scale = [0, 1].map(|i| target[i] as f32 / canvas[i]);
        let start = [rect.x * scale[0], rect.y * scale[1]];
        let size = [rect.w * scale[0], rect.h * scale[1]];
        // The pixels whose middles are inside it, as a quad over it would cover: by rasterizers'
        // top-left rule, a middle on its left or top edge is inside, and one on its right or bottom
        // edge isn't, so each edge is at the first pixel whose middle is on it or past it.
        let [left, top, right, bottom] = [
            (start[0], target[0]),
            (start[1], target[1]),
            (start[0] + size[0], target[0]),
            (start[1] + size[1], target[1]),
        ]
        .map(|(edge, limit)| (edge - 0.5).ceil().clamp(0.0, limit as f32));
        let [width, height] = [right - left, bottom - top];
        if !(width > 0.0 && height > 0.0 && size[0] > 0.0 && size[1] > 0.0) {
            return None;
        }
        // Where the clip space's edges, at -1 and 1 across the whole rect, fall across the
        // pixels it covers.
        let scale_x = size[0] / width;
        let offset_x = (size[0] + 2.0 * (start[0] - left)) / width - 1.0;
        let scale_y = size[1] / height;
        let offset_y = 1.0 - (size[1] + 2.0 * (start[1] - top)) / height;
        Some(Self {
            pixels: [left, top, width, height].map(|v| v as u32),
            crop: [
                [scale_x, 0.0, 0.0, 0.0],
                [0.0, scale_y, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [offset_x, offset_y, 0.0, 1.0],
            ],
        })
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
            view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        rgba,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    bind_texture(device, layout, sampler, &texture, &view)
}

/// Makes the pages a vector font's glyphs are drawn from, clear until glyphs are written to them,
/// and writes the glyphs. Writes reach the GPU before the commands of the frame they're made in,
/// so a glyph first laid out as a frame draws shows in it.
#[derive(Clone)]
pub(crate) struct GlyphPages {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl GlyphPages {
    /// A clear page `size` texels across and down, and the texture to write glyphs to.
    pub fn create(&self, size: u32) -> (wgpu::Texture, Texture) {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyphs"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bound = bind_texture(&self.device, &self.layout, &self.sampler, &texture, &view);
        (texture, bound)
    }

    /// Writes `rgba`, tightly packed rows of `[width, height]` texels, to `page` at `[x, y]`.
    pub fn write(
        &self,
        page: &wgpu::Texture,
        [x, y]: [u32; 2],
        [width, height]: [u32; 2],
        rgba: &[u8],
    ) {
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: page,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}

/// Wraps `texture`, through `view`, as a [`Texture`] the quad pipeline can draw with. Its format
/// must be sRGB, and have its format without sRGB among its view formats, which egui shows it
/// through: egui reads the bytes as they're stored.
fn bind_texture(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    texture: &wgpu::Texture,
    view: &wgpu::TextureView,
) -> Texture {
    let gui_view = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(texture.format().remove_srgb_suffix()),
        ..Default::default()
    });
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
    Texture::new(bind_group, gui_view, texture.width(), texture.height())
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

    /// Where a point at `clip` in a view's clip space lands in its viewport's pixels.
    fn to_pixels(viewport: &Viewport, [x, y]: [f32; 2]) -> [f32; 2] {
        let [left, top, width, height] = viewport.pixels.map(|v| v as f32);
        let [cx, cy] = project(viewport.crop, x, y);
        [
            left + (cx + 1.0) / 2.0 * width,
            top + (1.0 - cy) / 2.0 * height,
        ]
    }

    #[test]
    fn viewports_scale_from_the_canvas_to_the_target() {
        // A fixed 320x180 canvas on a target three times its size.
        let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
        let viewport = Viewport::new(rect, [320.0, 180.0], [960, 540]).unwrap();
        assert_eq!(viewport.pixels, [30, 60, 300, 150]);
        assert_eq!(to_pixels(&viewport, [-1.0, 1.0]), [30.0, 60.0]);
        assert_eq!(to_pixels(&viewport, [1.0, -1.0]), [330.0, 210.0]);
    }

    #[test]
    fn viewports_are_cut_to_the_target_and_keep_their_projection() {
        // Hanging off the top-left, and on fractions of pixels.
        let rect = Rect::new(-20.5, -10.25, 60.0, 30.0);
        let viewport = Viewport::new(rect, [100.0, 50.0], [100, 50]).unwrap();
        // Its right edge is on the middle of the 40th pixel, which it doesn't cover.
        assert_eq!(viewport.pixels, [0, 0, 39, 20]);
        let near = |[x, y]: [f32; 2], [ex, ey]: [f32; 2]| {
            assert!((x - ex).abs() < 1e-4 && (y - ey).abs() < 1e-4, "{x}, {y}");
        };
        // The rect's corners, still where they were, off the target.
        near(to_pixels(&viewport, [-1.0, 1.0]), [-20.5, -10.25]);
        near(to_pixels(&viewport, [1.0, -1.0]), [39.5, 19.75]);
        near(to_pixels(&viewport, [0.0, 0.0]), [9.5, 4.75]);
    }

    #[test]
    fn viewports_off_the_target_cover_nothing() {
        let size = [100.0, 50.0];
        assert_eq!(
            Viewport::new(Rect::new(100.0, 0.0, 10.0, 10.0), size, [100, 50]),
            None
        );
        assert_eq!(
            Viewport::new(Rect::new(0.0, 0.0, 0.0, 10.0), size, [100, 50]),
            None
        );
        assert_eq!(
            Viewport::new(Rect::new(0.0, 0.0, 10.0, 10.0), [0.0, 0.0], [100, 50]),
            None
        );
    }

    /// The first pixel, and the one past the last, that a quad from `start` to `end` covers of a
    /// row `limit` pixels long: those whose middles are inside it, counting a middle on its left
    /// or top edge but not one on its right or bottom edge, as rasterizers do.
    fn covered(start: f32, end: f32, limit: u32) -> Option<[u32; 2]> {
        let inside = |i: &u32| (start..end).contains(&(*i as f32 + 0.5));
        let first = (0..limit).find(inside)?;
        let last = (0..limit).rev().find(inside)?;
        Some([first, last + 1])
    }

    #[test]
    fn viewports_cover_the_pixels_a_quad_over_their_rect_would() {
        let canvas = [64.0, 48.0];
        // Eighths of a pixel, some of which put edges on pixels' middles at each scale, and either
        // side of a middle.
        let eighths = (-48..=528).map(|i| i as f32 / 8.0);
        let positions: Vec<f32> = eighths.chain([0.49, 0.51, 10.49, 10.51]).collect();
        for scale in [1.0, 1.25, 1.5, 2.0, 2.5] {
            let target = canvas.map(|v| (v * scale) as u32);
            let s = [0, 1].map(|i| target[i] as f32 / canvas[i]);
            for &position in &positions {
                let rect = Rect::new(position, position - 4.0, 10.0, 7.5);
                let viewport = Viewport::new(rect, canvas, target).map(|v| v.pixels);
                // From the rect's edges on the target, worked out as the viewport does.
                let [x, y] = [rect.x * s[0], rect.y * s[1]];
                let across = covered(x, x + rect.w * s[0], target[0]);
                let down = covered(y, y + rect.h * s[1], target[1]);
                let expected = across
                    .zip(down)
                    .map(|([left, right], [top, bottom])| [left, top, right - left, bottom - top]);
                assert_eq!(viewport, expected, "{rect:?} at {scale}x");
            }
        }
    }
}
