use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use bytemuck::{Pod, Zeroable};

use super::{
    Align, Blend, Camera3d, Canvas, Color, Font, Mesh, Model, Projection, Ray, Sprite, Texture,
    camera3d::{Basis, Mat4},
};
use crate::{
    math::{Affine3, Bounds3, Rect, add3, cross3, dot3, normalize3, scale3, sub3},
    particles::{Look, Particles3d},
};

/// What moves a billboard's normals: nothing across or up it, and up for its front, so its
/// normals point up whichever way it faces, as the ground's do; which the shader knows it by.
const BILLBOARD_NORMAL: [[f32; 3]; 3] = [[0.0; 3], [0.0; 3], [0.0, 1.0, 0.0]];

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
    /// What it shows wherever nothing is drawn, if anything, which its fog fades into. A sky
    /// that can't be is none.
    pub sky: Option<Sky>,
}

impl View3d {
    /// A view through `camera` filling `rect`, lit by [`Lighting::default`], without fog or a
    /// sky.
    pub fn new(camera: Camera3d, rect: Rect) -> Self {
        Self {
            camera,
            rect,
            lighting: Lighting::default(),
            fog: None,
            sky: None,
        }
    }

    /// Its fog, if it has one that can be.
    pub(crate) fn fog(&self) -> Option<Fog> {
        self.fog.filter(|fog| fog.problem().is_none())
    }

    /// Its sky, if it has one that can be.
    pub(crate) fn sky(&self) -> Option<Sky> {
        self.sky.filter(|sky| sky.problem().is_none())
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
    /// How the sun casts shadows, or `None` for a sun that casts none. Shadows that can't be,
    /// such as ones with no `reach`, are none, and so are those of a sun that gives no light.
    pub shadows: Option<SunShadows>,
}

impl Lighting {
    /// No shading: every surface shows as it is.
    pub const UNLIT: Self = Self {
        direction: [0.0, -1.0, 0.0],
        color: Color::BLACK,
        ambient: Color::WHITE,
        shadows: None,
    };
}

impl Default for Lighting {
    /// A sun high above and a little in front, to the right, so the tops of things are brightest,
    /// then their fronts, then their right-hand sides. It casts no shadows.
    fn default() -> Self {
        Self {
            direction: [-0.3, -1.0, -0.5],
            color: Color::rgb(0.8, 0.8, 0.8),
            ambient: Color::rgb(0.5, 0.5, 0.5),
            shadows: None,
        }
    }
}

/// How a view's sun casts shadows ([`Lighting::shadows`]): the view's meshes and models in the
/// sunlight's way shade what's behind them, but for those drawn after
/// [`Canvas3d::set_casts_shadow`] says not, and never its billboards, as from a [`Light`] with
/// shadows. A place in shadow gets less of the sun's light, and all of the view's other light.
///
/// The shadows are in one map of the sun's own, [`SunShadows::RESOLUTION`] texels across and
/// down, which looks along the sunlight at what's within `reach` of the point the camera looks
/// at ([`Camera3d::target`](super::Camera3d::target)). Nothing farther from that point is
/// shaded, though what's up the sunlight from there shades what's within, however far up it
/// is. Shadows have hard edges, a texel of the map wide ([`SunShadows::texel`]): the shorter
/// the reach, the finer. The map moves by whole texels as the camera does, so their edges keep
/// still on what they fall on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunShadows {
    /// How far from the point the camera looks at they reach, in world units: above 0.
    pub reach: f32,
    /// How much of the sun's light a shadow takes, from 0, none of it, to 1, all of it.
    pub darkness: f32,
}

impl SunShadows {
    /// How many texels across and down the sun's shadow map is. It's made when a view's sun
    /// first casts shadows, so a game without any doesn't hold it.
    pub const RESOLUTION: u32 = 2048;
    /// How many texels of the map lie past `reach` at each of its edges: room for the half a
    /// texel the map is moved by, and for how far off a surface it's looked up.
    const MARGIN: u32 = 3;

    /// Shadows reaching `reach` world units from the point the camera looks at, which take all
    /// of the sun's light.
    pub const fn new(reach: f32) -> Self {
        Self {
            reach,
            darkness: 1.0,
        }
    }

    /// What's wrong with them, if they can't be.
    pub(crate) fn problem(&self) -> Option<String> {
        if !(self.reach > 0.0 && self.reach.is_finite()) {
            return Some(format!(
                "the reach of a sun's shadows must be above 0, not {}",
                self.reach
            ));
        }
        if !(0.0..=1.0).contains(&self.darkness) {
            return Some(format!(
                "the darkness of a sun's shadows is from 0 to 1, not {}",
                self.darkness
            ));
        }
        None
    }

    /// How large a texel of the sun's shadow map is, in world units: how wide a shadow's edge
    /// is, and the steps along it.
    pub fn texel(&self) -> f32 {
        2.0 * self.reach / (Self::RESOLUTION - 2 * Self::MARGIN) as f32
    }
}

/// A view's sun's shadow map, as the renderer draws it and `mesh.wgsl` reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SunMap {
    /// What maps the world onto the map: from -1 to 1 across and up it, and from 0, nearest the
    /// sun, to 1 along its light.
    pub onto_map: Mat4,
    /// How large a texel of it is, in world units.
    pub texel: f32,
    /// The point shadows reach out from, and how far.
    pub center: [f32; 3],
    pub reach: f32,
    /// How much of the sun's light a shadow takes.
    pub darkness: f32,
}

impl View3d {
    /// Its sun's shadow map, if its sun casts shadows that can be: one looking along the
    /// sunlight, without perspective, at the box that reaches a little past the shadows' reach
    /// every way from the point the camera looks at. Which ways are across and up the map depend
    /// only on the sunlight's, and the map's middle is a whole number of texels from the world's
    /// origin, so texels stay where they are in the world as the camera moves.
    pub(crate) fn sun_map(&self) -> Option<SunMap> {
        let lighting = &self.lighting;
        let shadows = lighting.shadows.filter(|s| s.problem().is_none())?;
        let direction = normalize3(lighting.direction).filter(|d| d.iter().all(|v| v.is_finite()));
        let direction = direction?;
        let [r, g, b, _] = lighting.color.to_rgba8();
        if [r, g, b] == [0; 3] || shadows.darkness == 0.0 {
            return None;
        }
        let center = self.camera.target;
        if !center.iter().all(|v| v.is_finite()) {
            return None;
        }
        let Basis { right, up, forward } = Camera3d::orthographic([0.0; 3], direction, 1.0).basis();
        let texel = shadows.texel();
        let half = texel * (SunShadows::RESOLUTION / 2) as f32;
        let whole = |along: f32| (along / texel).round() * texel;
        let [across, over] = [whole(dot3(right, center)), whole(dot3(up, center))];
        let deep = dot3(forward, center);
        let column = |axis: usize| {
            [
                right[axis] / half,
                up[axis] / half,
                forward[axis] / (2.0 * half),
                0.0,
            ]
        };
        let onto_map = [
            column(0),
            column(1),
            column(2),
            [-across / half, -over / half, 0.5 - deep / (2.0 * half), 1.0],
        ];
        Some(SunMap {
            onto_map,
            texel,
            center,
            reach: shadows.reach,
            darkness: shadows.darkness,
        })
    }
}

/// Fog in a 3D view: a color that what's drawn in it fades to with its distance from the camera
/// ([`View3d::distance`]), in a straight line from none at `near` to nothing but fog at `far`
/// and beyond. It's over the view's meshes, models and billboards alike, lit or not.
///
/// In a view with a [`Sky`], what's drawn fades into the sky instead, the way it's seen from the
/// camera, and `color` goes unused. A view without one draws no background, so what's behind
/// the fog is what the canvas was cleared to, or had drawn on it: clear it to the fog's color
/// for the far side of a world to fade into it.
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

/// A 3D view's sky: what it shows wherever nothing is drawn, all round the camera, as though
/// infinitely far off, so it turns as the camera does and never comes nearer. It's a gradient by
/// the height of the way each pixel looks: `horizon` level with the camera, becoming `zenith`
/// up to `band` radians over it and `ground` down to `band` under it, in a straight line in the
/// way's height, mixed in linear light ([`Sky::color_at`]).
///
/// It's drawn behind everything in its view, over what was drawn on the canvas before the view,
/// so a view with a sky covers its rect. Its [`Fog`] fades into it. It lights nothing: it isn't
/// the view's [`Lighting`], and takes none of its lights or shadow maps. Colors' alphas are
/// ignored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sky {
    /// Its color straight up, and from `band` up.
    pub zenith: Color,
    /// Its color level with the camera.
    pub horizon: Color,
    /// Its color straight down, and from `band` down.
    pub ground: Color,
    /// How far up and down from the horizon, in radians, it becomes the zenith's color and the
    /// ground's: above 0. Past a quarter turn is a quarter turn.
    pub band: f32,
}

impl Sky {
    /// The band of a sky that says none: a quarter turn, so it changes all the way up.
    pub const WHOLE_BAND: f32 = std::f32::consts::FRAC_PI_2;

    /// A sky of one color every way, as a view without one shows the canvas cleared to it.
    pub const fn flat(color: Color) -> Self {
        Self {
            zenith: color,
            horizon: color,
            ground: color,
            band: Self::WHOLE_BAND,
        }
    }

    /// What's wrong with it, if it can't be.
    pub(crate) fn problem(&self) -> Option<String> {
        if !(self.band > 0.0 && self.band.is_finite()) {
            return Some(format!("a sky's band must be above 0, not {}", self.band));
        }
        None
    }

    /// 1 over the sine of its band: how many times its height, in a way of length 1, a way is
    /// along from its horizon to its zenith or its ground.
    pub(crate) fn steepness(&self) -> f32 {
        1.0 / self.band.min(Self::WHOLE_BAND).sin()
    }

