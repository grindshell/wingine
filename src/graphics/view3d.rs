use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use bytemuck::{Pod, Zeroable};

use super::{
    Camera3d, Canvas, Color, Mesh, Model, Projection, Ray, Sprite, Texture, camera3d::Mat4,
};
use crate::math::{Affine3, Rect, add3, dot3, normalize3, scale3, sub3};

/// What a 3D view shows, where on the canvas, and how it's lit: see [`Canvas::begin_3d`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View3d {
    pub camera: Camera3d,
    /// Where on the canvas the view shows, in the coordinates drawing uses when it begins: the
    /// canvas's camera moves and zooms it, as it does everything else.
    pub rect: Rect,
    pub lighting: Lighting,
    /// What it shows fades to with distance, if anything. Fog that can't be, such as one whose
    /// `far` isn't past its `near`, is none.
    pub fog: Option<Fog>,
}

impl View3d {
    /// A view through `camera` filling `rect`, lit by [`Lighting::default`], without fog.
    pub fn new(camera: Camera3d, rect: Rect) -> Self {
        Self {
            camera,
            rect,
            lighting: Lighting::default(),
            fog: None,
        }
    }

    /// Its fog, if it has one that can be.
    pub(crate) fn fog(&self) -> Option<Fog> {
        self.fog.filter(|fog| fog.problem().is_none())
    }

    /// How far `point` is from the camera, as fog measures it: in a straight line from where
    /// the camera is, or for an orthographic camera, which has no place to be far from, how far
    /// ahead of it.
    pub fn distance(&self, point: [f32; 3]) -> f32 {
        let away = sub3(point, self.camera.position);
        match self.camera.projection {
            Projection::Perspective { .. } => dot3(away, away).sqrt(),
            Projection::Orthographic { .. } => dot3(away, self.camera.forward()),
        }
    }

    /// Where `point` in the view's world shows, in the coordinates its `rect` is in, as
    /// [`Camera3d::project`] places it on the rect. `None` if it's nearer than the near plane or
    /// farther than the far one.
    pub fn project(&self, point: [f32; 3]) -> Option<[f32; 2]> {
        let rect = self.rect;
        let [x, y] = self.camera.project(point, [rect.w, rect.h])?;
        Some([rect.x + x, rect.y + y])
    }

    /// The ray through `point`, in the coordinates its `rect` is in, into the view's world, as
    /// [`Camera3d::ray`] gives it. Points outside the rect have rays too, as though it went on.
    pub fn ray(&self, [x, y]: [f32; 2]) -> Ray {
        let rect = self.rect;
        self.camera.ray([x - rect.x, y - rect.y], [rect.w, rect.h])
    }
}

/// How a 3D view is lit: by a sun, whose light comes from one direction, and by ambient light from
/// all around. Colors multiply what surfaces show, so white lights them as they are, and
/// together they light them no brighter than that.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lighting {
    /// The way the sun's light goes: `[0.0, -1.0, 0.0]` straight down. Its length doesn't matter.
    pub direction: [f32; 3],
    /// The sun's light on surfaces facing it, less on those turned partly away.
    pub color: Color,
    /// Light on every surface, even those facing away from the sun.
    pub ambient: Color,
}

impl Lighting {
    /// No shading: every surface shows as it is.
    pub const UNLIT: Self = Self {
        direction: [0.0, -1.0, 0.0],
        color: Color::BLACK,
        ambient: Color::WHITE,
    };
}

impl Default for Lighting {
    /// A sun high above and a little in front, to the right, so the tops of things are brightest,
    /// then their fronts, then their right-hand sides.
    fn default() -> Self {
        Self {
            direction: [-0.3, -1.0, -0.5],
            color: Color::rgb(0.8, 0.8, 0.8),
            ambient: Color::rgb(0.5, 0.5, 0.5),
        }
    }
}

/// Fog in a 3D view: a color that what's drawn in it fades to with its distance from the camera
/// ([`View3d::distance`]), in a straight line from none at `near` to nothing but fog at `far`
/// and beyond. It's over the view's meshes, models and billboards alike, lit or not.
///
/// A view draws no background, so what's behind the fog is what the canvas was cleared to, or
/// had drawn on it: clear it to the fog's color for the far side of a world to fade into it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fog {
    /// What things fade to.
    pub color: Color,
    /// How far from the camera fog begins, in world units: 0 or more.
    pub near: f32,
    /// How far from the camera there's nothing but fog: more than `near`.
    pub far: f32,
}

impl Fog {
    /// Fog of `color` from `near` to `far` world units from the camera.
    pub const fn new(color: Color, near: f32, far: f32) -> Self {
        Self { color, near, far }
    }

    /// What's wrong with it, if it can't be.
    pub(crate) fn problem(&self) -> Option<String> {
        if !(self.near >= 0.0 && self.near.is_finite()) {
            return Some(format!(
                "a fog's near can't be less than 0, not {}",
                self.near
            ));
        }
        if !(self.far > self.near && self.far.is_finite()) {
            return Some(format!(
                "a fog's far must be more than its near, {}, not {}",
                self.near, self.far
            ));
        }
        None
    }

    /// How much of what's `distance` from the camera is fog, from 0 to 1, as the view draws it.
    pub fn amount_at(&self, distance: f32) -> f32 {
        ((distance - self.near) / (self.far - self.near)).clamp(0.0, 1.0)
    }
}

