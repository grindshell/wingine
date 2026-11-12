// Draws a 3D view's meshes into a shadow map: how far the nearest of them is from a light, along
// the way the map looks. It takes the vertices and instances `mesh.wgsl` does.

struct Map {
    // Maps the world onto the shadow map, as a camera at the light would see it.
    light_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> map: Map;

@group(1) @binding(0)
var mesh_texture: texture_2d<f32>;
@group(1) @binding(1)
var mesh_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
}

struct InstanceIn {
    @location(4) model_x: vec3<f32>,
    @location(5) model_y: vec3<f32>,
    @location(6) model_z: vec3<f32>,
    @location(7) model_translation: vec3<f32>,
    @location(11) uv_bounds: vec4<f32>,
    @location(12) tint: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
}

@vertex
fn vs_main(vertex: VertexIn, instance: InstanceIn) -> VertexOut {
    let p = vertex.position;
    let world = instance.model_x * p.x + instance.model_y * p.y + instance.model_z * p.z
        + instance.model_translation;
    var out: VertexOut;
    out.clip_position = map.light_proj * vec4<f32>(world, 1.0);
    out.uv = mix(instance.uv_bounds.xy, instance.uv_bounds.zw, vertex.uv);
    out.alpha = vertex.color.a * instance.tint.a;
    return out;
}

// What the view doesn't draw of a mesh, the pixels of its texture less than half opaque, casts
// no shadow either.
@fragment
fn fs_main(in: VertexOut) {
    if textureSample(mesh_texture, mesh_sampler, in.uv).a * in.alpha < 0.5 {
        discard;
    }
}
