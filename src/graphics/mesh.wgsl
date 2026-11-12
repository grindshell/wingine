// A light at a place.
struct Light {
    // Where it is, then how far it reaches.
    position: vec4<f32>,
    // Linear, times its energy, then the cosine of the angle its cone begins to fade at.
    color: vec4<f32>,
    // The way it shines, of length 1, then the cosine of its cone's angle: under -1 for a point
    // light, which has no cone.
    direction: vec4<f32>,
}

struct View {
    view_proj: mat4x4<f32>,
    // The way the sun's light goes, of length 1, or 0 for no sun.
    light_direction: vec4<f32>,
    // Linear.
    light_color: vec4<f32>,
    // Linear.
    ambient: vec4<f32>,
    // How many of `lights` light the view, then nothing.
    light_count: vec4<u32>,
    lights: array<Light, 32>,
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
    // In the world, of length 1 at each corner, or 0 for none, so how long the mesh's normals are,
    // and how the transform stretches them, doesn't sway how they blend between corners. Blending
    // shortens them.
    @location(2) normal: vec3<f32>,
    // Where it is in the world, then 1 for a billboard, which lights at places light whichever
    // way they are from it, or 0 for a mesh, lit by how squarely it faces them.
    @location(3) world: vec4<f32>,
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
    let normal = instance.normal_x * n.x + instance.normal_y * n.y + instance.normal_z * n.z;
    let length_squared = dot(normal, normal);
    out.normal = select(vec3<f32>(0.0), normal * inverseSqrt(length_squared), length_squared > 0.0);
    // Only a billboard's normals are moved by nothing across and up it: they point up, whichever
    // way it faces. A mesh's are moved by its transform, which is never nothing.
    let billboard = all(instance.normal_x == vec3<f32>(0.0))
        && all(instance.normal_y == vec3<f32>(0.0));
    out.world = vec4<f32>(world, select(0.0, 1.0, billboard));
    return out;
}

@fragment
fn fs_main(in: VertexOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let color = textureSample(mesh_texture, mesh_sampler, in.uv) * in.color;
    if color.a < 0.5 {
        discard;
    }
    // Only double-sided meshes show their backs, which are lit as though they faced the other way.
    var normal = select(-in.normal, in.normal, front_facing);
    var facing = 0.0;
    let normal_length = length(normal);
    if normal_length > 0.0 {
        normal = normal / normal_length;
        facing = max(dot(normal, -view.light_direction.xyz), 0.0);
    }
    var light = view.ambient.rgb + view.light_color.rgb * facing;
    for (var i = 0u; i < view.light_count.x; i++) {
        let position = view.lights[i].position;
        let to_light = position.xyz - in.world.xyz;
        let distance = length(to_light);
        // Its light falls in a straight line, to nothing at its range.
        let reach = 1.0 - distance / position.w;
        if reach <= 0.0 {
            continue;
        }
        let toward = to_light / max(distance, 1e-6);
        var level = reach;
        if in.world.w < 0.5 {
            // A surface with no normal faces nothing, as the sun finds it.
            level *= max(dot(normal, toward), 0.0) * step(0.5, normal_length);
        }
        let direction = view.lights[i].direction;
        let color_inner = view.lights[i].color;
        if direction.w >= -1.0 {
            // A spot light: nothing outside its cone, and fading across the cone's edge, if it
            // has one.
            let along = dot(-toward, direction.xyz);
            let span = color_inner.w - direction.w;
            if span > 0.0 {
                level *= clamp((along - direction.w) / span, 0.0, 1.0);
            } else {
                level *= step(direction.w, along);
            }
        }
        light += color_inner.rgb * level;
    }
    return vec4<f32>(color.rgb * min(light, vec3<f32>(1.0)), 1.0);
}