/// A light at a place in a 3D view, lighting what's within its range: see
/// [`Canvas3d::add_light`]. Its light falls off in a straight line, to nothing at its range. With
/// a `cone` it's a spot light, lighting only what's within that angle of the way it shines.
///
/// A view's lights add to its sun and ambient light, and together they light a surface no
/// brighter than white.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub position: [f32; 3],
    /// Its light on a surface facing it squarely, at its place.
    pub color: Color,
    /// How far it reaches, in world units.
    pub range: f32,
    /// How many times its color it gives: over 1 keeps it bright farther out.
    pub energy: f32,
    /// The way a spot light shines. Its length doesn't matter.
    pub direction: [f32; 3],
    /// Half the angle of a spot light's cone, in radians, up to pi. `None` is a point light,
    /// shining every way.
    pub cone: Option<f32>,
    /// The part of the cone, from its rim inward, across which a spot light fades, by the cosine
    /// of the angle: 0 is a hard rim, and 1 fades from the middle.
    pub edge: f32,
    /// Whether what's in its way shades what's behind: the view's meshes and models, but for
    /// those drawn after [`Canvas3d::set_casts_shadow`] says not, and never its billboards.
    /// Shadows have hard edges. A view has [`Light::SHADOW_MAPS`] shadow maps: a light takes
    /// [`Light::shadow_maps`] of them, and one there are none left for lights without shadows.
    pub shadows: bool,
}

impl Light {
    /// How many lights light a view at once: of those that reach what its camera sees, the
    /// nearest to the camera.
    pub const MAX: usize = 32;
    /// How many shadow maps a view's lights share, the nearest to its camera first.
    pub const SHADOW_MAPS: usize = 24;
    /// How many texels across and down a shadow map is.
    pub const SHADOW_RESOLUTION: u32 = 512;
    /// The widest cone, as half its angle in radians, that one shadow map takes in. A spot light
    /// with a wider one takes six, as a point light does.
    pub const SHADOW_CONE: f32 = std::f32::consts::FRAC_PI_3;

    /// A light shining every way from `position`, reaching `range` world units.
    pub const fn point(position: [f32; 3], color: Color, range: f32) -> Self {
        Self {
            position,
            color,
            range,
            energy: 1.0,
            direction: [0.0, 0.0, 1.0],
            cone: None,
            edge: 0.0,
            shadows: false,
        }
    }

    /// How many of a view's shadow maps it takes, if it casts shadows: one for a spot light
    /// whose cone is no wider than [`Light::SHADOW_CONE`], and six for any other, one for each
    /// way along each axis.
    pub fn shadow_maps(&self) -> usize {
        match self.cone {
            Some(cone) if cone <= Self::SHADOW_CONE => 1,
            _ => 6,
        }
    }

    /// Half the angle each of its shadow maps takes in, across and down: a little more than a
    /// narrow spot light's cone, or of six maps, a quarter turn's half.
    fn shadow_map_angle(&self) -> f32 {
        match self.cone {
            Some(cone) if cone <= Self::SHADOW_CONE => cone + 0.02,
            _ => std::f32::consts::FRAC_PI_4,
        }
    }

    /// How large a texel of its shadow maps is, in world units, for each world unit from the
    /// light along the way a map looks.
    pub(crate) fn shadow_texel(&self) -> f32 {
        2.0 * self.shadow_map_angle().tan() / Self::SHADOW_RESOLUTION as f32
    }

    /// What maps the world onto each of its shadow maps, as a camera at the light would see it:
    /// along the way a narrow spot light shines, or along `+x`, `-x`, `+y`, `-y`, `+z` and `-z`,
    /// which is the order `mesh.wgsl` picks one of six in.
    pub(crate) fn shadow_views(&self) -> impl Iterator<Item = Mat4> + '_ {
        const AXES: [[f32; 3]; 6] = [
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ];
        let count = self.shadow_maps();
        let fov_y = 2.0 * self.shadow_map_angle();
        (0..count).map(move |map| {
            let direction = if count == 1 {
                normalize3(self.direction).unwrap_or(AXES[4])
            } else {
                AXES[map]
            };
            let camera = Camera3d {
                // Nothing nearer the light than this shades anything.
                near: self.range * SHADOW_NEAR,
                far: self.range,
                ..Camera3d::perspective(self.position, add3(self.position, direction), fov_y)
            };
            camera.view_projection(1.0)
        })
    }

    /// A light at `position` shining along `direction`, on what's within `cone` radians of it,
    /// with a hard rim.
    pub const fn spot(
        position: [f32; 3],
        direction: [f32; 3],
        cone: f32,
        color: Color,
        range: f32,
    ) -> Self {
        Self {
            direction,
            cone: Some(cone),
            ..Self::point(position, color, range)
        }
    }

    /// What's wrong with it, if it can't light anything as it is.
    pub(crate) fn problem(&self) -> Option<String> {
        let finite = |v: [f32; 3]| v.iter().all(|v| v.is_finite());
        if !finite(self.position) {
            return Some("a light's position must be finite".into());
        }
        if !(self.range > 0.0 && self.range.is_finite()) {
            return Some(format!(
                "a light's range must be more than 0, not {}",
                self.range
            ));
        }
        if !(self.energy >= 0.0 && self.energy.is_finite()) {
            return Some(format!(
                "a light's energy can't be less than 0, not {}",
                self.energy
            ));
        }
        if !(0.0..=1.0).contains(&self.edge) {
            return Some(format!("a light's edge is from 0 to 1, not {}", self.edge));
        }
        if let Some(cone) = self.cone {
            if !(cone > 0.0 && cone <= std::f32::consts::PI) {
                return Some(format!(
                    "a light's cone is an angle over 0, up to pi, not {cone}"
                ));
            }
            if !finite(self.direction) || normalize3(self.direction).is_none() {
                return Some("a spot light's direction must be finite, and not zero".into());
            }
        }
        None
    }

    /// The cosines of the angles its cone ends at and begins to fade at, or for a point light,
    /// ones every direction is inside.
    pub(crate) fn cone_cosines(&self) -> [f32; 2] {
        match self.cone {
            Some(cone) => [cone.cos(), (cone * (1.0 - self.edge)).cos()],
            None => [-2.0, -2.0],
        }
    }

    /// How much of its color reaches `point`, with nothing in the way: its energy at its place,
    /// falling in a straight line to 0 at its range, and for a spot light, 0 outside its cone.
    /// It's what a surface there facing the light squarely is lit by, as the view draws it.
    pub fn level_at(&self, point: [f32; 3]) -> f32 {
        let to_point = sub3(point, self.position);
        let distance = dot3(to_point, to_point).sqrt();
        let reach = (1.0 - distance / self.range).max(0.0);
        let [outer, inner] = self.cone_cosines();
        let cone = match normalize3(self.direction) {
            Some(direction) if self.cone.is_some() && distance > 0.0 => {
                let along = dot3(to_point, direction) / distance;
                if inner > outer {
                    ((along - outer) / (inner - outer)).clamp(0.0, 1.0)
                } else if along >= outer {
                    1.0
                } else {
                    0.0
                }
            }
            _ => 1.0,
        };
        self.energy * reach * cone
    }
}