    /// Its color the way `way` looks from the camera, as the view draws it: the length of `way`
    /// doesn't matter, and one of no length looks at the horizon.
    pub fn color_at(&self, way: [f32; 3]) -> Color {
        let height = normalize3(way).map_or(0.0, |[_, y, _]| y);
        let along = (height.abs() * self.steepness()).min(1.0);
        let toward = if height >= 0.0 {
            self.zenith
        } else {
            self.ground
        };
        let [from, to] = [self.horizon, toward].map(Color::to_linear);
        let [r, g, b] = [0, 1, 2].map(|i| from[i] + (to[i] - from[i]) * along);
        Color::from_linear([r, g, b, 1.0])
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
    /// The box it lights within, if it keeps to one: what takes the world onto the cube from -1
    /// to 1 each way, inside which, faces and all, it lights what it reaches, and outside which
    /// it lights nothing, whatever its range. A box keeps a light without shadows from lighting
    /// through walls and floors, as a room's. [`Light::within`] makes one from a box's corners.
    pub bounds: Option<Affine3>,
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
            bounds: None,
        }
    }

    /// The light, keeping to `bounds`, a box placed by `placed`: a room's inside, say, in the
    /// room's coordinates, and where the room is. `None` if the box placed so has no room in it,
    /// as when `placed` squashes it flat.
    pub fn within(self, bounds: Bounds3, placed: Affine3) -> Option<Self> {
        let half = bounds.size().map(|size| size / 2.0);
        // What takes the cube from -1 to 1 onto the box, in the world.
        let cube = Affine3 {
            x_axis: [half[0], 0.0, 0.0],
            y_axis: [0.0, half[1], 0.0],
            z_axis: [0.0, 0.0, half[2]],
            translation: bounds.center(),
        };
        let onto_cube = (placed * cube).inverse()?;
        let finite = |v: [f32; 3]| v.iter().all(|v| v.is_finite());
        let axes = [
            onto_cube.x_axis,
            onto_cube.y_axis,
            onto_cube.z_axis,
            onto_cube.translation,
        ];
        axes.into_iter().all(finite).then_some(Self {
            bounds: Some(onto_cube),
            ..self
        })
    }

    /// Whether `point` is in its box, faces and all, or it has none. A small allowance for
    /// world-to-box roundoff keeps points on a face inside, as the shader does.
    pub fn bounds_hold(&self, point: [f32; 3]) -> bool {
        self.bounds.is_none_or(|onto_cube| {
            let along = onto_cube.apply(point);
            (0..3).all(|axis| {
                // Match mesh.wgsl: allow the roundoff of the world-to-box calculation, not
                // a fixed world-space margin that would fail when a room moves or stretches.
                let terms = (onto_cube.x_axis[axis] * point[0]).abs()
                    + (onto_cube.y_axis[axis] * point[1]).abs()
                    + (onto_cube.z_axis[axis] * point[2]).abs()
                    + onto_cube.translation[axis].abs();
                let rounding = terms.max(1.0) * (16.0 * f32::EPSILON);
                along[axis].abs() <= 1.0 + rounding
            })
        })
    }

    /// The corners of its box in the world, if it has one.
    fn corners(&self) -> Option<[[f32; 3]; 8]> {
        let onto_world = self.bounds?.inverse()?;
        Some(std::array::from_fn(|corner| {
            let pick = |axis: usize| if corner & (1 << axis) == 0 { -1.0 } else { 1.0 };
            onto_world.apply([pick(0), pick(1), pick(2)])
        }))
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
        if let Some(onto_cube) = self.bounds {
            let rows = [
                onto_cube.x_axis,
                onto_cube.y_axis,
                onto_cube.z_axis,
                onto_cube.translation,
            ];
            if !rows.into_iter().all(finite) || onto_cube.inverse().is_none() {
                return Some("a light's bounds must be finite, with room in them".into());
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
    /// falling in a straight line to 0 at its range, and for a spot light, 0 outside its cone,
    /// and for one that keeps to a box, 0 outside that. It's what a surface there facing the
    /// light squarely is lit by, as the view draws it.
    pub fn level_at(&self, point: [f32; 3]) -> f32 {
        if !self.bounds_hold(point) {
            return 0.0;
        }
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

/// Which of `lights` light `view`, into `chosen` as indices into them: those whose reach, and
/// box if they keep to one, are in what its camera sees, short of where its fog hides
/// everything, and when there are more than [`Light::MAX`], the nearest to the camera, in the
/// order they were added. Returns how many it left out for being too many.
pub(crate) fn lights_of(view: &View3d, lights: &[Light], chosen: &mut Vec<u32>) -> u32 {
    chosen.clear();
    if lights.is_empty() {
        return 0;
    }
    let planes = sides_of(view);
    // What a light must reach nearer than to light anything that shows through the fog.
    let hidden = view.fog().map(|fog| fog.far);
    let reaches = |light: &Light| {
        if hidden.is_some_and(|far| view.distance(light.position) - light.range >= far) {
            return false;
        }
        // A box wholly outside a side is out of sight, as a reach is.
        let corners = light.corners();
        planes.iter().all(|&[a, b, c, d]| {
            let length = (a * a + b * b + c * c).sqrt();
            let [px, py, pz] = light.position;
            // A plane the matrix doesn't give, as of a view with no size, cuts nothing off.
            if length > 0.0 {
                let inside = |[x, y, z]: [f32; 3]| (a * x + b * y + c * z + d) / length;
                inside([px, py, pz]) >= -light.range
                    && corners.is_none_or(|corners| corners.into_iter().any(|at| inside(at) >= 0.0))
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

/// The sides of what `view`'s camera sees, each facing inward: left and right, bottom and top,
/// near and far, each `[a, b, c, d]`, with a point on its inside where `a * x + b * y + c * z + d`
/// is 0 or more. A side the camera's matrix doesn't give, as of a view with no size, is nothing
/// but zeros, which leaves nothing outside it.
fn sides_of(view: &View3d) -> [[f32; 4]; 6] {
    let aspect = view.rect.w / view.rect.h;
    let matrix = view.camera.view_projection(aspect);
    let row = |i: usize| [matrix[0][i], matrix[1][i], matrix[2][i], matrix[3][i]];
    let [x, y, z, w] = [row(0), row(1), row(2), row(3)];
    let add = |a: [f32; 4], b: [f32; 4]| [0, 1, 2, 3].map(|i| a[i] + b[i]);
    let sub = |a: [f32; 4], b: [f32; 4]| [0, 1, 2, 3].map(|i| a[i] - b[i]);
    [add(w, x), sub(w, x), add(w, y), sub(w, y), z, sub(w, z)]
}

/// How much more a box takes up than it does, as a part of how far it is from the world's origin
/// and how far it reaches, and how far past where a view's fog is whole it must be, as a part of
/// that, to be out of sight: room for the roundoff between the box's corners worked out on the
/// CPU and a mesh's placed by the GPU.
const ROUNDOFF: f32 = 1e-5;

/// What a 3D view can show: the sides of what its camera sees, and in a view with a sky, how far
/// its fog lets anything show. A mesh wholly outside it is left out of the view, having nothing
/// that would show (see [`Canvas3d`]).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Sight {
    /// The sides, as [`sides_of`] gives them, by their terms: every side's `a`, then every
    /// side's `b`, `c` and `d`, so the sides are tested together.
    sides: [[f32; 6]; 4],
    /// How far a box reaching 1 along each axis from its middle reaches across each side: the
    /// sizes of every side's `a`, `b` and `c`.
    spans: [[f32; 6]; 3],
    /// Where the camera is, which the fog measures from.
    eye: [f32; 3],
    /// For an orthographic camera, the way it looks: its fog measures how far ahead of it
    /// things are, as there's no place it is to be far from.
    ahead: Option<[f32; 3]>,
    /// How far off the fog hides everything, in a view with a sky, past which a mesh shows as
    /// the very sky that shows without it. `None` without fog or a sky, or both.
    hidden: Option<f32>,
}

/// How much of a box a view's [`Sight`] takes in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Seen {
    /// None of it: it's wholly outside one of the view's sides, or past where its fog hides
    /// everything.
    Not,
    /// Some of it, or perhaps all.
    Partly,
    /// All of it: it's inside every side, and none of it is past the fog.
    Wholly,
}

impl Sight {
    /// Everything: what a view sees that leaves nothing out, as tests draw to hold what's left
    /// out to what shows.
    #[cfg(test)]
    pub(crate) const EVERYTHING: Self = Self {
        sides: [[0.0; 6]; 4],
        spans: [[0.0; 6]; 3],
        eye: [0.0; 3],
        ahead: None,
        hidden: None,
    };

    /// What `view` can show.
    pub(crate) fn of(view: &View3d) -> Self {
        let camera = &view.camera;
        let ahead = match camera.projection {
            Projection::Perspective { .. } => None,
            Projection::Orthographic { .. } => Some(camera.forward()),
        };
        let sides = sides_of(view);
        let terms = [0, 1, 2, 3].map(|term| sides.map(|side| side[term]));
        Self {
            sides: terms,
            spans: [0, 1, 2].map(|term| terms[term].map(f32::abs)),
            eye: camera.position,
            ahead,
            hidden: view.sky().and(view.fog()).map(|fog| fog.far),
        }
    }

    /// How much of `bounds`, a box in the world, it takes in.
    #[cfg(test)]
    pub(crate) fn sees(&self, bounds: &Bounds3) -> Seen {
        self.sees_placed(bounds, &Affine3::IDENTITY)
    }

    /// How much of `bounds` placed by `transform` it takes in: of the smallest box round it with
    /// its sides along the world's axes, as [`Bounds3::transformed`] gives. A box that isn't
    /// numbers is taken in partly, as nothing can be said of it.
    pub(crate) fn sees_placed(&self, bounds: &Bounds3, transform: &Affine3) -> Seen {
        // The box round it in the world, by its middle and how far it reaches from there along
        // each axis, which is quicker to work out than its corners and to test.
        let middle = transform.apply(bounds.center());
        let half = bounds.size().map(|size| size / 2.0);
        let axes = [transform.x_axis, transform.y_axis, transform.z_axis];
        // Grown by a little of how far it is from the world's origin, for the roundoff between
        // its corners worked out here and a mesh's placed by the GPU.
        let reach = [0, 1, 2].map(|i| {
            let reach = (axes[0][i] * half[0]).abs()
                + (axes[1][i] * half[1]).abs()
                + (axes[2][i] * half[2]).abs();
            reach + (middle[i].abs() + reach) * ROUNDOFF
        });
        if !middle.iter().chain(&reach).all(|v| v.is_finite()) {
            return Seen::Partly;
        }
        let [x, y, z] = middle;
        let ([a, b, c, d], [sa, sb, sc]) = (&self.sides, &self.spans);
        let (mut outside, mut wholly) = (false, true);
        // Every side in turn, with nothing to stop for, which tests them a few at once.
        for side in 0..6 {
            // How far inside the side its middle is, and how far it reaches across it.
            let inside = a[side] * x + b[side] * y + c[side] * z + d[side];
            let across = sa[side] * reach[0] + sb[side] * reach[1] + sc[side] * reach[2];
            outside |= inside + across < 0.0;
            wholly &= inside >= across;
        }
        if outside {
            return Seen::Not;
        }
        if let Some(hidden) = self.hidden {
            let [past, within] = [hidden * (1.0 + ROUNDOFF), hidden * (1.0 - ROUNDOFF)];
            let away = sub3(middle, self.eye);
            let (wholly_past, wholly_within) = match self.ahead {
                // How far ahead of the camera its nearest corner and its farthest are.
                Some(ahead) => {
                    let along = dot3(away, ahead);
                    let across: f32 = (0..3).map(|i| ahead[i].abs() * reach[i]).sum();
                    (along - across > past, along + across < within)
                }
                // How far from the camera its nearest point and its farthest corner are.
                None => {
                    let [mut nearest, mut farthest] = [0.0f32; 2];
                    for i in 0..3 {
                        let off = away[i].abs();
                        let near = (off - reach[i]).max(0.0);
                        let far = off + reach[i];
                        nearest += near * near;
                        farthest += far * far;
                    }
                    (nearest > past * past, farthest < within * within)
                }
            };
            if wholly_past {
                return Seen::Not;
            }
            wholly &= wholly_within;
        }
        if wholly { Seen::Wholly } else { Seen::Partly }
    }
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
/// Meshes hide what's behind them, whatever order they're drawn in. Draws of the same mesh,
/// showing the same textures the same way, are one draw call, wherever they're drawn among the
/// view's others; and those of meshes showing the same textures the same way are drawn one after
/// another, binding nothing between them, as all meshes' corners are in buffers they share.
///
/// A mesh, or a model's part, that couldn't show is left out of the view: one whose box, placed as
/// it's drawn, is wholly outside what the camera sees, beside, above, below, nearer than its
/// `near` or farther than its `far`; or in a view with a [`Sky`], wholly past where its [`Fog`] is
/// whole, where it would show as the very sky that shows without it. A model wholly in sight or
/// out of it is tested once, by its box ([`Model::bounds`]), and one partly in sight part by
/// part. What's left out would have shown nothing, so the view looks the same; one that casts
/// shadows still draws into the view's shadow maps, since its shadow may fall in sight, and is
/// left out of the view itself. Billboards, text and particles are drawn whatever.
/// [`FrameStats::out_of_sight`](crate::FrameStats::out_of_sight) counts what's left out.
pub struct Canvas3d<'a> {
    pub(super) canvas: &'a mut Canvas,
}

impl Canvas3d<'_> {
    /// Draws `mesh` placed by `transform`, showing `sprite`, or white without one, multiplied by
    /// the mesh's colors and by `tint`. Pixels of the sprite less than half opaque aren't drawn,
    /// so what's behind shows through them; the rest are drawn opaque, unless the view's meshes
    /// now blend ([`set_blend`](Self::set_blend)).
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
        self.draw_sides(mesh, transform, sprite, colors, false, None);
    }

    /// Draws each part of `model` as [`draw_mesh`](Self::draw_mesh) would, placed in the model,
    /// and the model placed by `transform`, all multiplied by `tint`: a draw call for each part.
    /// Double-sided parts show from behind too, and each part gives off its
    /// [`emission`](ModelPart::emission).
    pub fn draw_model(&mut self, model: &Model, transform: Affine3, tint: Color) {
        let in_sight = self.model_in_sight(model, &transform);
        for part in model.parts() {
            let placed = transform * part.transform;
            let sprite = part.sprite.as_ref();
            let colors = Colors {
                tint,
                emission: part.emission,
                emission_sprite: part.emission_sprite.as_ref(),
            };
            let sides = part.double_sided;
            self.draw_sides(&part.mesh, placed, sprite, colors, sides, in_sight);
        }
    }

    /// Whether `model`, placed by `transform`, is wholly in sight or wholly out of it, by its
    /// box; or `None` if it's partly in sight, or one of its parts' meshes has been written since
    /// it was made, which may have moved it out of the box, so each part is tested on its own.
    fn model_in_sight(&self, model: &Model, transform: &Affine3) -> Option<bool> {
        if model.parts().iter().any(|part| part.mesh.is_rewritten()) {
            return None;
        }
        let segment = self.canvas.segments.last().expect("begin_3d adds one");
        match segment.sight.sees_placed(&model.bounds()?, transform) {
            Seen::Not => Some(false),
            Seen::Partly => None,
            Seen::Wholly => Some(true),
        }
    }

    /// Draws `mesh` as [`draw_mesh`](Self::draw_mesh) does, from behind too if `double_sided`,
    /// unless it's out of sight and casts no shadow: as `in_sight` says, or by its box if that's
    /// `None`.
    fn draw_sides(
        &mut self,
        mesh: &Mesh,
        transform: Affine3,
        sprite: Option<&Sprite>,
        colors: Colors,
        double_sided: bool,
        in_sight: Option<bool>,
    ) {
        if mesh.index_count() == 0 {
            return;
        }
        let segment = self.canvas.segments.last_mut().expect("begin_3d adds one");
        let mut how = How::of(segment);
        if segment.blend.is_some() {
            // Blended, it's drawn after the view's opaque meshes, by how far its origin is from
            // the camera, and shades nothing behind it.
            how.blend = segment.blend;
            how.depth = segment.view.distance(transform.translation);
            how.casts_shadow = false;
        }
        how.in_sight = in_sight.unwrap_or_else(|| {
            let bounds = mesh.bounds();
            bounds.is_none_or(|bounds| segment.sight.sees_placed(&bounds, &transform) != Seen::Not)
        });
        if !how.in_sight && !how.casts_shadow {
            segment.out_of_sight += 1;
            return;
        }
        let normal = transform.normal_matrix();
        self.push(
            Some(mesh),
            transform,
            normal,
            sprite,
            colors,
            double_sided,
            how,
        );
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

    /// How the meshes and models drawn in the view from now on blend with what's behind them:
    /// `None`, as a view begins, draws them opaque. Blended, a mesh is as see-through as its
    /// colors say, its sprite's alpha times its vertices' times its tint's, with none of its
    /// pixels left out for being under half opaque. It's drawn after the view's opaque meshes,
    /// hidden by what's in front of it and hiding nothing, as blended particles are: among
    /// those blended normally, the farthest from the camera first, by where its transform puts
    /// its origin. It shows both its sides, casts no shadow, and is lit, glows and fades into
    /// fog as an opaque mesh does. Billboards and text never blend.
    ///
    /// A mesh is ordered whole, by that one distance: two see-through meshes that cross show
    /// one wholly over the other, and one seen through itself, such as a box or a glass ball,
    /// shows its far side through its near one, in the order its triangles come. A pane is
    /// one layer if it's a flat mesh, not a thin box.
    pub fn set_blend(&mut self, blend: Option<Blend>) {
        let segment = self.canvas.segments.last_mut().expect("begin_3d adds one");
        segment.blend = blend;
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
        let axes = self.billboard_axes_at(position);
        self.push_billboard(sprite, position, axes, size, tint);
    }

    /// Draws `text` in `font` standing upright as a sign, facing the camera across the ground as
    /// a billboard does: its lines `height` world units apart, with the middle of its bottom edge
    /// at `position`, or its left or right end as `align` says, which lines its lines up too.
    /// `\n` starts a new line. Each glyph is a billboard showing the font's own texture,
    /// multiplied by `tint`, so the text hides what's behind it and is hidden by what's in front,
    /// its pixels less than half opaque left out, and is lit as billboards are.
    pub fn draw_text(
        &mut self,
        font: &Font,
        text: &str,
        position: [f32; 3],
        height: f32,
        align: Align,
        tint: Color,
    ) {
        let line_height = font.line_height();
        if !(height > 0.0 && line_height > 0.0) {
            return;
        }
        // World units to a pixel of the font's.
        let scale = height / line_height;
        let layout = font.layout_aligned(text, None, align);
        let [width, tall] = layout.size();
        let from_left = match align {
            Align::Left => 0.0,
            Align::Center => width / 2.0,
            Align::Right => width,
        };
        let (right, facing) = self.billboard_axes_at(position);
        // The texture of the page the last glyph was on, which the next is likely on too.
        let mut page: Option<(u32, Texture)> = None;
        for glyph in layout.glyphs() {
            let rect = glyph.rect;
            if rect.w <= 0.0 || rect.h <= 0.0 {
                continue;
            }
            if page
                .as_ref()
                .is_none_or(|(number, _)| *number != glyph.page)
            {
                page = Some((glyph.page, font.page(glyph.page)));
            }
            let (_, texture) = page.as_ref().expect("the glyph's page was just found");
            // The texels the glyph shows, as a sprite of the page.
            let ([u0, v0], [u1, v1]) = glyph.uv;
            let size = [texture.width() as f32, texture.height() as f32];
            let region = Rect::new(
                u0 * size[0],
                v0 * size[1],
                (u1 - u0) * size[0],
                (v1 - v0) * size[1],
            );
            let sprite = Sprite::new(texture.clone(), region);
            // The middle of its bottom edge, along the sign from where it's placed, and up from
            // the sign's bottom.
            let along = (rect.x + rect.w / 2.0 - from_left) * scale;
            let up = (tall - rect.y - rect.h) * scale;
            let at = add3(add3(position, scale3(right, along)), [0.0, up, 0.0]);
            let quad = [rect.w * scale, rect.h * scale];
            self.push_billboard(&sprite, at, (right, facing), quad, tint);
        }
    }

    /// The ways a billboard at `position` has its right and its front point: the view's, turned
    /// about if a camera in perspective would see it from behind, as when it looks steeply down
    /// on one below it.
    fn billboard_axes_at(&self, position: [f32; 3]) -> ([f32; 3], [f32; 3]) {
        let segment = self.canvas.segments.last().expect("begin_3d adds one");
        let (right, facing) = segment.billboard_axes;
        let camera = &segment.view.camera;
        if let Projection::Perspective { .. } = camera.projection
            && dot3(sub3(camera.position, position), facing) < 0.0
        {
            return (scale3(right, -1.0), scale3(facing, -1.0));
        }
        (right, facing)
    }

    /// Records `sprite` as a billboard `size` wide and tall with the middle of its bottom edge at
    /// `position`, its right and front pointing as `axes` say.
    fn push_billboard(
        &mut self,
        sprite: &Sprite,
        position: [f32; 3],
        (right, facing): ([f32; 3], [f32; 3]),
        size: [f32; 2],
        tint: Color,
    ) {
        let transform = Affine3 {
            x_axis: scale3(right, size[0]),
            y_axis: [0.0, size[1], 0.0],
            z_axis: facing,
            translation: position,
        };
        // Whichever way the quad faces, its normal points up, as the ground's do.
        let colors = Colors {
            tint,
            emission: Color::BLACK,
            emission_sprite: None,
        };
        let segment = self.canvas.segments.last().expect("begin_3d adds one");
        let how = How::of(segment);
        self.push(
            None,
            transform,
            BILLBOARD_NORMAL,
            Some(sprite),
            colors,
            false,
            how,
        );
    }

    /// Draws the particles of `particles`, oldest first, as its settings'
    /// [`look`](crate::particles::ParticleSettings3d::look) has them, placed by `placed` if
    /// they're attached, sized by their scale and colored by their color at their age.
    ///
    /// Unless they draw opaque ([`is_opaque`](crate::particles::ParticleSettings3d::is_opaque)),
    /// they're drawn after the view's other meshes, blended as their settings say: hidden by what's
    /// in front of them, but hiding nothing, those blended normally the farthest from the camera
    /// first. They cast no shadows, and fade into the view's fog as everything does.
    pub fn draw_particles(&mut self, particles: &Particles3d, placed: Affine3) {
        let settings = particles.settings();
        let common = &settings.common;
        let blend = (!settings.is_opaque()).then_some(common.blend);
        let placed = if common.attached {
            placed
        } else {
            Affine3::IDENTITY
        };
        let segment = self.canvas.segments.last().expect("begin_3d adds one");
        let view = segment.view;
        let Basis { right, up, forward } = view.camera.basis();
        let toward_camera = scale3(forward, -1.0);
        let perspective = matches!(view.camera.projection, Projection::Perspective { .. });
        let sprite = particles.sprite();
        // A sprite as tall as its shape makes it at its width.
        let tall = sprite.map_or(1.0, |sprite| sprite.height() / sprite.width());
        let mesh = particles.mesh().filter(|_| settings.look == Look::Mesh);
        let look = match (settings.look, mesh) {
            (Look::Mesh, None) => Look::Billboard,
            (look, _) => look,
        };
        let glow = if settings.lit {
            settings.glow
        } else {
            Color::BLACK
        };
        for particle in particles.live() {
            let life = particle.age / particle.lifetime;
            let scale = settings.scale_at(life);
            let color = settings.color_at(life);
            if scale <= 0.0 || color.a <= 0.0 {
                continue;
            }
            let position = placed.apply(particle.position);
            let (transform, normal) = match look {
                Look::Billboard => {
                    // Facing the camera, turned by its angle about the way the camera looks.
                    let (sin, cos) = particle.angle.sin_cos();
                    let across = add3(scale3(right, cos), scale3(up, sin));
                    let upward = sub3(scale3(up, cos), scale3(right, sin));
                    let height = scale * tall;
                    let transform = Affine3 {
                        x_axis: scale3(across, scale),
                        y_axis: scale3(upward, height),
                        z_axis: toward_camera,
                        translation: sub3(position, scale3(upward, height / 2.0)),
                    };
                    (transform, BILLBOARD_NORMAL)
                }
                Look::Streak => {
                    // From where it is back the way it came, turned about that way to face the
                    // camera as squarely as it can.
                    let velocity = placed.apply_vector(particle.velocity);
                    let speed = dot3(velocity, velocity).sqrt();
                    let along = normalize3(velocity).unwrap_or(up);
                    let facing = if perspective {
                        normalize3(sub3(view.camera.position, position)).unwrap_or(toward_camera)
                    } else {
                        toward_camera
                    };
                    let across = normalize3(cross3(along, facing)).unwrap_or(right);
                    let length = scale * tall + settings.streak * speed;
                    let transform = Affine3 {
                        x_axis: scale3(across, scale),
                        y_axis: scale3(along, length),
                        z_axis: cross3(across, along),
                        translation: sub3(position, scale3(along, length)),
                    };
                    (transform, BILLBOARD_NORMAL)
                }
                Look::Mesh => {
                    let turned = placed * turned_about(particle.axis, particle.angle);
                    let transform = Affine3 {
                        x_axis: scale3(turned.x_axis, scale),
                        y_axis: scale3(turned.y_axis, scale),
                        z_axis: scale3(turned.z_axis, scale),
                        translation: position,
                    };
                    (transform, transform.normal_matrix())
                }
            };
            // Unlit, a particle shows its colors whatever lights it: as what it gives off, times
            // its sprite's colors, its tint keeping only how opaque it is.
            let colors = if settings.lit {
                Colors {
                    tint: color,
                    emission: Color::BLACK,
                    emission_sprite: None,
                }
            } else {
                Colors {
                    tint: Color::rgba(0.0, 0.0, 0.0, color.a),
                    emission: Color::rgb(color.r, color.g, color.b),
                    emission_sprite: sprite,
                }
            };
            let how = How {
                glow,
                casts_shadow: false,
                blend,
                depth: view.distance(position),
                in_sight: true,
            };
            self.push(mesh, transform, normal, sprite, colors, false, how);
        }
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
    /// moving its normals, colored by `colors`, showing from behind too if `double_sided`, and
    /// glowing, casting shadows and blending as `how` says.
    #[expect(clippy::too_many_arguments)]
    fn push(
        &mut self,
        mesh: Option<&Mesh>,
        transform: Affine3,
        normal: [[f32; 3]; 3],
        sprite: Option<&Sprite>,
        colors: Colors,
        double_sided: bool,
        how: How,
    ) {
        let determinant = transform.determinant();
        if !determinant.is_finite() {
            return;
        }
        let ([u0, v0], [u1, v1]) = sprite.map_or(([0.0, 0.0], [1.0, 1.0]), Sprite::uv_bounds);
        let canvas = &mut *self.canvas;
        let segment = canvas.segments.last_mut().expect("begin_3d adds a segment");
        // Out of sight, it draws only into the view's shadow maps, if it has any, and is kept
        // apart from what the view draws, which keeps its order.
        let (draws, instances) = if how.in_sight {
            segment.draws.end += 1;
            (&mut canvas.mesh_draws, &mut canvas.instances)
        } else {
            segment.shadow_draws.end += 1;
            (&mut canvas.shadow_draws, &mut canvas.shadow_instances)
        };
        let casts_shadow = how.casts_shadow && mesh.is_some();
        instances.push(Instance {
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
            glow: how.glow.to_rgba8(),
        });
        draws.push(MeshDraw {
            mesh: mesh.cloned(),
            texture: sprite.map(|sprite| sprite.texture().clone()),
            emission_texture: colors
                .emission_sprite
                .map(|sprite| sprite.texture().clone()),
            mirrored: determinant < 0.0,
            double_sided,
            casts_shadow,
            blend: how.blend,
            depth: how.depth,
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
    /// Its meshes out of sight that cast shadows, in the canvas's `shadow_draws`, which draw only
    /// into its shadow maps, if it has any.
    pub shadow_draws: Range<u32>,
    /// Its lights, in the canvas's `lights`.
    pub lights: Range<u32>,
    /// Whether the meshes drawn in it now cast shadows.
    pub casting: bool,
    /// How much of their own colors what's drawn in it now shows whatever lights it.
    pub glow: Color,
    /// How the meshes and models drawn in it now blend with what's behind them, `None` for
    /// opaque.
    pub blend: Option<Blend>,
    /// The ways its billboards' right and front point ([`Camera3d::billboard_axes`]), the same
    /// for all of them.
    pub billboard_axes: ([f32; 3], [f32; 3]),
    /// What it can show, which meshes wholly outside are left out of.
    pub sight: Sight,
    /// How many meshes it left out unrecorded, for being out of sight and casting no shadow.
    pub out_of_sight: u32,
}

impl Segment3d {
    /// How many meshes it left out for being out of sight: those left unrecorded, and those
    /// recorded only for their shadows.
    pub fn out_of_sight_count(&self) -> u32 {
        self.out_of_sight + self.shadow_draws.len() as u32
    }
}

/// What a mesh drawn is colored by besides the view's light: its tint, and what it gives off.
#[derive(Clone, Copy)]
struct Colors<'a> {
    tint: Color,
    emission: Color,
    emission_sprite: Option<&'a Sprite>,
}

/// How a mesh drawn shows besides its colors: how much of them it shows whatever lights it,
/// whether it casts shadows, how it blends, `None` for opaque, how far it is from the camera,
/// which blended draws are ordered by, and whether any of it may be in sight.
#[derive(Clone, Copy)]
struct How {
    glow: Color,
    casts_shadow: bool,
    blend: Option<Blend>,
    depth: f32,
    in_sight: bool,
}

impl How {
    /// As `segment` now has what's drawn in it: opaque, glowing and casting as it says, and in
    /// sight.
    fn of(segment: &Segment3d) -> Self {
        Self {
            glow: segment.glow,
            casts_shadow: segment.casting,
            blend: None,
            depth: 0.0,
            in_sight: true,
        }
    }
}

/// Turning by `angle` radians about `axis`, of length 1, counter-clockwise seen from its end.
fn turned_about(axis: [f32; 3], angle: f32) -> Affine3 {
    let (sin, cos) = angle.sin_cos();
    let [x, y, z] = axis;
    let rest = 1.0 - cos;
    // Rodrigues' rotation, by its columns.
    Affine3 {
        x_axis: [
            cos + x * x * rest,
            y * x * rest + z * sin,
            z * x * rest - y * sin,
        ],
        y_axis: [
            x * y * rest - z * sin,
            cos + y * y * rest,
            z * y * rest + x * sin,
        ],
        z_axis: [
            x * z * rest + y * sin,
            y * z * rest - x * sin,
            cos + z * z * rest,
        ],
        translation: [0.0; 3],
    }
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
    /// How it blends with what's behind it, `None` for opaque: particles blend, and meshes and
    /// models drawn after [`Canvas3d::set_blend`].
    pub blend: Option<Blend>,
    /// How far it is from the camera, as [`View3d::distance`] says, for a blended draw.
    pub depth: f32,
}

impl MeshDraw {
    /// What it's drawn as: draws of the same kind, the same mesh showing the same textures drawn
    /// the same way round and blended the same way, are drawn in one draw call, as instances of
    /// one mesh.
    fn kind(&self) -> DrawKind {
        DrawKind {
            mesh: self.mesh.as_ref().map_or(0, Mesh::id),
            texture: self.texture.as_ref().map_or(0, Texture::id),
            emission_texture: self.emission_texture.as_ref().map_or(0, Texture::id),
            mirrored: self.mirrored,
            double_sided: self.double_sided,
            casts_shadow: self.casts_shadow,
            blend: self.blend,
        }
    }

    /// What it looks like, its mesh aside: draws that look alike draw one after another, with
    /// nothing bound between them but their meshes.
    pub fn look(&self) -> DrawLook {
        // A billboard's quad is in the first arena.
        let arena = self.mesh.as_ref().map_or(0, Mesh::arena);
        self.kind().look(arena)
    }
}

/// The kind of a [`MeshDraw`]: its mesh's and textures' IDs, 0 for none, which way round it's
/// drawn, whether it casts shadows, and how it blends.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct DrawKind {
    mesh: usize,
    texture: usize,
    emission_texture: usize,
    mirrored: bool,
    double_sided: bool,
    casts_shadow: bool,
    blend: Option<Blend>,
}

impl DrawKind {
    /// What a draw of this kind looks like, its mesh in `arena`.
    fn look(self, arena: u32) -> DrawLook {
        DrawLook {
            billboard: self.mesh == 0,
            arena,
            texture: self.texture,
            emission_texture: self.emission_texture,
            mirrored: self.mirrored,
            double_sided: self.double_sided,
            casts_shadow: self.casts_shadow,
            blend: self.blend,
        }
    }
}

/// What a [`MeshDraw`] looks like, its mesh aside: its [`DrawKind`] but for its mesh, and
/// whether it's a billboard, which shadows leave out. Draws that look alike, one after another,
/// are drawn in one call. Not a particle's [`Look`], which is what its particles are drawn as.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct DrawLook {
    billboard: bool,
    arena: u32,
    texture: usize,
    emission_texture: usize,
    mirrored: bool,
    double_sided: bool,
    casts_shadow: bool,
    blend: Option<Blend>,
}

/// The order a 3D view's meshes draw in: draws of a kind together, so each kind is one run of
/// instances, and the kinds that look alike together ([`MeshDraw::look`]), so nothing is bound
/// between them but their meshes. The looks go in the order each was first drawn, each look's
/// kinds in the order each was first drawn, and each kind's draws in the order they were drawn,
/// so the order is the same every frame. Opaque draws have a depth buffer to sort them, so their
/// order shows only in which of two surfaces at the same depth is in front. Blended ones,
/// particles and meshes drawn blended, come after every opaque one: those that add, multiply or
/// screen by kind, since their order doesn't change what they make, and then those blended
/// normally, the farthest from the camera first, each run of a kind among them a run of its own.
/// Kept from frame to frame for what it allocates.
#[derive(Default)]
pub(crate) struct MeshOrder {
    /// Whether the draws draw in the order they were drawn, each kind's together already.
    pub in_order: bool,
    /// Unless they're in order, indices into the view's draws, in the order they draw.
    order: Vec<u32>,
    /// Where each run of the order that's one kind ends.
    pub ends: Vec<u32>,
    kinds: HashMap<DrawKind, u32>,
    kind_of: Vec<u32>,
    /// The looks of the kinds being laid out, numbered as each first comes.
    looks: HashMap<DrawLook, u32>,
    /// Each of those kinds' look.
    look_of: Vec<u32>,
    /// Each of those kinds' place among their runs, when that isn't where it first came.
    slot_of: Vec<u32>,
    /// How many draws each kind has, or where each look's kinds begin, as they're laid out.
    counts: Vec<u32>,
}

impl MeshOrder {
    /// Works out the order `draws` draw in.
    pub fn arrange(&mut self, draws: &[MeshDraw]) {
        if draws.iter().any(|draw| draw.blend.is_some()) {
            self.arrange_blended(draws);
            return;
        }
        self.kinds.clear();
        self.kind_of.clear();
        self.looks.clear();
        self.look_of.clear();
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
                        self.note_look(draw);
                    } else if self.in_order {
                        self.in_order = false;
                        self.count_kinds_so_far();
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
        self.place_kinds();
        if self.in_order && self.slot_of.is_empty() {
            let mut end = 0;
            for count in &mut self.ends {
                end += *count;
                *count = end;
            }
            return;
        }
        if self.in_order {
            self.in_order = false;
            self.count_kinds_so_far();
        }
        self.lay_out(0, 0..draws.len() as u32);
    }

    /// Notes that the draws so far came a kind at a time, the kinds one after another, as many
    /// of each as counted in `ends`, in `kind_of`.
    fn count_kinds_so_far(&mut self) {
        for (kind, &count) in self.ends.iter().enumerate() {
            let kinds = std::iter::repeat_n(kind as u32, count as usize);
            self.kind_of.extend(kinds);
        }
    }

    /// Numbers the look of `draw`, of a kind first come, as looks first come. Worked out a kind at
    /// a time, not a draw, since it reads the draw's mesh.
    fn note_look(&mut self, draw: &MeshDraw) {
        let next = self.looks.len() as u32;
        let look = *self.looks.entry(draw.look()).or_insert(next);
        self.look_of.push(look);
    }

    /// Works out each kind's place among the kinds' runs, the kinds of a look together, in
    /// `slot_of`; or leaves it empty when the kinds of each look come together already.
    fn place_kinds(&mut self) {
        self.slot_of.clear();
        if self.look_of.is_sorted() {
            return;
        }
        // Where each look's kinds begin, which becomes where the next goes.
        self.counts.clear();
        self.counts.resize(self.looks.len(), 0);
        for &look in &self.look_of {
            self.counts[look as usize] += 1;
        }
        let mut start = 0;
        for count in &mut self.counts {
            (*count, start) = (start, start + *count);
        }
        for &look in &self.look_of {
            let at = &mut self.counts[look as usize];
            self.slot_of.push(*at);
            *at += 1;
        }
    }

    /// Lays the draws `indices` out in `order`, after those there, each kind's together and the
    /// kinds where `slot_of` places them, `kind_of` saying each draw's kind and `ends[first..]`
    /// how many draws each kind has, which become where each run ends.
    fn lay_out(&mut self, first: usize, indices: impl Iterator<Item = u32>) {
        if !self.slot_of.is_empty() {
            self.counts.clear();
            self.counts.extend_from_slice(&self.ends[first..]);
            for (kind, &count) in self.counts.iter().enumerate() {
                self.ends[first + self.slot_of[kind] as usize] = count;
            }
        }
        // Where each run starts, which becomes where it ends as its draws are put in order.
        let mut start = self.order.len() as u32;
        for count in &mut self.ends[first..] {
            (*count, start) = (start, start + *count);
        }
        self.order.resize(start as usize, 0);
        for (i, &kind) in indices.zip(&self.kind_of) {
            let slot = match self.slot_of.get(kind as usize) {
                Some(&slot) => slot,
                None => kind,
            };
            let at = &mut self.ends[first + slot as usize];
            self.order[*at as usize] = i;
            *at += 1;
        }
    }

    /// Works out the order `draws` draw in, some of them blended: the opaque ones by kind, then
    /// those whose order doesn't matter by kind, then those blended normally far to near.
    fn arrange_blended(&mut self, draws: &[MeshDraw]) {
        self.kinds.clear();
        self.kind_of.clear();
        self.ends.clear();
        self.order.clear();
        self.in_order = false;
        let indices = 0..draws.len() as u32;
        let opaque = indices
            .clone()
            .filter(|&i| draws[i as usize].blend.is_none());
        self.group(draws, opaque);
        let unordered = indices.clone().filter(|&i| {
            matches!(
                draws[i as usize].blend,
                Some(Blend::Add | Blend::Multiply | Blend::Screen)
            )
        });
        self.group(draws, unordered);
        let mut ordered: Vec<u32> = indices
            .filter(|&i| draws[i as usize].blend == Some(Blend::Normal))
            .collect();
        // Far to near; drawn first first among those as far, for the same order every frame.
        ordered.sort_by(|&a, &b| {
            let [a_depth, b_depth] = [a, b].map(|i| draws[i as usize].depth);
            b_depth.total_cmp(&a_depth).then(a.cmp(&b))
        });
        let mut last = None;
        for i in ordered {
            let kind = draws[i as usize].kind();
            if last.is_some_and(|last| last != kind) {
                self.ends.push(self.order.len() as u32);
            }
            last = Some(kind);
            self.order.push(i);
        }
        if last.is_some() {
            self.ends.push(self.order.len() as u32);
        }
    }

    /// Adds the draws `indices` of `draws` to the order, those of a kind together, each kind's
    /// in the order they're given, and the kinds that look alike together, the looks and each
    /// look's kinds in the order each first comes.
    fn group(&mut self, draws: &[MeshDraw], indices: impl Iterator<Item = u32> + Clone) {
        // Each draw's kind, numbered as kinds first come, and how many draws each kind has, in
        // `ends` for now, after the runs already there: a view may hold a thousand kinds, so
        // each is looked up, not searched for among those so far.
        self.kinds.clear();
        self.kind_of.clear();
        self.looks.clear();
        self.look_of.clear();
        let first = self.ends.len();
        for i in indices.clone() {
            let next = (self.ends.len() - first) as u32;
            let draw = &draws[i as usize];
            let kind = *self.kinds.entry(draw.kind()).or_insert(next);
            if kind == next {
                self.ends.push(0);
                self.note_look(draw);
            }
            self.ends[first + kind as usize] += 1;
            self.kind_of.push(kind);
        }
        self.place_kinds();
        self.lay_out(first, indices);
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

    /// How many kinds of draw `draws` are, and so how many draw calls they take; or with draws
    /// blended normally among them, how many runs of a kind they draw in.
    pub fn kind_count(draws: &[MeshDraw]) -> usize {
        if draws.iter().any(|draw| draw.blend == Some(Blend::Normal)) {
            let mut order = Self::default();
            order.arrange(draws);
            return order.ends.len();
        }
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
    fn a_light_box_includes_its_faces_after_world_roundoff() {
        // Qwine's row homes put their walls exactly on their ceiling light's box.
        let room = Bounds3 {
            min: [0.7, -0.25, 2.4],
            max: [5.3, 3.2, 13.6],
        };
        for placed in [
            Affine3::IDENTITY,
            Affine3::from_translation([-16.0, 0.0, -80.0]),
            Affine3::from_translation([128.0, -18.0, -256.0])
                * Affine3::from_rotation_y(0.71)
                * Affine3::from_scale([-1.5, 0.8, 2.0]),
        ] {
            let lamp = Light::point(placed.apply(room.center()), Color::WHITE, 100.0)
                .within(room, placed)
                .unwrap();
            for axis in 0..3 {
                for face in [room.min[axis], room.max[axis]] {
                    for a in 0..=10 {
                        for b in 0..=10 {
                            let mut point = room.center();
                            point[axis] = face;
                            for (other, step) in [((axis + 1) % 3, a), ((axis + 2) % 3, b)] {
                                point[other] = room.min[other]
                                    + (room.max[other] - room.min[other]) * step as f32 / 10.0;
                            }
                            assert!(
                                lamp.bounds_hold(placed.apply(point)),
                                "face {point:?}, {placed:?}"
                            );
                            assert!(lamp.level_at(placed.apply(point)) > 0.0);
                        }
                    }
                    // Roundoff allowance must still reject a point a centimetre outside.
                    let mut outside = room.center();
                    outside[axis] = face + if face == room.min[axis] { -0.01 } else { 0.01 };
                    assert!(
                        !lamp.bounds_hold(placed.apply(outside)),
                        "outside {outside:?}"
                    );
                    assert_eq!(lamp.level_at(placed.apply(outside)), 0.0);
                }
            }
        }
    }

    #[test]
    fn a_light_keeps_to_its_box_wherever_its_parent_places_it() {
        // A room 4 wide, deep and high about the origin, its floor at 0, and a light in it a
        // metre under its ceiling, reaching past its floor and its ceiling.
        let room = Bounds3 {
            min: [-2.0, 0.0, -2.0],
            max: [2.0, 4.0, 2.0],
        };
        let lamp = Light::point([0.0, 3.0, 0.0], Color::WHITE, 6.0);
        let kept = lamp.within(room, Affine3::IDENTITY).unwrap();
        assert_eq!(kept.problem(), None);
        assert_eq!(
            kept.level_at([0.0, 0.0, 0.0]),
            lamp.level_at([0.0, 0.0, 0.0])
        );
        assert!(kept.level_at([0.0, 0.0, 0.0]) > 0.0, "its floor");
        assert!(
            kept.level_at([2.0, 1.0, 2.0]) > 0.0,
            "a corner, on its faces"
        );
        assert!(
            lamp.level_at([0.0, -1.0, 0.0]) > 0.0,
            "unkept, under its floor"
        );
        assert_eq!(
            kept.level_at([0.0, -1.0, 0.0]),
            0.0,
            "kept, nothing under its floor"
        );
        assert_eq!(kept.level_at([0.0, 5.0, 0.0]), 0.0, "nor over its ceiling");
        assert_eq!(kept.level_at([2.5, 1.0, 0.0]), 0.0, "nor past a wall");

        // Its box goes where its parent places it, turned and stretched: twice as wide along
        // its own `x`, a quarter turn left, and 10 along the world's `x`. So it's 4 either side
        // of 0 along `z`, and 2 either side of 10 along `x`.
        let placed = Affine3::from_translation([10.0, 0.0, 0.0])
            * Affine3::from_rotation_y(std::f32::consts::FRAC_PI_2)
            * Affine3::from_scale([2.0, 1.0, 1.0]);
        let turned = Light {
            position: placed.apply([0.0, 3.0, 0.0]),
            ..lamp
        }
        .within(room, placed)
        .unwrap();
        assert!(
            turned.bounds_hold([10.0, 1.0, 3.9]),
            "the stretched way, turned"
        );
        assert!(!turned.bounds_hold([10.0, 1.0, 4.1]));
        assert!(turned.bounds_hold([11.9, 1.0, 0.0]), "the other way");
        assert!(!turned.bounds_hold([12.1, 1.0, 0.0]));
        assert!(!turned.bounds_hold([10.0, -0.1, 0.0]));

        // A box placed with no room in it is none, and one that's broken is a problem.
        assert_eq!(
            lamp.within(room, Affine3::from_scale([1.0, 0.0, 1.0])),
            None
        );
        let broken = Light {
            bounds: Some(Affine3::from_scale([0.0; 3])),
            ..lamp
        };
        assert!(broken.problem().unwrap().contains("bounds"));
    }

    #[test]
    fn a_light_whose_box_is_out_of_view_lights_nothing_of_it() {
        // Looking along `-z` from the origin, at a light 2 behind the camera, reaching 6, so
        // into view: kept to a box behind the camera, it's out of view; kept to a box reaching
        // ahead of the camera, it isn't.
        let camera = Camera3d::perspective([0.0; 3], [0.0, 0.0, -1.0], 0.8);
        let view = View3d::new(camera, Rect::new(0.0, 0.0, 160.0, 90.0));
        let lamp = Light::point([0.0, 0.0, 2.0], Color::WHITE, 6.0);
        let behind = Bounds3 {
            min: [-2.0, -2.0, 0.5],
            max: [2.0, 2.0, 4.0],
        };
        let reaching = Bounds3 {
            min: [-2.0, -2.0, -2.0],
            max: [2.0, 2.0, 4.0],
        };
        let lights = [
            lamp,
            lamp.within(behind, Affine3::IDENTITY).unwrap(),
            lamp.within(reaching, Affine3::IDENTITY).unwrap(),
        ];
        let mut chosen = Vec::new();
        assert_eq!(lights_of(&view, &lights, &mut chosen), 0);
        assert_eq!(chosen, [0, 2]);
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
    fn a_sky_goes_from_its_horizon_to_its_zenith_and_its_ground_by_the_height_of_a_way() {
        let [zenith, horizon, ground] = [
            Color::rgb8(0x10, 0x20, 0x40),
            Color::rgb8(0xc0, 0xa0, 0x80),
            Color::rgb8(0x00, 0x00, 0x00),
        ];
        let sky = Sky {
            zenith,
            horizon,
            ground,
            band: std::f32::consts::FRAC_PI_6,
        };
        assert_eq!(sky.problem(), None);
        let bytes = |way: [f32; 3]| sky.color_at(way).to_rgba8();
        // Level, it's the horizon's, and of no length, a way is level.
        assert_eq!(bytes([0.0, 0.0, -1.0]), horizon.to_rgba8());
        assert_eq!(bytes([3.0, 0.0, 4.0]), horizon.to_rgba8());
        assert_eq!(bytes([0.0; 3]), horizon.to_rgba8());
        // From the band up, the zenith's, and from the band down, the ground's: a sixth of a
        // half turn is a height of a half.
        for way in [[0.0, 1.0, 0.0], [0.0, 1.0, -1.0], [0.0, 1.0, -1.7]] {
            assert_eq!(bytes(way), zenith.to_rgba8(), "{way:?}");
        }
        assert_eq!(bytes([1.0, -1.0, 0.0]), ground.to_rgba8());
        // Half way up the band, half way in linear light, which is nearer the brighter in sRGB.
        let half = sky.color_at([0.0, 0.25, -(1.0f32 - 0.0625).sqrt()]);
        let [from, to] = [horizon, zenith].map(Color::to_linear);
        for (i, shown) in half.to_linear().into_iter().take(3).enumerate() {
            let wanted = (from[i] + to[i]) / 2.0;
            assert!((shown - wanted).abs() < 1e-4, "{half:?}");
        }
        assert!(half.r > (horizon.r + zenith.r) / 2.0, "{half:?}");
        // It's opaque, whatever its colors say.
        let clear = Sky::flat(Color::rgba(0.2, 0.4, 0.6, 0.0));
        assert_eq!(clear.color_at([0.0, 1.0, 0.0]).a, 1.0);
        // A flat sky is one color every way, and changes all the way up.
        assert_eq!(clear.band, std::f32::consts::FRAC_PI_2);
        assert_eq!(
            Sky::flat(horizon).color_at([0.2, -0.7, 0.1]).to_rgba8(),
            horizon.to_rgba8()
        );

        // A band past a quarter turn is a quarter turn; one not above 0 can't be, and a view's
        // sky that can't be is none.
        let wide = Sky { band: 3.0, ..sky };
        assert_eq!(wide.problem(), None);
        let flat_band = Sky {
            band: Sky::WHOLE_BAND,
            ..sky
        };
        assert_eq!(
            wide.color_at([0.0, 0.5, -1.0]),
            flat_band.color_at([0.0, 0.5, -1.0])
        );
        for band in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let wrong = Sky { band, ..sky };
            let problem = wrong.problem().expect("it can't be");
            assert!(problem.contains("band"), "{problem}");
            let camera = Camera3d::perspective([0.0; 3], [0.0, 0.0, -1.0], 0.8);
            let view = View3d {
                sky: Some(wrong),
                ..View3d::new(camera, Rect::new(0.0, 0.0, 16.0, 9.0))
            };
            assert_eq!(view.sky(), None);
        }
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

    /// A view looking at `target`, whose sun, shining down and across, casts `shadows`.
    fn sunlit(target: [f32; 3], shadows: SunShadows) -> View3d {
        let camera = Camera3d::perspective(add3(target, [0.0, 6.0, 8.0]), target, 0.8);
        View3d {
            lighting: Lighting {
                direction: [-0.4, -1.0, -0.6],
                shadows: Some(shadows),
                ..Lighting::default()
            },
            ..View3d::new(camera, Rect::new(0.0, 0.0, 64.0, 48.0))
        }
    }

    /// Where `point` is on a sun's shadow map: across and up it from -1 to 1, then how far
    /// along the sunlight, from 0 to 1.
    fn on_map(map: &SunMap, [x, y, z]: [f32; 3]) -> [f32; 3] {
        let m = &map.onto_map;
        [0, 1, 2].map(|i| m[0][i] * x + m[1][i] * y + m[2][i] * z + m[3][i])
    }

    #[test]
    fn a_suns_shadow_map_holds_what_its_shadows_reach() {
        let target = [30.5, 2.0, -17.25];
        let shadows = SunShadows::new(10.0);
        let view = sunlit(target, shadows);
        let map = view.sun_map().expect("its sun casts shadows");
        assert_eq!(
            [map.reach, map.darkness, map.texel],
            [10.0, 1.0, shadows.texel()]
        );
        assert_eq!(map.center, target);
        // A texel is the reach's part of the map, but for a few at each edge.
        assert_near([shadows.texel()], [20.0 / 2042.0]);
        let texel_on_map = 2.0 / SunShadows::RESOLUTION as f32;
        // What the camera looks at is within half a texel of the map's middle, halfway along.
        let [x, y, depth] = on_map(&map, target);
        assert!(x.abs() <= texel_on_map / 2.0 + 1e-5, "{x}");
        assert!(y.abs() <= texel_on_map / 2.0 + 1e-5, "{y}");
        assert_near([depth], [0.5]);
        // Everything within the reach is in it, with room to be looked up off a surface.
        let sun = normalize3(view.lighting.direction).unwrap();
        let ways = [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            sun,
            normalize3([1.0, 1.0, 1.0]).unwrap(),
        ];
        for way in ways {
            for side in [-10.0, 10.0] {
                let [x, y, depth] = on_map(&map, add3(target, scale3(way, side)));
                let inside = 1.0 - 2.0 * texel_on_map;
                assert!(x.abs() < inside && y.abs() < inside, "{way:?}: {x}, {y}");
                assert!(depth > 0.001 && depth < 0.999, "{way:?}: {depth}");
            }
        }
        // It looks along the sunlight: what's down it is farther along, and nowhere else.
        let [x0, y0, near] = on_map(&map, target);
        let [x1, y1, far] = on_map(&map, add3(target, scale3(sun, 5.0)));
        assert_near([x1, y1], [x0, y0]);
        assert!(far > near, "{far} is not past {near}");
        // A world unit across the sunlight is as many texels wherever it is and whichever way.
        let across = normalize3(cross3(sun, [0.0, 1.0, 0.0])).unwrap();
        let [x2, y2, _] = on_map(&map, add3(target, across));
        let moved = ((x2 - x0).powi(2) + (y2 - y0).powi(2)).sqrt();
        assert_near([moved / texel_on_map * shadows.texel()], [1.0]);
    }

    #[test]
    fn a_suns_shadow_map_moves_by_whole_texels() {
        let shadows = SunShadows::new(12.0);
        let size = SunShadows::RESOLUTION as f32;
        // Where a place in the world is on the map, in texels.
        let place = [3.3, 0.7, -1.9];
        let texels = |target: [f32; 3]| {
            let map = sunlit(target, shadows).sun_map().unwrap();
            let [x, y, _] = on_map(&map, place);
            [x * size / 2.0, y * size / 2.0]
        };
        let from = texels([0.0; 3]);
        let mut moved = 0;
        for step in 1..200 {
            let along = step as f32 * 0.0137;
            let to = texels([along, along * 0.3, -along * 0.7]);
            for axis in 0..2 {
                let by = to[axis] - from[axis];
                assert!((by - by.round()).abs() < 0.01, "{by} texels at {along}");
                moved += u32::from(by.round() != 0.0);
            }
        }
        assert!(
            moved > 100,
            "the map moved with the camera only {moved} times"
        );
    }

    #[test]
    fn a_sun_casts_no_shadows_that_cant_be() {
        let target = [1.0, 2.0, 3.0];
        let shadows = SunShadows::new(10.0);
        assert_eq!(shadows.problem(), None);
        assert!(sunlit(target, shadows).sun_map().is_some());
        // A view's sun casts none unless it says so.
        let camera = Camera3d::perspective([0.0, 1.0, 5.0], [0.0; 3], 0.8);
        let plain = View3d::new(camera, Rect::new(0.0, 0.0, 64.0, 48.0));
        assert_eq!(plain.lighting.shadows, None);
        assert_eq!(plain.sun_map(), None);
        assert_eq!(Lighting::UNLIT.shadows, None);

        for reach in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let shadows = SunShadows::new(reach);
            assert!(shadows.problem().unwrap().contains("reach"), "{reach}");
            assert_eq!(sunlit(target, shadows).sun_map(), None, "{reach}");
        }
        for darkness in [-0.1, 1.1, f32::NAN] {
            let shadows = SunShadows {
                darkness,
                ..shadows
            };
            assert!(
                shadows.problem().unwrap().contains("darkness"),
                "{darkness}"
            );
            assert_eq!(sunlit(target, shadows).sun_map(), None, "{darkness}");
        }
        // Shadows that take none of the sun's light are none, and so are a sun's that gives
        // none, or shines no way.
        let faint = SunShadows {
            darkness: 0.0,
            ..shadows
        };
        assert_eq!(faint.problem(), None);
        assert_eq!(sunlit(target, faint).sun_map(), None);
        let mut view = sunlit(target, shadows);
        view.lighting.color = Color::BLACK;
        assert_eq!(view.sun_map(), None);
        let mut view = sunlit(target, shadows);
        view.lighting.direction = [0.0; 3];
        assert_eq!(view.sun_map(), None);
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
            blend: None,
            depth: 0.0,
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

    #[test]
    fn kinds_that_look_alike_draw_together_in_the_order_each_look_was_first_drawn() {
        use crate::{graphics::MeshData, headless::Headless};
        let mut headless = Headless::new().expect("a GPU adapter is available");
        let ctx = headless.context();
        let meshes = [0; 3].map(|_| ctx.create_mesh(&MeshData::cube(1.0)).unwrap());
        let textures = [0; 2].map(|_| ctx.create_texture(1, 1, &[255; 4]).unwrap());
        let draw = |(mesh, texture): (usize, usize)| MeshDraw {
            mesh: Some(meshes[mesh].clone()),
            texture: Some(textures[texture].clone()),
            emission_texture: None,
            mirrored: false,
            double_sided: false,
            casts_shadow: false,
            blend: None,
            depth: 0.0,
        };
        // Three meshes, the first and the third showing one texture, the second another.
        let draws = [(0, 0), (1, 1), (2, 0), (0, 0), (1, 1)].map(draw);
        let mut order = MeshOrder::default();
        order.arrange(&draws);
        assert!(!order.in_order);
        let drawn = |order: &MeshOrder, n| (0..n).map(|i| order.draw_at(i)).collect::<Vec<_>>();
        assert_eq!(drawn(&order, 5), [0, 3, 2, 1, 4]);
        assert_eq!(order.ends, [2, 3, 5]);
        assert_eq!(MeshOrder::kind_count(&draws), 3);

        // A kind at a time, but the first look's kinds apart.
        let draws = [(0, 0), (1, 1), (2, 0)].map(draw);
        order.arrange(&draws);
        assert!(!order.in_order);
        assert_eq!(drawn(&order, 3), [0, 2, 1]);
        assert_eq!(order.ends, [1, 2, 3]);
        // A look at a time already.
        let draws = [(0, 0), (2, 0), (1, 1)].map(draw);
        order.arrange(&draws);
        assert!(order.in_order);
        assert_eq!(order.ends, [1, 2, 3]);

        // Blended among them, the opaque ones are laid out the same, and so are those added.
        let mut draws = Vec::from([(0, 0), (1, 1), (2, 0), (0, 0), (1, 1)].map(draw));
        let added = [(0, 0), (1, 1), (2, 0)].map(|kind| MeshDraw {
            blend: Some(Blend::Add),
            ..draw(kind)
        });
        draws.extend(added);
        order.arrange(&draws);
        assert_eq!(drawn(&order, 8), [0, 3, 2, 1, 4, 5, 7, 6]);
        assert_eq!(order.ends, [2, 3, 5, 6, 7, 8]);
        let mut draws = Vec::from([(1, 1), (0, 0), (2, 0)].map(draw));
        draws.extend([(1, 1), (0, 0), (2, 0)].map(|kind| MeshDraw {
            blend: Some(Blend::Add),
            ..draw(kind)
        }));
        order.arrange(&draws);
        assert_eq!(drawn(&order, 6), [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn blended_draws_come_after_opaque_ones_and_those_blended_normally_far_to_near() {
        let draw = |blend, depth, mirrored| MeshDraw {
            mesh: None,
            texture: None,
            emission_texture: None,
            mirrored,
            double_sided: false,
            casts_shadow: false,
            blend,
            depth,
        };
        let normal = Some(Blend::Normal);
        let added = Some(Blend::Add);
        let draws = [
            draw(normal, 2.0, false),
            draw(None, 0.0, false),
            draw(added, 9.0, false),
            draw(normal, 5.0, true),
            draw(None, 0.0, true),
            draw(normal, 8.0, false),
            draw(added, 1.0, false),
            draw(None, 0.0, false),
        ];
        let mut order = MeshOrder::default();
        order.arrange(&draws);
        let drawn: Vec<u32> = (0..8).map(|i| order.draw_at(i)).collect();
        // Opaque by kind, then added by kind, then normal far to near: 8, then 5 of another
        // kind, then 2 of the first again.
        assert_eq!(drawn, [1, 7, 4, 2, 6, 5, 3, 0]);
        assert_eq!(order.ends, [2, 3, 5, 6, 7, 8]);
        assert_eq!(MeshOrder::kind_count(&draws), 6);
    }

    #[test]
    fn a_blended_draw_leaves_the_opaque_ones_in_the_order_they_have_without_it() {
        // Billboards of eight kinds, by which way round they're drawn and whether they cast
        // shadows, no two of a kind one after the other.
        let draw = |kind: u32, blend, depth| MeshDraw {
            mesh: None,
            texture: None,
            emission_texture: None,
            mirrored: kind & 1 != 0,
            double_sided: kind & 2 != 0,
            casts_shadow: kind & 4 != 0,
            blend,
            depth,
        };
        let mut draws: Vec<MeshDraw> = (0..200).map(|i| draw(i * 5 % 8, None, 0.0)).collect();
        let mut opaque = MeshOrder::default();
        opaque.arrange(&draws);
        assert!(!opaque.in_order);
        assert_eq!(opaque.ends.len(), 8);

        // One particle, drawn in the middle of them.
        draws.insert(100, draw(0, Some(Blend::Normal), 3.0));
        let mut order = MeshOrder::default();
        order.arrange(&draws);
        // Which draw each opaque one is, now that the particle is among them.
        let moved = |i: u32| if i >= 100 { i + 1 } else { i };
        let expected: Vec<u32> = opaque.order().iter().map(|&i| moved(i)).collect();
        assert_eq!(order.order()[..200], expected);
        assert_eq!(order.order()[200], 100);
        assert_eq!(order.ends[..8], opaque.ends);
        assert_eq!(order.ends[8], 201);
        assert_eq!(MeshOrder::kind_count(&draws), 9);

        // Kept from frame to frame, it starts again each time.
        order.arrange(&draws);
        assert_eq!(order.order()[..200], expected);
        assert_eq!(order.ends.len(), 9);
    }

    /// A box with its middle at `at`, reaching `half` from it each way.
    fn cube(at: [f32; 3], half: f32) -> Bounds3 {
        Bounds3 {
            min: at.map(|v| v - half),
            max: at.map(|v| v + half),
        }
    }

    #[test]
    fn a_view_sees_a_box_unless_its_wholly_outside_one_of_its_sides() {
        // Looking along `-z` from the origin, twice as wide as it's tall: 10 ahead, it takes in
        // 4.23 up and down, and 8.46 across.
        let camera = Camera3d {
            far: 100.0,
            ..Camera3d::perspective([0.0; 3], [0.0, 0.0, -1.0], 0.8)
        };
        let wide = Sight::of(&View3d::new(camera, Rect::new(0.0, 0.0, 200.0, 100.0)));
        assert_eq!(wide.sees(&cube([0.0, 0.0, -10.0], 1.0)), Seen::Wholly);
        assert_eq!(wide.sees(&cube([6.0, 0.0, -10.0], 1.0)), Seen::Wholly);
        for (at, side) in [
            ([0.0, 0.0, 10.0], "behind"),
            ([12.0, 0.0, -10.0], "right"),
            ([-12.0, 0.0, -10.0], "left"),
            ([0.0, 6.0, -10.0], "above"),
            ([0.0, -6.0, -10.0], "below"),
            ([0.0, 0.0, -102.0], "past far"),
        ] {
            assert_eq!(wide.sees(&cube(at, 1.0)), Seen::Not, "{side}");
        }
        assert_eq!(
            wide.sees(&cube([0.0, 0.0, -0.05], 0.01)),
            Seen::Not,
            "nearer than near"
        );
        assert_eq!(
            wide.sees(&cube([8.5, 0.0, -10.0], 1.0)),
            Seen::Partly,
            "across the right"
        );
        assert_eq!(
            wide.sees(&cube([0.0; 3], 1.0)),
            Seen::Partly,
            "round the camera"
        );
        assert_eq!(
            wide.sees(&cube([0.0, 0.0, -100.0], 1.0)),
            Seen::Partly,
            "across far"
        );

        // Half as wide as it's tall, it takes in 2.11 across, 10 ahead.
        let tall = Sight::of(&View3d::new(camera, Rect::new(0.0, 0.0, 50.0, 100.0)));
        assert_eq!(tall.sees(&cube([6.0, 0.0, -10.0], 1.0)), Seen::Not);
        assert_eq!(tall.sees(&cube([0.0, 6.0, -10.0], 1.0)), Seen::Not);

        // Without perspective, 4 tall and 8 across however far ahead.
        let flat = Camera3d {
            far: 100.0,
            ..Camera3d::orthographic([0.0; 3], [0.0, 0.0, -1.0], 4.0)
        };
        let ortho = Sight::of(&View3d::new(flat, Rect::new(0.0, 0.0, 200.0, 100.0)));
        assert_eq!(ortho.sees(&cube([3.0, 1.0, -50.0], 0.5)), Seen::Wholly);
        assert_eq!(ortho.sees(&cube([5.0, 0.0, -50.0], 0.5)), Seen::Not);
        assert_eq!(ortho.sees(&cube([0.0, 2.6, -1.0], 0.5)), Seen::Not);
        assert_eq!(ortho.sees(&cube([0.0, 0.0, 5.0], 1.0)), Seen::Not, "behind");

        // Nothing can be said of a box that isn't numbers, or in a view of no size.
        let lost = Bounds3 {
            min: [f32::NAN; 3],
            max: [0.0; 3],
        };
        assert_eq!(wide.sees(&lost), Seen::Partly);
        let empty = Sight::of(&View3d::new(camera, Rect::new(0.0, 0.0, 0.0, 0.0)));
        assert_eq!(empty.sees(&cube([12.0, 0.0, -10.0], 1.0)), Seen::Partly);
    }

    #[test]
    fn a_view_with_a_sky_sees_nothing_past_where_its_fog_is_whole() {
        let rect = Rect::new(0.0, 0.0, 200.0, 100.0);
        let camera = Camera3d::perspective([0.0; 3], [0.0, 0.0, -1.0], 0.8);
        let fog = Some(Fog::new(Color::WHITE, 10.0, 40.0));
        let sky = Some(Sky::flat(Color::WHITE));
        let hazy = Sight::of(&View3d {
            fog,
            sky,
            ..View3d::new(camera, rect)
        });
        assert_eq!(hazy.sees(&cube([0.0, 0.0, -20.0], 1.0)), Seen::Wholly);
        assert_eq!(hazy.sees(&cube([0.0, 0.0, -40.0], 1.0)), Seen::Partly);
        assert_eq!(hazy.sees(&cube([0.0, 0.0, -42.0], 1.0)), Seen::Not);
        // From where the camera is, not ahead of it: 41.5 off, though 35.5 ahead.
        assert_eq!(hazy.sees(&cube([22.0, 0.0, -36.0], 0.5)), Seen::Not);
        assert_eq!(hazy.sees(&cube([15.0, 0.0, -38.0], 1.0)), Seen::Partly);

        // Without a sky, the fog shows over what's behind the view, so what's past it stays.
        let foggy = Sight::of(&View3d {
            fog,
            ..View3d::new(camera, rect)
        });
        assert_eq!(foggy.sees(&cube([0.0, 0.0, -42.0], 1.0)), Seen::Wholly);
        let clear = Sight::of(&View3d {
            sky,
            ..View3d::new(camera, rect)
        });
        assert_eq!(clear.sees(&cube([0.0, 0.0, -42.0], 1.0)), Seen::Wholly);

        // Without perspective, by how far ahead of the camera.
        let flat = Camera3d::orthographic([0.0; 3], [0.0, 0.0, -1.0], 20.0);
        let ortho = Sight::of(&View3d {
            fog,
            sky,
            ..View3d::new(flat, rect)
        });
        assert_eq!(ortho.sees(&cube([15.0, 0.0, -38.0], 1.0)), Seen::Wholly);
        assert_eq!(ortho.sees(&cube([0.0, 0.0, -42.0], 1.0)), Seen::Not);
    }

    #[test]
    fn meshes_and_models_parts_out_of_sight_are_left_out_unless_they_cast_shadows() {
        use crate::{
            graphics::{MeshData, MeshVertex, ModelPart},
            headless::Headless,
        };
        let mut headless = Headless::new().expect("a GPU adapter is available");
        let ctx = headless.context();
        let box_data = MeshData::cube(1.0);
        let cube = ctx.create_mesh(&box_data).unwrap();
        let mut part = |at: [f32; 3]| ModelPart {
            mesh: ctx.create_mesh(&box_data).unwrap(),
            sprite: None,
            transform: Affine3::from_translation(at),
            double_sided: false,
            emission: Color::BLACK,
            emission_sprite: None,
        };
        // Two parts 20 apart across.
        let pair = Model::new(vec![part([0.0; 3]), part([20.0, 0.0, 0.0])]);
        let moved = Model::new(vec![part([0.0; 3]), part([20.0, 0.0, 0.0])]);
        // Its second part's mesh written 20 farther across, out of the box the model was made
        // with.
        let farther = box_data.vertices.iter().map(|vertex| MeshVertex {
            position: add3(vertex.position, [20.0, 0.0, 0.0]),
            ..*vertex
        });
        let farther: Vec<MeshVertex> = farther.collect();
        ctx.write_mesh(&moved.parts()[1].mesh, &farther).unwrap();

        // Looking along `-z` from the origin: 10 ahead, it takes in 8.46 across.
        let camera = Camera3d::perspective([0.0; 3], [0.0, 0.0, -1.0], 0.8);
        let view = View3d::new(camera, Rect::new(0.0, 0.0, 200.0, 100.0));
        let at = |x: f32, z: f32| Affine3::from_translation([x, 0.0, z]);
        // Stretched 30 along `x`, and the same turned to lie along `z`: from 10 ahead to 20
        // behind, 5 across.
        let along_x = |x: f32, z: f32| Affine3 {
            x_axis: [30.0, 0.0, 0.0],
            ..at(x, z)
        };
        let along_z = |x: f32, z: f32| Affine3 {
            x_axis: [0.0, 0.0, -30.0],
            z_axis: [1.0, 0.0, 0.0],
            ..at(x, z)
        };
        let mirrored = |x: f32, z: f32| Affine3 {
            x_axis: [-1.0, 0.0, 0.0],
            ..at(x, z)
        };
        let white = Color::WHITE;
        let mut canvas = Canvas::new();
        canvas.begin(200.0, 100.0);
        let mut world = canvas.begin_3d(view);
        world.set_casts_shadow(false);
        world.draw_mesh(&cube, at(0.0, -10.0), None, white);
        world.draw_mesh(&cube, at(0.0, 10.0), None, white);
        world.draw_mesh(&cube, along_z(5.0, 5.0), None, white);
        world.draw_mesh(&cube, along_x(5.0, 5.0), None, white);
        // Its parts at 12 across, out of sight, and at -8, in it; and unmirrored, at 12 and 32.
        world.draw_model(&pair, mirrored(12.0, -10.0), white);
        world.draw_model(&pair, at(12.0, -10.0), white);
        world.draw_model(&pair, at(0.0, -10.0), white);
        // By the box it was made with, it's out of sight, but its moved part is in it.
        world.draw_model(&moved, at(-40.0, -10.0), white);
        // Out of sight, but casting shadows, unless blended.
        world.set_casts_shadow(true);
        world.draw_mesh(&cube, at(0.0, 10.0), None, white);
        world.draw_model(&pair, at(0.0, 10.0), white);
        world.set_blend(Some(Blend::Normal));
        world.draw_mesh(&cube, at(0.0, 10.0), None, white);
        world.draw_mesh(&cube, at(0.0, -10.0), None, white);

        let meshes = |draws: &[MeshDraw]| -> Vec<Option<usize>> {
            draws
                .iter()
                .map(|draw| draw.mesh.as_ref().map(Mesh::id))
                .collect()
        };
        let [cube, first, second, moved] = [
            &cube,
            &pair.parts()[0].mesh,
            &pair.parts()[1].mesh,
            &moved.parts()[1].mesh,
        ]
        .map(|mesh| Some(mesh.id()));
        assert_eq!(
            meshes(&canvas.mesh_draws),
            [cube, cube, second, first, moved, cube]
        );
        // Those out of sight that cast shadows, apart.
        assert_eq!(meshes(&canvas.shadow_draws), [cube, first, second]);
        assert_eq!(canvas.instances.len(), 6);
        assert_eq!(canvas.shadow_instances.len(), 3);
        assert_eq!(canvas.segments[0].out_of_sight, 8);
        assert_eq!(canvas.out_of_sight_count(), 11);
        // Those in sight: six cubes of 12 triangles, of five kinds, the blended one among them.
        assert_eq!(canvas.triangle_count(), 72);
        assert_eq!(canvas.draw_call_count(), 5);
    }

    /// How far a channel of a pixel may be off where a block past the fog was left out: the sky
    /// shows there, the same color by the same way it's seen, but the way worked out across the
    /// sky's triangle and not from where the block was, which may round differently. On an RTX
    /// 3080, it doesn't.
    const SKY_ROUNDOFF: u8 = 1;

    /// A field of blocks all round the camera, near and far, some past where the fog is whole,
    /// under a sun that casts shadows, and a slab over the view, out of sight, whose shadow falls
    /// in it: leaving out what's out of sight leaves the same frame as drawing everything, with
    /// the slab's shadow, and with no shadows, the same frame again.
    #[test]
    fn leaving_out_what_is_out_of_sight_shows_the_same() {
        use crate::{graphics::MeshData, headless::Headless};
        let mut headless = Headless::new().expect("a GPU adapter is available");
        let ctx = headless.context();
        let floor = ctx.create_mesh(&MeshData::plane([200.0, 200.0])).unwrap();
        let block = ctx.create_mesh(&MeshData::cuboid([2.0, 3.0, 2.0])).unwrap();
        let slab = ctx.create_mesh(&MeshData::cuboid([3.0, 0.2, 3.0])).unwrap();
        // A little over the floor, looking a little down along `-z`.
        let camera = Camera3d::perspective([0.0, 2.0, 0.0], [0.0, 0.0, -10.0], 0.8);
        let sky = Sky {
            zenith: Color::rgb8(0x20, 0x30, 0x60),
            horizon: Color::rgb8(0x90, 0xa0, 0xb0),
            ground: Color::rgb8(0x30, 0x28, 0x20),
            band: 0.5,
        };
        let view = |shadows| View3d {
            lighting: Lighting {
                direction: [1.0, -1.0, 0.0],
                shadows,
                ..Lighting::default()
            },
            fog: Some(Fog::new(Color::WHITE, 10.0, 45.0)),
            sky: Some(sky),
            ..View3d::new(camera, Rect::new(0.0, 0.0, 128.0, 64.0))
        };
        let mut render = |shadows: Option<SunShadows>, leaves_out: bool, with_slab: bool| {
            let mut left_out = 0;
            let frame = headless
                .render(128, 64, |canvas| {
                    canvas.leaves_out = leaves_out;
                    let mut world = canvas.begin_3d(view(shadows));
                    world.draw_mesh(&floor, Affine3::IDENTITY, None, Color::WHITE);
                    for ring in [12.0, 25.0, 38.0, 60.0] {
                        for step in 0..16 {
                            let angle = step as f32 / 16.0 * std::f32::consts::TAU;
                            let at = [ring * angle.cos(), 1.5, ring * angle.sin()];
                            let tint = Color::rgb8(0xc0, (step * 15) as u8, 0x40);
                            world.set_casts_shadow(step % 2 == 0);
                            world.draw_mesh(&block, Affine3::from_translation(at), None, tint);
                        }
                    }
                    // Over the view, 8 ahead, its shadow falling 6 to its right, in sight.
                    world.set_casts_shadow(true);
                    if with_slab {
                        let over = Affine3::from_translation([-6.0, 6.0, -8.0]);
                        world.draw_mesh(&slab, over, None, Color::WHITE);
                    }
                    left_out = canvas.out_of_sight_count();
                })
                .unwrap();
            (frame, left_out)
        };
        let shadows = Some(SunShadows::new(30.0));
        let (everything, none_left_out) = render(shadows, false, true);
        assert_eq!(none_left_out, 0);
        let (frame, left_out) = render(shadows, true, true);
        assert!(left_out > 32, "{left_out} left out");
        assert!(
            frame
                .compare(&everything, SKY_ROUNDOFF)
                .differs_by_at_most(0),
            "leaving out what's out of sight changed the frame"
        );
        let (without_slab, _) = render(shadows, true, false);
        let shaded = frame.compare(&without_slab, 0);
        assert!(
            !shaded.differs_by_at_most(50),
            "the slab out of sight casts no shadow: {shaded:?}"
        );

        let (everything, _) = render(None, false, true);
        let (frame, _) = render(None, true, true);
        assert!(
            frame
                .compare(&everything, SKY_ROUNDOFF)
                .differs_by_at_most(0)
        );
        let (without_slab, _) = render(None, true, false);
        assert!(
            frame.compare(&without_slab, 0).differs_by_at_most(0),
            "with no shadows, the slab out of sight changes nothing"
        );
    }

    #[test]
    fn meshes_show_what_they_were_made_with_after_their_buffers_grow() {
        use crate::{graphics::MeshData, headless::Headless};
        let mut headless = Headless::new().expect("a GPU adapter is available");
        let ctx = headless.context();
        let floor = ctx.create_mesh(&MeshData::plane([60.0, 60.0])).unwrap();
        // Blocks of a mesh each, of two textures, and a few spheres, which together outgrow
        // the room the mesh buffers have at first.
        let blocks: Vec<Mesh> = (0..24)
            .map(|i| {
                let height = 1.0 + (i % 5) as f32;
                ctx.create_mesh(&MeshData::cuboid([1.5, height, 1.5]))
                    .unwrap()
            })
            .collect();
        let checks = [[0xff, 0x80, 0x40], [0x40, 0x80, 0xff]].map(|[r, g, b]| {
            let rgba: Vec<u8> = (0..4)
                .flat_map(|i| if i % 3 == 0 { [r, g, b, 255] } else { [255; 4] })
                .collect();
            ctx.create_texture(2, 2, &rgba).unwrap()
        });
        let camera = Camera3d::perspective([0.0, 8.0, 14.0], [0.0, 0.0, 0.0], 0.8);
        let view = View3d {
            lighting: Lighting {
                direction: [1.0, -2.0, -1.0],
                shadows: Some(SunShadows::new(30.0)),
                ..Lighting::default()
            },
            ..View3d::new(camera, Rect::new(0.0, 0.0, 128.0, 96.0))
        };
        let sprite = checks[0].sprite();
        let draw = |canvas: &mut Canvas, spheres: &[Mesh]| {
            let mut world = canvas.begin_3d(view);
            world.draw_mesh(&floor, Affine3::IDENTITY, None, Color::WHITE);
            for (i, block) in blocks.iter().enumerate() {
                let angle = i as f32 / 24.0 * std::f32::consts::TAU;
                let at = [6.0 * angle.cos(), 1.0, 6.0 * angle.sin()];
                // Some mirrored, some casting no shadow, a texture each in turn.
                let placed = Affine3 {
                    x_axis: [if i % 7 == 0 { -1.0 } else { 1.0 }, 0.0, 0.0],
                    ..Affine3::from_translation(at)
                };
                world.set_casts_shadow(i % 4 != 0);
                world.draw_mesh(block, placed, Some(&checks[i % 2].sprite()), Color::WHITE);
            }
            world.set_casts_shadow(true);
            for (i, sphere) in spheres.iter().enumerate() {
                let at = Affine3::from_translation([i as f32 - 2.0, 3.0, 0.0]);
                world.draw_mesh(sphere, at, Some(&checks[1].sprite()), Color::WHITE);
            }
            // Billboards showing what blocks do, which cast no shadow.
            world.draw_billboard(&sprite, [0.0, 0.5, 2.0], [1.0, 1.0], Color::WHITE);
            world.set_blend(Some(Blend::Normal));
            let half = Color::rgba(1.0, 1.0, 1.0, 0.5);
            world.draw_mesh(
                &blocks[3],
                Affine3::from_translation([0.0, 1.0, 3.0]),
                None,
                half,
            );
        };
        let before = headless
            .render(128, 96, |canvas| draw(canvas, &[]))
            .unwrap();
        let ctx = headless.context();
        let spheres: Vec<Mesh> = (0..5)
            .map(|_| ctx.create_mesh(&MeshData::sphere(0.5, 48, 24)).unwrap())
            .collect();
        let sizes = ctx.renderer.mesh_buffers().sizes();
        assert!(
            sizes[0][0] > 4096 && sizes[0][1] > 12288,
            "they grew: {sizes:?}"
        );
        let with_spheres = headless
            .render(128, 96, |canvas| draw(canvas, &spheres))
            .unwrap();
        let shown = with_spheres.compare(&before, 0);
        assert!(!shown.differs_by_at_most(50), "the spheres show: {shown:?}");
        // What was made before the buffers grew shows as it did.
        let after = headless
            .render(128, 96, |canvas| draw(canvas, &[]))
            .unwrap();
        assert!(after.compare(&before, 0).differs_by_at_most(0));
        // And meshes dropped give their room to the next.
        let ctx = headless.context();
        let (indices, base) = (spheres[0].indices(), spheres[0].base_vertex());
        drop(spheres);
        let again = ctx.create_mesh(&MeshData::sphere(0.5, 48, 24)).unwrap();
        assert_eq!(again.base_vertex(), base);
        assert_eq!(again.indices(), indices);
    }
}
