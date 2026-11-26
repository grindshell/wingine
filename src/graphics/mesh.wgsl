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
    // shadows; how many are, one or six; how large a texel of them is, in world units, a world
    // unit from the light along the way a map looks; and 1 for a light that keeps to a box, or 0.
    shadow: vec4<f32>,
    // The rows of what takes the world onto its box's cube, from -1 to 1 each way, for a light
    // that keeps to a box.
    bounds: array<vec4<f32>, 3>,
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
    // Its sky's color straight up, linear, then 1, or 0 for a view without a sky.
    sky_zenith: vec4<f32>,
    // Its sky's color level with the camera, linear, then how many times a way's height it is
    // along from there to the zenith or the ground.
    sky_horizon: vec4<f32>,
    // Its sky's color straight down, linear.
    sky_ground: vec4<f32>,
    // How far the way the view's right edge looks is from the way its middle does, along the
    // camera's right, and the same of its top edge along its up: nothing for an orthographic
    // camera.
    sky_right: vec4<f32>,
    sky_up: vec4<f32>,
    // What places the view's clip space on the target's: across and down scaled, then moved.
    sky_crop: vec4<f32>,
    // What maps the world onto the sun's shadow map, which looks along its light without
    // perspective: from -1 to 1 across and up it, and from 0, nearest the sun, to 1.
    sun_map: mat4x4<f32>,
    // How large a texel of that map is, in world units; how much of the sun's light a shadow
    // takes; how far from `sun_center` shadows reach, squared; then 1 for a sun that casts
    // shadows, or 0.
    sun_shadow: vec4<f32>,
    // The point the sun's shadows reach out from.
    sun_center: vec4<f32>,
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
// How far along the sun's light the nearest thing casting a shadow is, in the sun's shadow map.
@group(0) @binding(3)
var sun_texture: texture_depth_2d;

@group(1) @binding(0)
var mesh_texture: texture_2d<f32>;
@group(1) @binding(1)
var mesh_sampler: sampler;
// What the mesh's emission is multiplied by, on the same texture coordinates: white for none.
@group(2) @binding(0)
var emission_texture: texture_2d<f32>;
@group(2) @binding(1)
var emission_sampler: sampler;

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
    // What it gives off whatever lights it, sRGB, from bytes.
    @location(13) emission: vec4<f32>,
    // How much of its own colors it shows whatever lights it, sRGB, from bytes.
    @location(14) glow: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // Linear.
    @location(1) color: vec4<f32>,
    // Linear, both: what it gives off, and how much of its own colors it shows, whatever lights it.
    @location(4) emission: vec3<f32>,
    @location(5) glow: vec3<f32>,
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
    out.emission = srgb_to_linear(instance.emission.rgb);
    out.glow = srgb_to_linear(instance.glow.rgb);
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

// How much of the sun's light reaches `world`: all of it, 1, or with something casting a shadow
// nearer the sun, what a shadow leaves. All of it too past the shadows' reach. `normal` is the
// surface's, or nothing for a billboard's. A texel is one size all over the map, there being no
// perspective in it.
fn sun_lit(world: vec3<f32>, normal: vec3<f32>) -> f32 {
    let shadow = view.sun_shadow;
    let away = world - view.sun_center.xyz;
    if dot(away, away) > shadow.z {
        return 1.0;
    }
    let moved = world + normal * (shadow.x * SHADOW_OFF_SURFACE)
        - view.light_direction.xyz * (shadow.x * SHADOW_TOWARD_LIGHT);
    let place = (view.sun_map * vec4<f32>(moved, 1.0)).xyz;
    let uv = place.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    let reached = textureSampleCompareLevel(sun_texture, shadow_sampler, uv, place.z);
    return 1.0 - shadow.y * (1.0 - reached);
}

@fragment
fn fs_main(in: VertexOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let color = textureSample(mesh_texture, mesh_sampler, in.uv) * in.color;
    // Sampled before anything can make what follows differ from pixel to pixel, as sampling must.
    let emitted = in.emission * textureSample(emission_texture, emission_sampler, in.uv).rgb;
    if color.a < 0.5 {
        discard;
    }
    var shown = shade(in, front_facing, color.rgb, emitted);
    let fog = fog_amount(in.world.xyz);
    if fog > 0.0 {
        shown = mix(shown, fades_into(in.world.xyz), fog);
    }
    return vec4<f32>(shown, 1.0);
}

// What a blended pixel fades to in fog, set as its pipeline is made: 0 what an opaque one does,
// the sky or the fog's color, for one blended normally, which covers what's behind it; 1 black,
// for one that adds or screens, which then adds nothing; 2 white, for one that multiplies, which
// then darkens nothing.
override fades_to: u32 = 0u;

// A blended mesh's pixel, a particle's or a mesh's drawn blended: shaded as an opaque one is,
// whatever its alpha, and given as a premultiplied color, as blending wants.
@fragment
fn fs_blended(in: VertexOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let color = textureSample(mesh_texture, mesh_sampler, in.uv) * in.color;
    let emitted = in.emission * textureSample(emission_texture, emission_sampler, in.uv).rgb;
    let shown = shade(in, front_facing, color.rgb, emitted);
    var toward = fades_into(in.world.xyz);
    if fades_to == 1u {
        toward = vec3<f32>(0.0);
    } else if fades_to == 2u {
        toward = vec3<f32>(1.0);
    }
    let fogged = mix(shown, toward, fog_amount(in.world.xyz));
    return vec4<f32>(fogged * color.a, color.a);
}