/// Which of `lights` light `view`, into `chosen` as indices into them: those whose reach is in
/// what its camera sees, short of where its fog hides everything, and when there are more than
/// [`Light::MAX`], the nearest to the camera, in the order they were added. Returns how many it
/// left out for being too many.
pub(crate) fn lights_of(view: &View3d, lights: &[Light], chosen: &mut Vec<u32>) -> u32 {
    chosen.clear();
    if lights.is_empty() {
        return 0;
    }
    let aspect = view.rect.w / view.rect.h;
    let matrix = view.camera.view_projection(aspect);
    let row = |i: usize| [matrix[0][i], matrix[1][i], matrix[2][i], matrix[3][i]];
    let [x, y, z, w] = [row(0), row(1), row(2), row(3)];
    let add = |a: [f32; 4], b: [f32; 4]| [0, 1, 2, 3].map(|i| a[i] + b[i]);
    let sub = |a: [f32; 4], b: [f32; 4]| [0, 1, 2, 3].map(|i| a[i] - b[i]);
    // The sides of what the camera sees, each facing inward: left and right, bottom and top,
    // near and far.
    let planes = [add(w, x), sub(w, x), add(w, y), sub(w, y), z, sub(w, z)];
    // What a light must reach nearer than to light anything that shows through the fog.
    let hidden = view.fog().map(|fog| fog.far);
    let reaches = |light: &Light| {
        if hidden.is_some_and(|far| view.distance(light.position) - light.range >= far) {
            return false;
        }
        planes.iter().all(|&[a, b, c, d]| {
            let length = (a * a + b * b + c * c).sqrt();
            let [px, py, pz] = light.position;
            // A plane the matrix doesn't give, as of a view with no size, cuts nothing off.
            if length > 0.0 {
                (a * px + b * py + c * pz + d) / length >= -light.range
            } else {
                true
            }
        })
    };
    chosen.extend((0..lights.len() as u32).filter(|&i| reaches(&lights[i as usize])));
    let extra = chosen.len().saturating_sub(Light::MAX);
    if extra > 0 {
        let distance = |&i: &u32| {
            let away = sub3(lights[i as usize].position, view.camera.position);
            dot3(away, away)
        };
        chosen.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
        chosen.truncate(Light::MAX);
        chosen.sort_unstable();
    }
    extra as u32
}

/// How near a light, as a part of its range, something must be to cast no shadow.
const SHADOW_NEAR: f32 = 0.002;

/// Which of `chosen`, the lights of `lights` that light `view`, have shadows, into `shadowed`
/// as their place in `chosen` and the first of the view's shadow maps each takes. The nearest to
/// the camera take theirs first, and one there aren't enough left for goes without. Returns how
/// many maps they take, and how many lights went without.
pub(crate) fn shadows_of(
    view: &View3d,
    lights: &[Light],
    chosen: &[u32],
    shadowed: &mut Vec<(u32, u32)>,
) -> [u32; 2] {
    shadowed.clear();
    let casting = |&(_, light): &(usize, &u32)| lights[*light as usize].shadows;
    let places = chosen.iter().enumerate().filter(casting);
    shadowed.extend(places.map(|(place, _)| (place as u32, 0)));
    let distance = |&(place, _): &(u32, u32)| {
        let light = &lights[chosen[place as usize] as usize];
        let away = sub3(light.position, view.camera.position);
        dot3(away, away)
    };
    shadowed.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
    let mut maps = 0;
    let mut without = 0;
    shadowed.retain_mut(|(place, first)| {
        let takes = lights[chosen[*place as usize] as usize].shadow_maps() as u32;
        if maps + takes > Light::SHADOW_MAPS as u32 {
            without += 1;
            return false;
        }
        *first = maps;
        maps += takes;
        true
    });
    [maps, without]
}

/// Draws meshes into a 3D view on a canvas, from [`Canvas::begin_3d`]. The view is finished when
/// this is dropped, and what's drawn on the canvas after goes over it.
///
/// Meshes hide what's behind them, whatever order they're drawn in.
pub struct Canvas3d<'a> {
    pub(super) canvas: &'a mut Canvas,
}

