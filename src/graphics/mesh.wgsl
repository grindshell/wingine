struct View {
    view_proj: mat4x4<f32>,
    // The way the sun's light goes, of length 1, or 0 for no sun.
    light_direction: vec4<f32>,
    // Linear.
    light_color: vec4<f32>,
    // Linear.
    ambient: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> view: View;

@group(1) @binding(0)
var mesh_texture: texture_2d<f32>;
@group(1) @binding(1)
var mesh_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    // sRGB, from bytes.
    @location(3) color: vec4<f32>,
}

// How the mesh is placed and colored.
struct InstanceIn {
    @location(4) model_x: vec3<f32>,
    @location(5) model_y: vec3<f32>,
    @location(6) model_z: vec3<f32>,
    @location(7) model_translation: vec3<f32>,
    @location(8) normal_x: vec3<f32>,
    @location(9) normal_y: vec3<f32>,
    @location(10) normal_z: vec3<f32>,
    // The sprite's texture coordinates: its top-left's, then its bottom-right's.
    @location(11) uv_bounds: vec4<f32>,
    // sRGB, from bytes.
    @location(12) tint: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // Linear.
    @location(1) color: vec4<f32>,
    // In the world, of any length.
    @location(2) normal: vec3<f32>,
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

@vertex
fn vs_main(vertex: VertexIn, instance: InstanceIn) -> VertexOut {
    let p = vertex.position;
    let world = instance.model_x * p.x + instance.model_y * p.y + instance.model_z * p.z
        + instance.model_translation;
    let n = vertex.normal;
    var out: VertexOut;
    out.clip_position = view.view_proj * vec4<f32>(world, 1.0);
    out.uv = mix(instance.uv_bounds.xy, instance.uv_bounds.zw, vertex.uv);
    out.color = vec4<f32>(
        srgb_to_linear(vertex.color.rgb) * srgb_to_linear(instance.tint.rgb),
        vertex.color.a * instance.tint.a,
    );
    out.normal = instance.normal_x * n.x + instance.normal_y * n.y + instance.normal_z * n.z;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let color = textureSample(mesh_texture, mesh_sampler, in.uv) * in.color;
    if color.a < 0.5 {
        discard;
    }
    var facing = 0.0;
    let normal_length = length(in.normal);
    if normal_length > 0.0 {
        facing = max(dot(in.normal / normal_length, -view.light_direction.xyz), 0.0);
    }
    let light = min(view.ambient.rgb + view.light_color.rgb * facing, vec3<f32>(1.0));
    return vec4<f32>(color.rgb * light, 1.0);
}
