// A light at a place.
struct Light {
    // Where it is, then how far it reaches.
    position: vec4<f32>,
    // Linear, times its energy, then the cosine of the angle its cone begins to fade at.
    color: vec4<f32>,
    // The way it shines, of length 1, then the cosine of its cone's angle: under -1 for a point
    // light, which has no cone.
    direction: vec4<f32>,
    // The first of the view's shadow maps that are its own, or under 0 for a light without
    // shadows; how many are, one or six; and how large a texel of them is, in world units, a
    // world unit from the light along the way a map looks.
    shadow: vec4<f32>,
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
    // Its fog's color, linear, then 1, or 0 for a view without fog.
    fog_color: vec4<f32>,
    // How far from the camera its fog begins, then 1 over how much farther it's whole.
    fog: vec4<f32>,
    // Where the camera is.
    eye: vec4<f32>,
    // The way the camera looks, of length 1, then 1 for an orthographic camera, or 0.
    eye_forward: vec4<f32>,
    lights: array<Light, 32>,
    // What maps the world onto each shadow map, as a camera at its light would see it.
    shadow_maps: array<mat4x4<f32>, 24>,
}

@group(0) @binding(0)
var<uniform> view: View;
// How far from its light the nearest thing casting a shadow is, in each shadow map.
@group(0) @binding(1)
var shadow_texture: texture_depth_2d_array;
@group(0) @binding(2)
var shadow_sampler: sampler_comparison;

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

// How many of a shadow map's texels a surface is moved off itself by, along its normal and
// toward the light, before it's looked up in the map: enough that a surface the light reaches
// doesn't shade itself for the map's coarseness, however steeply the light meets it.
const SHADOW_OFF_SURFACE: f32 = 1.5;
const SHADOW_TOWARD_LIGHT: f32 = 0.5;

// Whether the light reaches `world`, 1, or something casting a shadow is nearer the light, 0.
// `normal` is the surface's, or nothing for a billboard's, and `toward` the way to the light.
fn lit(index: u32, world: vec3<f32>, normal: vec3<f32>, toward: vec3<f32>) -> f32 {
    let shadow = view.lights[index].shadow;
    if shadow.x < 0.0 {
        return 1.0;
    }
    var map = u32(shadow.x);
    if shadow.y > 1.5 {
        // Six maps, one each way along each axis: the one that looks most nearly this way.
        let away = -toward;
        let size = abs(away);
        if size.x >= size.y && size.x >= size.z {
            map += select(1u, 0u, away.x > 0.0);
        } else if size.y >= size.z {
            map += select(3u, 2u, away.y > 0.0);
        } else {
            map += select(5u, 4u, away.z > 0.0);
        }
    }
    let onto_map = view.shadow_maps[map];
    // How far it is from the light along the way the map looks, which is what a texel's size
    // grows with.
    let texel = (onto_map * vec4<f32>(world, 1.0)).w * shadow.z;
    let moved = world + normal * (texel * SHADOW_OFF_SURFACE)
        + toward * (texel * SHADOW_TOWARD_LIGHT);
    let clip = onto_map * vec4<f32>(moved, 1.0);
    if clip.w <= 0.0 {
        return 1.0;
    }
    let place = clip.xyz / clip.w;
    let uv = place.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    return textureSampleCompareLevel(shadow_texture, shadow_sampler, uv, i32(map), place.z);
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
        var surface = vec3<f32>(0.0);
        if in.world.w < 0.5 {
            // A surface with no normal faces nothing, as the sun finds it.
            level *= max(dot(normal, toward), 0.0) * step(0.5, normal_length);
            surface = normal;
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
        if level > 0.0 {
            level *= lit(i, in.world.xyz, surface, toward);
        }
        light += color_inner.rgb * level;
    }
    var shown = color.rgb * min(light, vec3<f32>(1.0));
    if view.fog_color.a > 0.5 {
        // It fades to the fog's color with how far it is from the camera, in a straight line
        // from where the fog begins to where there's nothing else: from where the camera is,
        // or for an orthographic camera, which is nowhere, by how far ahead of it.
        let away = in.world.xyz - view.eye.xyz;
        let distance = select(
            length(away),
            dot(away, view.eye_forward.xyz),
            view.eye_forward.w > 0.5,
        );
        let amount = clamp((distance - view.fog.x) * view.fog.y, 0.0, 1.0);
        shown = mix(shown, view.fog_color.rgb, amount);
    }
    return vec4<f32>(shown, 1.0);
}