impl Canvas3d<'_> {
    /// Draws `mesh` placed by `transform`, showing `sprite`, or white without one, multiplied by
    /// the mesh's colors and by `tint`. Pixels of the sprite less than half opaque aren't drawn,
    /// so what's behind shows through them; the rest are drawn opaque.
    pub fn draw_mesh(
        &mut self,
        mesh: &Mesh,
        transform: Affine3,
        sprite: Option<&Sprite>,
        tint: Color,
    ) {
        let colors = Colors {
            tint,
            emission: Color::BLACK,
            emission_sprite: None,
        };
        self.draw_sides(mesh, transform, sprite, colors, false);
    }

    /// Draws each part of `model` as [`draw_mesh`](Self::draw_mesh) would, placed in the model,
    /// and the model placed by `transform`, all multiplied by `tint`: a draw call for each part.
    /// Double-sided parts show from behind too, and each part gives off its
    /// [`emission`](ModelPart::emission).
    pub fn draw_model(&mut self, model: &Model, transform: Affine3, tint: Color) {
        for part in model.parts() {
            let placed = transform * part.transform;
            let sprite = part.sprite.as_ref();
            let colors = Colors {
                tint,
                emission: part.emission,
                emission_sprite: part.emission_sprite.as_ref(),
            };
            self.draw_sides(&part.mesh, placed, sprite, colors, part.double_sided);
        }
    }

    /// Draws `mesh` as [`draw_mesh`](Self::draw_mesh) does, from behind too if `double_sided`.
    fn draw_sides(
        &mut self,
        mesh: &Mesh,
        transform: Affine3,
        sprite: Option<&Sprite>,
        colors: Colors,
        double_sided: bool,
    ) {
        if mesh.index_count() > 0 {
            let normal = transform.normal_matrix();
            self.push(Some(mesh), transform, normal, sprite, colors, double_sided);
        }
    }

    /// How much of their own colors the meshes, models and billboards drawn in the view from now
    /// on show whatever lights them, each channel from black, nothing, as a view begins, to
    /// white, as full light would show them. It adds to the light that falls on them, which
    /// stops at white, and isn't shaded: they glow, though they light nothing round them.
    pub fn set_glow(&mut self, glow: Color) {
        let segment = self.canvas.segments.last_mut().expect("begin_3d adds one");
        segment.glow = glow;
    }

    /// Whether the meshes and models drawn in the view from now on cast shadows from its
    /// lights that have them ([`Light::shadows`]). They do as a view begins. Billboards never
    /// do: they face the camera, not the light.
    pub fn set_casts_shadow(&mut self, casts: bool) {
        let segment = self.canvas.segments.last_mut().expect("begin_3d adds one");
        segment.casting = casts;
    }

    /// Draws `sprite` standing upright with the middle of its bottom edge at `position`, `size`
    /// world units wide and tall, turned to face the camera across the ground, as characters do
    /// in 2.5D games: it stays upright in the world however far the camera looks down, and turns
    /// with the world in the view as the camera rolls. One a camera in perspective would see from
    /// behind, as below one looking steeply down, turns around to face it. A negative width
    /// mirrors it. It's lit as the ground is, whichever way it faces, and hides what's behind it as
    /// meshes do, so pixels of the sprite less than half opaque aren't drawn.
    pub fn draw_billboard(
        &mut self,
        sprite: &Sprite,
        position: [f32; 3],
        size: [f32; 2],
        tint: Color,
    ) {
        if size[0] == 0.0 || size[1] == 0.0 {
            return;
        }
        let segment = self.canvas.segments.last().expect("begin_3d adds one");
        let (mut right, mut facing) = segment.billboard_axes;
        // A camera in perspective sees one behind it across the ground from behind, as when it
        // looks steeply down on one below it, so that one turns around to face it.
        let camera = &segment.view.camera;
        if let Projection::Perspective { .. } = camera.projection
            && dot3(sub3(camera.position, position), facing) < 0.0
        {
            right = scale3(right, -1.0);
            facing = scale3(facing, -1.0);
        }
        let transform = Affine3 {
            x_axis: scale3(right, size[0]),
            y_axis: [0.0, size[1], 0.0],
            z_axis: facing,
            translation: position,
        };
        // Whichever way the quad faces, its normal points up, as the ground's do.
        let normal = [[0.0; 3], [0.0; 3], [0.0, 1.0, 0.0]];
        let colors = Colors {
            tint,
            emission: Color::BLACK,
            emission_sprite: None,
        };
        self.push(None, transform, normal, Some(sprite), colors, false);
    }

    /// Lights the view with `light`, as well as by its [`Lighting`]: everything drawn in it,
    /// before and after. A view is lit by [`Light::MAX`] lights at once, the nearest to its
    /// camera of those that reach what it sees. A light that can't light anything, such as one
    /// with no range, is left out.
    ///
    /// A billboard takes a light's whole level wherever the light is, having no side to turn
    /// away from it.
    pub fn add_light(&mut self, light: Light) {
        if light.problem().is_some() {
            return;
        }
        let canvas = &mut *self.canvas;
        canvas.lights.push(light);
        let segment = canvas.segments.last_mut().expect("begin_3d adds a segment");
        segment.lights.end += 1;
    }

    /// Records `mesh`, or the billboard quad for `None`, placed by `transform`, with `normal`
    /// moving its normals, colored by `colors` and glowing as the view now says, showing from
    /// behind too if `double_sided`.
    fn push(
        &mut self,
        mesh: Option<&Mesh>,
        transform: Affine3,
        normal: [[f32; 3]; 3],
        sprite: Option<&Sprite>,
        colors: Colors,
        double_sided: bool,
    ) {
        let determinant = transform.determinant();
        if !determinant.is_finite() {
            return;
        }
        let ([u0, v0], [u1, v1]) = sprite.map_or(([0.0, 0.0], [1.0, 1.0]), Sprite::uv_bounds);
        let canvas = &mut *self.canvas;
        let segment = canvas.segments.last_mut().expect("begin_3d adds a segment");
        segment.draws.end += 1;
        let casts_shadow = segment.casting && mesh.is_some();
        canvas.instances.push(Instance {
            model: [
                transform.x_axis,
                transform.y_axis,
                transform.z_axis,
                transform.translation,
            ],
            normal,
            uv: [u0, v0, u1, v1],
            tint: colors.tint.to_rgba8(),
            emission: colors.emission.to_rgba8(),
            glow: segment.glow.to_rgba8(),
        });
        canvas.mesh_draws.push(MeshDraw {
            mesh: mesh.cloned(),
            texture: sprite.map(|sprite| sprite.texture().clone()),
            emission_texture: colors
                .emission_sprite
                .map(|sprite| sprite.texture().clone()),
            mirrored: determinant < 0.0,
            double_sided,
            casts_shadow,
        });
    }

    /// The view being drawn, with its rect in canvas pixels.
    pub fn view(&self) -> &View3d {
        &self.canvas.segments.last().expect("begin_3d adds one").view
    }
}