// How much of what's at `world` the view's fog hides, from 0 to 1: none nearer than where it
// begins, all at where it's whole, and in a straight line between, from where the camera is, or
// for an orthographic camera, which is nowhere, by how far ahead of it. None without fog.
fn fog_amount(world: vec3<f32>) -> f32 {
    if view.fog_color.a < 0.5 {
        return 0.0;
    }
    let away = world - view.eye.xyz;
    let distance = select(length(away), dot(away, view.eye_forward.xyz), view.eye_forward.w > 0.5);
    return clamp((distance - view.fog.x) * view.fog.y, 0.0, 1.0);
}

// What's at `world` fades into in fog: the sky the way it's seen from the camera, as though
// nothing were there, or for an orthographic camera, the way it looks; or without a sky, the
// fog's color.
fn fades_into(world: vec3<f32>) -> vec3<f32> {
    if view.sky_zenith.a < 0.5 {
        return view.fog_color.rgb;
    }
    let way = select(world - view.eye.xyz, view.eye_forward.xyz, view.eye_forward.w > 0.5);
    return sky_at(way);
}

// The view's sky the way `way` looks, whatever its length: from its horizon's color, level, to
// its zenith's or its ground's, in a straight line in the way's height, as `Sky::color_at` has it.
fn sky_at(way: vec3<f32>) -> vec3<f32> {
    let size = length(way);
    let height = select(0.0, way.y / size, size > 0.0);
    let along = min(abs(height) * view.sky_horizon.w, 1.0);
    let toward = select(view.sky_ground.rgb, view.sky_zenith.rgb, height >= 0.0);
    return mix(view.sky_horizon.rgb, toward, along);
}

struct SkyOut {
    @builtin(position) clip_position: vec4<f32>,
    // The way the camera looks through it, not of length 1.
    @location(0) way: vec3<f32>,
}

// A corner of one triangle covering the view, at the far end of its depth, so only what nothing
// nearer hides shows it, carrying the way the camera looks there. The view's clip space is from -1
// to 1 across and up, which the triangle's corners at -1 and 3 cover.
@vertex
fn vs_sky(@builtin(vertex_index) index: u32) -> SkyOut {
    let corner = vec2<f32>(select(-1.0, 3.0, index == 1u), select(-1.0, 3.0, index == 2u));
    var out: SkyOut;
    out.clip_position = vec4<f32>(corner * view.sky_crop.xy + view.sky_crop.zw, 1.0, 1.0);
    out.way = view.eye_forward.xyz + view.sky_right.xyz * corner.x + view.sky_up.xyz * corner.y;
    return out;
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    return vec4<f32>(sky_at(in.way), 1.0);
}

// What a pixel of `color`, giving off `emitted`, shows before fog: lit by the view's sun,
// ambient light and lights, and glowing.
fn shade(in: VertexOut, front_facing: bool, color: vec3<f32>, emitted: vec3<f32>) -> vec3<f32> {
    // Only double-sided meshes show their backs, which are lit as though they faced the other way.
    var normal = select(-in.normal, in.normal, front_facing);
    var facing = 0.0;
    let normal_length = length(normal);
    if normal_length > 0.0 {
        normal = normal / normal_length;
        facing = max(dot(normal, -view.light_direction.xyz), 0.0);
    }
    if facing > 0.0 && view.sun_shadow.w > 0.5 {
        // A billboard's normals aren't its surface's, so it's looked up where it is, as lights
        // at places find it.
        let surface = select(normal, vec3<f32>(0.0), in.world.w > 0.5);
        facing *= sun_lit(in.world.xyz, surface);
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
        if view.lights[i].shadow.w > 0.5 {
            // A light that keeps to a box lights nothing outside it, faces and all inside.
            let bounds = view.lights[i].bounds;
            let place = vec4<f32>(in.world.xyz, 1.0);
            let along = vec3<f32>(dot(bounds[0], place), dot(bounds[1], place), dot(bounds[2], place));
            // The world position is interpolated, and mapping it onto a box can cancel large
            // translated coordinates. Keep faces inside despite that roundoff, with allowance
            // scaled by the terms of each dot product, as Light::bounds_hold does on the CPU.
            let magnitude = abs(place);
            let terms = vec3<f32>(dot(abs(bounds[0]), magnitude),
                dot(abs(bounds[1]), magnitude), dot(abs(bounds[2]), magnitude));
            let rounding = max(terms, vec3<f32>(1.0)) * 0.0000019073486328125;
            if any(abs(along) > vec3<f32>(1.0) + rounding) {
                continue;
            }
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
    // What it gives off, and how much of its own colors it shows, whatever lights it: added to
    // what light shows of it, unshaded, and as bright as white at most, as light is.
    return min(
        color * min(light, vec3<f32>(1.0)) + emitted + color * in.glow,
        vec3<f32>(1.0),
    );
}