/// A 3D view recorded on a canvas, drawn between its quads.
pub(crate) struct Segment3d {
    /// Its rect in canvas pixels.
    pub view: View3d,
    /// The batch of quads it's drawn before, which begins after it.
    pub before_batch: u32,
    /// Its meshes, in the canvas's `mesh_draws`.
    pub draws: Range<u32>,
    /// Its lights, in the canvas's `lights`.
    pub lights: Range<u32>,
    /// Whether the meshes drawn in it now cast shadows.
    pub casting: bool,
    /// How much of their own colors what's drawn in it now shows whatever lights it.
    pub glow: Color,
    /// The ways its billboards' right and front point ([`Camera3d::billboard_axes`]), the same
    /// for all of them.
    pub billboard_axes: ([f32; 3], [f32; 3]),
}

/// What a mesh drawn is colored by besides the view's light: its tint, and what it gives off.
#[derive(Clone, Copy)]
struct Colors<'a> {
    tint: Color,
    emission: Color,
    emission_sprite: Option<&'a Sprite>,
}

/// A mesh recorded in a 3D view. Its transform and colors are its [`Instance`].
pub(crate) struct MeshDraw {
    /// `None` for a billboard, which draws the renderer's quad.
    pub mesh: Option<Mesh>,
    /// `None` draws with a plain white texture.
    pub texture: Option<Texture>,
    /// What its emission is multiplied by: `None` for a plain white texture.
    pub emission_texture: Option<Texture>,
    /// Whether its transform mirrors it, which turns its triangles' winding round.
    pub mirrored: bool,
    /// Whether its triangles show from behind too.
    pub double_sided: bool,
    /// Whether it shades what's behind it from lights with shadows.
    pub casts_shadow: bool,
}

impl MeshDraw {
    /// What it's drawn as: draws of the same kind, the same mesh showing the same textures drawn
    /// the same way round, are drawn in one draw call, as instances of one mesh.
    fn kind(&self) -> DrawKind {
        DrawKind {
            mesh: self.mesh.as_ref().map_or(0, Mesh::id),
            texture: self.texture.as_ref().map_or(0, Texture::id),
            emission_texture: self.emission_texture.as_ref().map_or(0, Texture::id),
            mirrored: self.mirrored,
            double_sided: self.double_sided,
            casts_shadow: self.casts_shadow,
        }
    }
}

/// The kind of a [`MeshDraw`]: its mesh's and textures' IDs, 0 for none, which way round it's
/// drawn, and whether it casts shadows.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct DrawKind {
    mesh: usize,
    texture: usize,
    emission_texture: usize,
    mirrored: bool,
    double_sided: bool,
    casts_shadow: bool,
}

/// The order a 3D view's meshes draw in: draws of a kind together, so each kind is one draw call,
/// the kinds in the order each was first drawn, and each kind's in the order they were drawn, so
/// the order is the same every frame. Nothing in a view blends, and each view has a depth buffer,
/// so the order shows only in which of two surfaces at the same depth is in front. Kept from frame
/// to frame for what it allocates.
#[derive(Default)]
pub(crate) struct MeshOrder {
    /// Whether the draws draw in the order they were drawn, each kind's together already.
    pub in_order: bool,
    /// Unless they're in order, indices into the view's draws, in the order they draw.
    order: Vec<u32>,
    /// Where each run of the order that's one draw call ends.
    pub ends: Vec<u32>,
    kinds: HashMap<DrawKind, u32>,
    kind_of: Vec<u32>,
}

impl MeshOrder {
    /// Works out the order `draws` draw in.
    pub fn arrange(&mut self, draws: &[MeshDraw]) {
        self.kinds.clear();
        self.kind_of.clear();
        self.ends.clear();
        self.order.clear();
        // How many draws of each kind there are, in `ends` for now. Draws mostly come in runs of
        // a kind, which needn't be hashed.
        self.in_order = true;
        let mut last = None;
        for draw in draws {
            let kind = draw.kind();
            let index = match last {
                Some((was, index)) if was == kind => index,
                _ => {
                    let next = self.ends.len() as u32;
                    let index = *self.kinds.entry(kind).or_insert(next);
                    if index == next {
                        self.ends.push(0);
                    } else if self.in_order {
                        // The kinds so far came one after another, as many of each as counted.
                        self.in_order = false;
                        for (kind, &count) in self.ends.iter().enumerate() {
                            let kinds = std::iter::repeat_n(kind as u32, count as usize);
                            self.kind_of.extend(kinds);
                        }
                    }
                    index
                }
            };
            last = Some((kind, index));
            self.ends[index as usize] += 1;
            if !self.in_order {
                self.kind_of.push(index);
            }
        }
        if self.in_order {
            let mut end = 0;
            for count in &mut self.ends {
                end += *count;
                *count = end;
            }
            return;
        }
        // Where each kind starts, which becomes where it ends as its draws are put in order.
        let mut start = 0;
        for count in &mut self.ends {
            (*count, start) = (start, start + *count);
        }
        self.order.resize(draws.len(), 0);
        for (i, &kind) in self.kind_of.iter().enumerate() {
            let at = &mut self.ends[kind as usize];
            self.order[*at as usize] = i as u32;
            *at += 1;
        }
    }

    /// Which of the draws draws `position`th.
    pub fn draw_at(&self, position: u32) -> u32 {
        if self.in_order {
            position
        } else {
            self.order[position as usize]
        }
    }

    /// The draws in the order they draw, unless they're in order.
    pub fn order(&self) -> &[u32] {
        &self.order
    }

    /// How many kinds of draw `draws` are, and so how many draw calls they take.
    pub fn kind_count(draws: &[MeshDraw]) -> usize {
        let mut kinds = HashSet::new();
        let mut last = None;
        for draw in draws {
            let kind = draw.kind();
            if last != Some(kind) {
                kinds.insert(kind);
                last = Some(kind);
            }
        }
        kinds.len()
    }
}

/// How a mesh drawn is placed and colored, as the GPU reads it: 112 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct Instance {
    /// The transform's axes, then its translation.
    pub model: [[f32; 3]; 4],
    /// The columns of what moves its normals.
    pub normal: [[f32; 3]; 3],
    /// The sprite's texture coordinates, the top-left's then the bottom-right's.
    pub uv: [f32; 4],
    /// sRGB RGBA.
    pub tint: [u8; 4],
    /// What it gives off, sRGB RGB and nothing.
    pub emission: [u8; 4],
    /// How much of its own colors it shows whatever lights it, sRGB RGB and nothing.
    pub glow: [u8; 4],
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near<const N: usize>(actual: [f32; N], expected: [f32; N]) {
        assert!(
            (0..N).all(|i| (actual[i] - expected[i]).abs() < 1e-4),
            "{actual:?} is not {expected:?}"
        );
    }

    #[test]
    fn views_place_points_and_rays_where_their_rects_are() {
        // Twice as wide as it's tall, so it shows 4 units across, 10 pixels each.
        let camera = Camera3d::orthographic([0.0, 0.0, 5.0], [0.0; 3], 2.0);
        let view = View3d::new(camera, Rect::new(10.0, 20.0, 40.0, 20.0));
        assert_near(view.project([0.0; 3]).unwrap(), [30.0, 30.0]);
        assert_near(view.project([2.0, 1.0, 0.0]).unwrap(), [50.0, 20.0]);
        assert_eq!(view.project([0.0, 0.0, 10.0]), None, "behind the camera");

        let middle = view.ray([30.0, 30.0]);
        assert_near(middle.origin, [0.0, 0.0, 4.9]);
        assert_near(middle.direction, [0.0, 0.0, -1.0]);
        // Past the rect's right edge, as though the view went on.
        assert_near(view.ray([70.0, 30.0]).origin, [4.0, 0.0, 4.9]);
    }

    #[test]
    fn lights_fall_off_to_their_range_and_spots_light_their_cones() {
        let lamp = Light {
            energy: 2.0,
            ..Light::point([1.0, 2.0, 3.0], Color::WHITE, 4.0)
        };
        assert_eq!(lamp.level_at([1.0, 2.0, 3.0]), 2.0);
        assert_eq!(lamp.level_at([1.0, 0.0, 3.0]), 1.0, "halfway to its range");
        assert_eq!(lamp.level_at([5.0, 2.0, 3.0]), 0.0, "at its range");
        assert_eq!(lamp.level_at([1.0, 2.0, 9.0]), 0.0, "past it");

        // Shining down, on what's within a quarter turn's half of straight under it.
        let quarter = std::f32::consts::FRAC_PI_4;
        let spot = Light::spot(
            [0.0, 4.0, 0.0],
            [0.0, -3.0, 0.0],
            quarter,
            Color::WHITE,
            8.0,
        );
        assert_eq!(spot.level_at([0.0, 2.0, 0.0]), 0.75);
        assert!(spot.level_at([1.9, 2.0, 0.0]) > 0.0, "inside its rim");
        assert_eq!(spot.level_at([2.1, 2.0, 0.0]), 0.0, "outside it");
        assert_eq!(spot.level_at([0.0, 6.0, 0.0]), 0.0, "behind it");
        // With an edge, it fades from where the edge begins to the rim.
        let soft = Light { edge: 1.0, ..spot };
        let [near, far] = [0.5, 1.5].map(|x| soft.level_at([x, 2.0, 0.0]));
        assert!(far > 0.0 && far < near && near < 0.75, "{near} {far}");

        for (wrong, message) in [
            (Light { range: 0.0, ..lamp }, "range"),
            (
                Light {
                    energy: -1.0,
                    ..lamp
                },
                "energy",
            ),
            (Light { edge: 1.5, ..spot }, "edge"),
            (
                Light {
                    cone: Some(4.0),
                    ..spot
                },
                "cone",
            ),
            (
                Light {
                    direction: [0.0; 3],
                    ..spot
                },
                "direction",
            ),
            (
                Light {
                    position: [f32::NAN, 0.0, 0.0],
                    ..lamp
                },
                "position",
            ),
        ] {
            let problem = wrong.problem().expect("it can't light anything");
            assert!(problem.contains(message), "{problem}");
        }
        assert_eq!(lamp.problem(), None);
        assert_eq!(soft.problem(), None);
    }

    #[test]
    fn a_view_is_lit_by_the_nearest_lights_that_reach_what_it_sees() {
        // Looking along `-z` from the origin.
        let camera = Camera3d::perspective([0.0; 3], [0.0, 0.0, -1.0], 0.8);
        let view = View3d::new(camera, Rect::new(0.0, 0.0, 160.0, 90.0));
        let at = |position: [f32; 3]| Light::point(position, Color::WHITE, 2.0);
        let mut chosen = Vec::new();

        assert_eq!(lights_of(&view, &[], &mut chosen), 0);
        assert!(chosen.is_empty());

        // Ahead; behind, out of reach; behind, but reaching past the camera; far off to a side;
        // and off to a side, but reaching into view.
        let lights = [
            at([0.0, 0.0, -5.0]),
            at([0.0, 0.0, 5.0]),
            at([0.0, 0.0, 1.5]),
            at([50.0, 0.0, -5.0]),
            at([4.0, 0.0, -5.0]),
        ];
        assert_eq!(lights_of(&view, &lights, &mut chosen), 0);
        assert_eq!(chosen, [0, 2, 4]);

        // More than light a view at once: the nearest, in the order they were added.
        let row: Vec<Light> = (0..40).map(|i| at([0.0, 0.0, -1.0 - i as f32])).collect();
        let mut mixed = row.clone();
        mixed.reverse();
        assert_eq!(lights_of(&view, &mixed, &mut chosen), 8);
        assert_eq!(chosen, (8..40).collect::<Vec<u32>>());
        assert_eq!(lights_of(&view, &row, &mut chosen), 8);
        assert_eq!(chosen, (0..32).collect::<Vec<u32>>());
    }

    #[test]
    fn fog_thickens_in_a_straight_line_and_leaves_out_the_lights_past_it() {
        let fog = Fog::new(Color::WHITE, 10.0, 30.0);
        assert_eq!(fog.problem(), None);
        let amounts = [5.0, 10.0, 20.0, 30.0, 100.0].map(|distance| fog.amount_at(distance));
        assert_eq!(amounts, [0.0, 0.0, 0.5, 1.0, 1.0]);
        for (wrong, message) in [
            (Fog { near: -1.0, ..fog }, "near"),
            (Fog { far: 10.0, ..fog }, "far"),
            (
                Fog {
                    far: f32::INFINITY,
                    ..fog
                },
                "far",
            ),
        ] {
            let problem = wrong.problem().expect("it can't be");
            assert!(problem.contains(message), "{problem}");
        }

        // It's by how far a place is from a camera in perspective, and how far ahead of an
        // orthographic one.
        let rect = Rect::new(0.0, 0.0, 160.0, 90.0);
        let camera = Camera3d::perspective([0.0; 3], [0.0, 0.0, -1.0], 0.8);
        let view = View3d::new(camera, rect);
        assert_eq!(view.distance([3.0, 0.0, -4.0]), 5.0);
        let flat = View3d::new(
            Camera3d::orthographic([0.0; 3], [0.0, 0.0, -1.0], 2.0),
            rect,
        );
        assert_eq!(flat.distance([3.0, 0.0, -4.0]), 4.0);

        // A light none of whose reach is short of where the fog is whole lights nothing that
        // shows, so isn't one of the view's: the third's ends just there.
        let at = |z: f32| Light::point([0.0, 0.0, z], Color::WHITE, 2.0);
        let lights = [at(-5.0), at(-31.5), at(-32.0), at(-40.0)];
        let mut chosen = Vec::new();
        assert_eq!(lights_of(&view, &lights, &mut chosen), 0);
        assert_eq!(chosen, [0, 1, 2, 3]);
        let foggy = View3d {
            fog: Some(fog),
            ..view
        };
        assert_eq!(lights_of(&foggy, &lights, &mut chosen), 0);
        assert_eq!(chosen, [0, 1]);
        // Nor does it count as left out, however many there are.
        let row: Vec<Light> = (0..60).map(|i| at(-1.0 - i as f32)).collect();
        assert_eq!(lights_of(&foggy, &row, &mut chosen), 0);
        assert_eq!(chosen, (0..31).collect::<Vec<u32>>());
        // Fog that can't be is none.
        let wrong = View3d {
            fog: Some(Fog { far: 5.0, ..fog }),
            ..view
        };
        assert_eq!(wrong.fog(), None);
        assert_eq!(lights_of(&wrong, &lights, &mut chosen), 0);
        assert_eq!(chosen.len(), 4);
    }

    #[test]
    fn shadow_maps_go_to_the_lights_nearest_the_camera() {
        let camera = Camera3d::perspective([0.0; 3], [0.0, 0.0, -1.0], 0.8);
        let view = View3d::new(camera, Rect::new(0.0, 0.0, 160.0, 90.0));
        let point = |z: f32| Light {
            shadows: true,
            ..Light::point([0.0, 0.0, z], Color::WHITE, 2.0)
        };
        let spot = |z: f32, cone: f32| Light {
            shadows: true,
            ..Light::spot([0.0, 0.0, z], [0.0, -1.0, 0.0], cone, Color::WHITE, 2.0)
        };
        // A narrow spot light's shadows are in one map, and any other light's in six.
        assert_eq!(point(-1.0).shadow_maps(), 6);
        assert_eq!(spot(-1.0, 0.5).shadow_maps(), 1);
        assert_eq!(spot(-1.0, Light::SHADOW_CONE).shadow_maps(), 1);
        assert_eq!(spot(-1.0, 1.2).shadow_maps(), 6);
        assert_eq!(point(-1.0).shadow_views().count(), 6);
        assert_eq!(spot(-1.0, 0.5).shadow_views().count(), 1);

        let mut chosen = Vec::new();
        let mut shadowed = Vec::new();
        let mut shadows = |lights: &[Light]| {
            lights_of(&view, lights, &mut chosen);
            let counts = shadows_of(&view, lights, &chosen, &mut shadowed);
            (counts, shadowed.clone())
        };
        // Lights without shadows take none.
        let plain = Light::point([0.0, 0.0, -1.0], Color::WHITE, 2.0);
        assert_eq!(shadows(&[plain]), ([0, 0], vec![]));
        // The nearest first, whatever order they're in: each is its place among the view's
        // lights, and the first map it takes.
        let lights = [point(-3.0), plain, spot(-1.0, 0.5), point(-2.0)];
        assert_eq!(shadows(&lights), ([13, 0], vec![(2, 0), (3, 1), (0, 7)]));
        // Three point lights and a spot light take nineteen maps, so a fourth point light,
        // farther off, goes without: and a spot light farther still takes one of what's left.
        let row = [
            point(-4.0),
            point(-3.0),
            point(-2.0),
            point(-1.0),
            spot(-3.5, 0.5),
            spot(-5.0, 0.5),
        ];
        let taken = vec![(3, 0), (2, 6), (1, 12), (4, 18), (5, 19)];
        assert_eq!(shadows(&row), ([20, 1], taken));
    }

    #[test]
    fn a_shadow_map_takes_in_its_lights_cone() {
        let transform = |matrix: &Mat4, [x, y, z]: [f32; 3]| {
            let clip = [0, 1, 2, 3]
                .map(|i| matrix[0][i] * x + matrix[1][i] * y + matrix[2][i] * z + matrix[3][i]);
            [
                clip[0] / clip[3],
                clip[1] / clip[3],
                clip[2] / clip[3],
                clip[3],
            ]
        };
        // Shining down from 4 up, on what's within half a radian of straight under it.
        let spot = Light {
            shadows: true,
            ..Light::spot([0.0, 4.0, 0.0], [0.0, -1.0, 0.0], 0.5, Color::WHITE, 8.0)
        };
        let onto_map = spot.shadow_views().next().unwrap();
        // Straight under it is the map's middle, as far along it as it is from the light.
        let [x, y, depth, along] = transform(&onto_map, [0.0, 1.0, 0.0]);
        assert_near([x, y, along], [0.0, 0.0, 3.0]);
        assert!(depth > 0.0 && depth < 1.0, "{depth}");
        // Its cone's rim is just inside the map's edge, and its range the map's far side.
        let rim = 3.0 * 0.5_f32.tan();
        let [x, ..] = transform(&onto_map, [rim, 1.0, 0.0]);
        assert!(x.abs() > 0.9 && x.abs() < 1.0, "{x}");
        assert_near([transform(&onto_map, [0.0, -4.0, 0.0])[2]], [1.0]);
        // A texel is its part of the map's width, at a world unit from the light.
        let width = 2.0 * (0.5_f32 + 0.02).tan();
        assert_near([spot.shadow_texel() * 512.0], [width]);

        // A point light's six look along each axis, each taking in a quarter turn.
        let lamp = Light::point([1.0, 2.0, 3.0], Color::WHITE, 8.0);
        let ways = [
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ];
        for (onto_map, way) in lamp.shadow_views().zip(ways) {
            let ahead = add3(lamp.position, scale3(way, 2.0));
            let [x, y, _, along] = transform(&onto_map, ahead);
            assert_near([x, y, along], [0.0, 0.0, 2.0]);
            let behind = add3(lamp.position, scale3(way, -2.0));
            assert!(transform(&onto_map, behind)[3] < 0.0, "{way:?}");
        }
        assert_near([lamp.shadow_texel() * 512.0], [2.0]);
    }

    #[test]
    fn draws_of_a_kind_draw_together_in_the_order_each_kind_was_first_drawn() {
        // Billboards of three kinds, by which way round they're drawn.
        let draw = |mirrored, double_sided| MeshDraw {
            mesh: None,
            texture: None,
            emission_texture: None,
            mirrored,
            double_sided,
            casts_shadow: false,
        };
        let [a, b, c] = [(false, false), (true, false), (false, true)];
        let draws = [a, b, a, a, c, b].map(|(mirrored, sided)| draw(mirrored, sided));
        let mut order = MeshOrder::default();
        order.arrange(&draws);
        assert!(!order.in_order);
        let drawn: Vec<u32> = (0..6).map(|i| order.draw_at(i)).collect();
        assert_eq!(drawn, [0, 2, 3, 1, 5, 4]);
        assert_eq!(order.ends, [3, 5, 6]);
        assert_eq!(MeshOrder::kind_count(&draws), 3);

        // Kept from frame to frame, it starts again each time. Draws of each kind together
        // already draw as they are.
        order.arrange(&draws[2..]);
        assert!(order.in_order);
        assert_eq!(
            (0..4).map(|i| order.draw_at(i)).collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
        assert_eq!(order.ends, [2, 3, 4]);
        order.arrange(&[]);
        assert!(order.in_order && order.ends.is_empty());
        assert_eq!(MeshOrder::kind_count(&[]), 0);
    }
}
