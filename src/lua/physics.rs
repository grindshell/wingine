//! Physics worlds as scripts hold them, from `physics.world()` and `physics.world_3d()`: Rapier's
//! colliders and bodies, each of a node, following the node as it moves, or moving it, characters
//! walked among them, and asked where things are by rays, overlaps and sensors.

mod world2d;

use std::{cell::RefCell, collections::HashMap, rc::Rc};

pub use world2d::PhysicsWorld;

use super::{SceneNode, check_finite, not_3d, vector, vector3};
use crate::{
    graphics::{Mesh, Model},
    math::{Affine3, Bounds3},
    physics::{Body3d, BodyId, BodyKind, Character3d, Material, Passing, Pose3d, Shape3d, World3d},
    scene::{Kind, Node, NodeId, Scene},
};

/// The nodes a query's `except` option names: a node, or a list of them.
fn excepted(options: Option<&mlua::Table>) -> mlua::Result<Vec<mlua::UserDataRef<SceneNode>>> {
    let Some(options) = options else {
        return Ok(Vec::new());
    };
    let nodes: Vec<mlua::AnyUserData> = match options.get::<mlua::Value>("except")? {
        mlua::Value::Nil => Vec::new(),
        mlua::Value::UserData(node) => vec![node],
        mlua::Value::Table(nodes) => nodes.sequence_values().collect::<mlua::Result<_>>()?,
        other => {
            return Err(mlua::Error::runtime(format!(
                "`except` is a node or a list of nodes, not a {}",
                other.type_name()
            )));
        }
    };
    nodes
        .into_iter()
        .map(|node| {
            node.borrow::<SceneNode>().map_err(|_| {
                mlua::Error::runtime("`except` is a node or a list of nodes, not other userdata")
            })
        })
        .collect()
}

/// The tags a query's `except_tagged` option names: a list of strings.
fn except_tagged(options: Option<&mlua::Table>) -> mlua::Result<Vec<String>> {
    let Some(options) = options else {
        return Ok(Vec::new());
    };
    let refused =
        |what: &str| mlua::Error::runtime(format!("`except_tagged` is a list of tags, not {what}"));
    // What Luau calls it: its numbers are all one type.
    let kind = |value: &mlua::Value| match value {
        mlua::Value::Integer(_) | mlua::Value::Number(_) => "number",
        other => other.type_name(),
    };
    match options.get::<mlua::Value>("except_tagged")? {
        mlua::Value::Nil => Ok(Vec::new()),
        mlua::Value::Table(tags) => tags
            .sequence_values::<mlua::Value>()
            .map(|tag| match tag? {
                mlua::Value::String(tag) => Ok(tag.to_str()?.to_string()),
                other => Err(refused(&format!("a list holding a {}", kind(&other)))),
            })
            .collect(),
        other => Err(refused(&format!("a {}", kind(&other)))),
    }
}

/// The tags queries of a world have named in `except_tagged`, each with a bit of the tags its
/// colliders or bodies hold ([`crate::physics::Passing::tags`]), in the order first named, as
/// many as there are bits. One named after those is asked of each node a query comes to, by
/// its name: slower, but the same answer, whatever was named before.
#[derive(Default)]
struct Tags {
    bits: HashMap<String, u32>,
}

impl Tags {
    /// How many tags have bits.
    const MOST: usize = u128::BITS as usize;

    /// The bits of `node`'s tags.
    fn of(&self, node: &Node) -> u128 {
        node.tags()
            .iter()
            .filter_map(|tag| self.bits.get(tag))
            .fold(0, |bits, &bit| bits | 1 << bit)
    }

    /// The bits of `names` that have them, giving one to each that hasn't while there are bits
    /// left, which `give` is to set on what has that tag; and those left without, by name.
    fn named(&mut self, names: &[String], mut give: impl FnMut(&str, u128)) -> (u128, Vec<String>) {
        let mut bits = 0;
        let mut past = Vec::new();
        for name in names {
            let bit = match self.bits.get(name) {
                Some(&bit) => 1 << bit,
                None if self.bits.len() < Self::MOST => {
                    let bit = self.bits.len() as u32;
                    self.bits.insert(name.clone(), bit);
                    give(name, 1 << bit);
                    1 << bit
                }
                None => {
                    past.push(name.clone());
                    0
                }
            };
            bits |= bit;
        }
        (bits, past)
    }
}

/// How far a query's ray goes, from its `distance` option: as far as there is, by default.
fn ray_distance(options: Option<&mlua::Table>) -> mlua::Result<f32> {
    let distance = match options {
        Some(options) => options.get::<Option<f32>>("distance")?,
        None => None,
    };
    let distance = distance.unwrap_or(f32::INFINITY);
    if distance.is_nan() || distance < 0.0 {
        let message = format!("a ray's distance must be 0 or more, not {distance}");
        return Err(mlua::Error::runtime(message));
    }
    Ok(distance.min(f32::MAX))
}

/// Brings `world` up to date with its nodes with `follow` before a query, and returns their
/// scene, or `None` if it has no nodes yet.
fn followed(
    scene: Option<&Rc<RefCell<Scene>>>,
    follow: impl FnOnce(&Scene),
) -> mlua::Result<Option<Rc<RefCell<Scene>>>> {
    let Some(scene) = scene else {
        return Ok(None);
    };
    let borrowed = scene
        .try_borrow()
        .map_err(|_| mlua::Error::runtime("the scene is being changed"))?;
    follow(&borrowed);
    Ok(Some(Rc::clone(scene)))
}

/// A [`World3d`] of the nodes of a 3D view, as scripts hold it, from `physics.world_3d()`: a body
/// for each node given to it, shaped like what the node shows. Fixed and kinematic bodies follow
/// their nodes as they move, dynamic bodies move their nodes as the world steps, and characters
/// walk their nodes among them. Bodies go when their nodes do. Queries (`ray`, `overlap_ball`,
/// `overlap_box` and a sensor's `inside`) follow the nodes first, and return nodes, in the order
/// they were added.
///
/// Following costs by what has changed, not by how many bodies there are: the world has its
/// scene keep which nodes change ([`Scene::keep_changes`]), and looks only at the bodies of
/// those and of what's inside them (see [`PhysicsWorld3d::follow`]). So a write to a node with
/// no body at it or in it, a view's camera or a rig's joint, costs the next query a lookup.
///
/// A query goes through the bodies of the nodes its `except` names, which costs what the list is
/// long for each body it comes to, and through those of nodes tagged with any of its
/// `except_tagged`, which costs the same however many there are: each body holds the bits of its
/// node's tags among those queries have named ([`Tags`]), given as it's added, and to every body
/// at once the first time a query names a tag, once the bodies have followed their nodes. A tag
/// named past the 128th is asked of each body's node by name.
///
/// Adding a body costs by how many models and meshes there are, not by how many bodies: the
/// world makes the shape of a model, shaped one way and stretched one way, once, and keeps it
/// for as long as it lasts, for every body of a copy of the model to share ([`Shapes`]). A body
/// is posed where its node is, its shape in its own coordinates, a fixed one too: unless a node
/// it's in stretches it across how it turns it, which no pose can, when its shape is made for
/// it alone, where it is in the world. A fixed body turned by quarter turns has its shape
/// turned, and is only moved, so that pieces of a kit meet exactly ([`split`]).
///
/// Its arithmetic gives the same bits on every platform, as Rapier's does: where it places a
/// fixed body, it turns its node's yaw, pitch and roll into a transform with `libm`, not as the
/// scene does for drawing, whose sines come from the platform's C library.
#[derive(Default)]
pub struct PhysicsWorld3d {
    world: World3d,
    /// The scene its nodes are of, and the view they're in: the first node's given to it.
    view: Option<(Rc<RefCell<Scene>>, NodeId)>,
    /// The nodes with bodies, in the order they were added.
    bodies: Vec<Bodied>,
    /// Where each of `bodies` is in it, by its body and by its node.
    by_body: HashMap<BodyId, usize>,
    by_node: HashMap<NodeId, usize>,
    /// The [`Scene::revision`] the bodies last followed their nodes at.
    followed: Option<u64>,
    /// How many of the scene's changes ([`Scene::changes_seen`]) they had followed then.
    seen: u64,
    /// The nodes with a body at them or inside them.
    holders: HashMap<NodeId, Holder>,
    /// How many times a body has been looked at to follow its node.
    looked: u64,
    /// The shapes it has made of what its nodes show.
    shapes: Shapes,
    /// The tags its queries have named, which its bodies hold as bits.
    tags: Tags,
}

/// What a [`PhysicsWorld3d`] keeps to follow its nodes by, and what following them has cost:
/// from [`PhysicsWorld3d::following`], for tests and benchmarks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Following {
    /// How many nodes have bodies.
    pub bodies: usize,
    /// How many nodes the world keeps as having a body at them or inside them.
    pub holders: usize,
    /// How many times a body has been looked at to follow its node, since the world was made:
    /// each body once when all are, and otherwise only those at and inside nodes that changed.
    pub looked: u64,
    /// How many shapes it has made of what nodes show, since it was made: one for each model or
    /// mesh, way of shaping, stretch and turn by quarter turns its bodies have had, and one each
    /// time a body was shaped where it is in the world, its node stretched across how it's
    /// turned.
    pub shapes: u64,
}

/// The shapes a [`PhysicsWorld3d`] has made of what its nodes show, kept for as long as it lasts,
/// so that the bodies of copies of a model, shaped and stretched alike, share one: a level is
/// some dozens of models, and as many shapes, however many bodies.
#[derive(Default)]
struct Shapes {
    kept: HashMap<ShapeKey, Kept>,
    /// How many it has made, kept or not.
    made: u64,
}

/// What a kept shape is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ShapeKey {
    /// The model or mesh it was made of, by [`shown_id`].
    of: usize,
    shaped: Shaped,
    /// The bits of its shaping: the axes of the transform its points are moved by, in its body's
    /// coordinates (see [`Placed::Fixed`]).
    shaping: [u32; 9],
}

/// A shape a [`PhysicsWorld3d`] keeps, and the model or mesh it was made of, held so that no
/// other is where it was while the shape is kept by it.
struct Kept {
    shape: Shape3d,
    _of: Box<dyn std::any::Any>,
}

/// What tells the mesh or model `kind` shows from others while it's held, if it shows one.
fn shown_id(kind: &Kind) -> Option<usize> {
    match kind {
        Kind::Mesh(mesh) => Some(mesh.mesh.id()),
        Kind::Model(model) => Some(model.model.id()),
        _ => None,
    }
}

/// A handle to the mesh or model `kind` shows, if it shows one.
fn shown_handle(kind: &Kind) -> Option<Box<dyn std::any::Any>> {
    match kind {
        Kind::Mesh(mesh) => Some(Box::new(Mesh::clone(&mesh.mesh))),
        Kind::Model(model) => Some(Box::new(Model::clone(&model.model))),
        _ => None,
    }
}

impl Shapes {
    /// The shape of a body of `id`, a node of `scene`, shaped as `shaped` says, its points moved
    /// by `shaping`, which only stretches them, or turns them by quarter turns too: the one kept,
    /// if there is one, or else made and kept.
    fn kept(
        &mut self,
        scene: &Scene,
        id: NodeId,
        shaped: Shaped,
        shaping: &Affine3,
    ) -> Result<Shape3d, String> {
        let kind = scene.node(id).kind();
        let [x, y, z] =
            [shaping.x_axis, shaping.y_axis, shaping.z_axis].map(|axis| axis.map(f32::to_bits));
        let key = ShapeKey {
            of: shown_id(kind).ok_or_else(|| shows_nothing(scene, id))?,
            shaped,
            shaping: [x[0], x[1], x[2], y[0], y[1], y[2], z[0], z[1], z[2]],
        };
        if let Some(kept) = self.kept.get(&key) {
            return Ok(kept.shape.clone());
        }
        let ((points, triangles), bounds) = shown(scene, id)?;
        self.made += 1;
        let shape = shape_of(points, triangles, &bounds, shaped, shaping, true).made()?;
        let kept = Kept {
            shape: shape.clone(),
            _of: shown_handle(kind).expect("it shows a mesh or a model"),
        };
        self.kept.insert(key, kept);
        Ok(shape)
    }
}

/// A node of a [`PhysicsWorld3d`] with a body at it or inside it: with the others, the part of
/// the view's tree that leads down to bodies, which the world keeps so that a change to a node
/// says which bodies to look at.
///
/// - When the world has followed all its scene's changes, `placed` is how the node is placed
///   now. Between, it's only ever made, for a node that had no holder, and never changed: so it
///   differs from the node only while a change to that node is yet to be followed, and
///   following that change looks at every body inside the node.
/// - `parent` is the node's as the holder was made, which it stays, a scene having no way to
///   move a node to another parent, and which is still here once the node is gone, when the
///   scene no longer says what it was inside.
#[derive(Clone, Copy, Debug)]
struct Holder {
    /// How many bodies are at the node or inside it.
    bodies: u32,
    /// How the node was placed in its parent when the bodies inside it last followed it.
    placed: Local,
    /// The node it's in, if that's placed in 3D too, and so has a holder.
    parent: Option<NodeId>,
}

/// A node of a [`PhysicsWorld3d`] with a body.
struct Bodied {
    node: NodeId,
    body: BodyId,
    kind: BodyKind,
    shaped: Shaped,
    character: Option<Character3d>,
    /// How the node was placed when the body was last put where it is, or put the node there.
    placed: Placed,
    /// Where a character's last walk left its feet, in the view's world: its body goes there
    /// over the next step, pushing what's in its way, but anywhere else a script puts its node,
    /// at once.
    walked: Option<[f32; 3]>,
}

/// What a body is shaped as, from what its node shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Shaped {
    Triangles,
    Hull,
    Box,
    Ball,
}

impl Shaped {
    const NAMES: [(&str, Self); 4] = [
        ("triangles", Self::Triangles),
        ("hull", Self::Hull),
        ("box", Self::Box),
        ("ball", Self::Ball),
    ];
}

/// How a node with a body was placed.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Placed {
    /// A fixed body's node, by its transform from its own coordinates to the view's world's, and
    /// if that can be split into a pose and a shaping ([`split`]), its shaping: what its shape's
    /// points are moved by in the body's coordinates, as they're posed where the node is.
    Fixed {
        transform: Affine3,
        shaping: Option<Affine3>,
    },
    /// A body's that moves: in its parent, and where the nodes it's in put its parent's origin,
    /// so moving them moves the body too.
    Moving { local: Local, origin: [f32; 3] },
}

/// Where a node is placed in 3D, in its parent: its position, how it's turned, and how much it's
/// stretched.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Local {
    position: [f32; 3],
    yaw: f32,
    pitch: f32,
    roll: f32,
    scale: [f32; 3],
}

impl Local {
    /// How `node` is placed, or `None` if it isn't in a 3D view.
    fn of(node: &Node) -> Option<Self> {
        let placement = node.placement3()?;
        let [x, y] = node.position();
        let [scale_x, scale_y] = node.scale();
        Some(Self {
            position: [x, y, placement.z],
            yaw: placement.yaw,
            pitch: placement.pitch,
            roll: placement.roll,
            scale: [scale_x, scale_y, placement.scale_z],
        })
    }

    /// Whether it only moves what's in it, without turning or stretching it.
    fn only_moves(&self) -> bool {
        [self.yaw, self.pitch, self.roll] == [0.0; 3] && self.scale == [1.0; 3]
    }

    /// From the node's coordinates to its parent's, as [`Node::local_3d`] has it, but with its
    /// sines from `libm`: stretched, rolled, pitched, turned, then moved.
    fn transform(&self) -> Affine3 {
        let (sy, cy) = (libm::sinf(self.yaw), libm::cosf(self.yaw));
        let (sx, cx) = (libm::sinf(self.pitch), libm::cosf(self.pitch));
        let (sz, cz) = (libm::sinf(self.roll), libm::cosf(self.roll));
        let turned = |x_axis, y_axis, z_axis| Affine3 {
            x_axis,
            y_axis,
            z_axis,
            translation: [0.0; 3],
        };
        Affine3::from_translation(self.position)
            * turned([cy, 0.0, -sy], [0.0, 1.0, 0.0], [sy, 0.0, cy])
            * turned([1.0, 0.0, 0.0], [0.0, cx, sx], [0.0, -sx, cx])
            * turned([cz, sz, 0.0], [-sz, cz, 0.0], [0.0, 0.0, 1.0])
            * Affine3::from_scale(self.scale)
    }
}

/// From the coordinates of `id`, a node in a 3D view, to the view's world's, as
/// [`Scene::transform_3d`] has it, but with [`Local::transform`]'s sines.
fn exact_transform(scene: &Scene, id: NodeId) -> Affine3 {
    let node = scene.node(id);
    let Some(local) = Local::of(node) else {
        return Affine3::IDENTITY;
    };
    match node.parent() {
        Some(parent) => exact_transform(scene, parent) * local.transform(),
        None => local.transform(),
    }
}

/// Where the parent of `id`, a node in a 3D view, has its origin in the view's world, or an
/// error if a node it's in, up to the view, turns or stretches it: bodies that move are placed
/// only by where they are and how they're turned.
fn parent_origin(scene: &Scene, id: NodeId) -> Result<[f32; 3], String> {
    let mut origin = [0.0; 3];
    let mut at = scene.node(id).parent();
    while let Some(parent) = at {
        let node = scene.node(parent);
        let Some(local) = Local::of(node) else {
            break;
        };
        if !local.only_moves() {
            return Err(format!(
                "{} turns or stretches what's in it, so {} can't move in a physics world: only \
                 nodes that just move what's in them can be around bodies that move",
                scene.describe(parent),
                scene.describe(id)
            ));
        }
        origin = [0, 1, 2].map(|i| origin[i] + local.position[i]);
        at = node.parent();
    }
    Ok(origin)
}

/// Corners, and three indices into them for each triangle.
type Triangles = (Vec<[f32; 3]>, Vec<[u32; 3]>);

/// The corners and triangles of what `kind` shows, in its node's coordinates: a mesh's, or a
/// model's parts', as they're placed in it. `None` for a node that shows neither.
fn geometry(kind: &Kind) -> Option<Triangles> {
    match kind {
        Kind::Mesh(mesh) => {
            let geometry = mesh.mesh.geometry();
            Some((geometry.points.clone(), geometry.triangles.clone()))
        }
        Kind::Model(model) => {
            let mut points = Vec::new();
            let mut triangles = Vec::new();
            for part in model.model.parts() {
                let geometry = part.mesh.geometry();
                let first = points.len() as u32;
                points.extend(geometry.points.iter().map(|&p| part.transform.apply(p)));
                // Mirrored, its triangles face the other way round.
                let mirrored = part.transform.determinant() < 0.0;
                triangles.extend(geometry.triangles.iter().map(|&[a, b, c]| {
                    let [a, b, c] = [a, b, c].map(|i| first + i);
                    if mirrored { [a, c, b] } else { [a, b, c] }
                }));
            }
            Some((points, triangles))
        }
        _ => None,
    }
}

/// The ball inside the box `bounds`, as wide as its widest side.
fn ball_of(bounds: &Bounds3) -> ([f32; 3], f32) {
    let size = bounds.size();
    (bounds.center(), size[0].max(size[1]).max(size[2]) / 2.0)
}

/// How long the longest of `transform`'s axes is: how much it stretches a ball's radius at most.
fn longest_axis(transform: &Affine3) -> f32 {
    let length =
        |axis: [f32; 3]| (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    length(transform.x_axis)
        .max(length(transform.y_axis))
        .max(length(transform.z_axis))
}

/// What a script asked a node's body to be.
struct Asked {
    kind: BodyKind,
    shaped: Shaped,
    material: Material,
    character: Option<Character3d>,
    sensor: bool,
}

impl Asked {
    /// From `world:add`'s options, for `node`.
    fn read(options: Option<mlua::Table>, scene: &Scene, id: NodeId) -> mlua::Result<Self> {
        let Some(options) = options else {
            return Ok(Self {
                kind: BodyKind::Fixed,
                shaped: Shaped::Triangles,
                material: Material::default(),
                character: None,
                sensor: false,
            });
        };
        let character = match options.get::<Option<mlua::Table>>("character")? {
            Some(character) => Some(read_character(&character, scene, id)?),
            None => None,
        };
        let sensor = options.get::<Option<bool>>("sensor")?.unwrap_or(false);
        if sensor && character.is_some() {
            return Err(mlua::Error::runtime(
                "a character can't be a sensor: it walks into what's solid",
            ));
        }
        let kind = match options.get::<Option<String>>("body")?.as_deref() {
            None if character.is_some() => BodyKind::Kinematic,
            None | Some("fixed") => BodyKind::Fixed,
            Some("dynamic") => BodyKind::Dynamic,
            Some("kinematic") => BodyKind::Kinematic,
            Some(other) => {
                return Err(mlua::Error::runtime(format!(
                    "a body is \"fixed\", \"dynamic\" or \"kinematic\", not {other:?}"
                )));
            }
        };
        if character.is_some() && kind != BodyKind::Kinematic {
            return Err(mlua::Error::runtime(
                "a character's body is kinematic: it moves where it walks",
            ));
        }
        let shaped = match options.get::<Option<String>>("shape")? {
            // A sensor is solid all through.
            None if kind == BodyKind::Fixed && !sensor => Shaped::Triangles,
            None => Shaped::Hull,
            Some(name) => Shaped::NAMES
                .iter()
                .find(|(known, _)| *known == name)
                .map(|(_, shaped)| *shaped)
                .ok_or_else(|| {
                    mlua::Error::runtime(format!(
                        "a shape is \"triangles\", \"hull\", \"box\" or \"ball\", not {name:?}"
                    ))
                })?,
        };
        // Checked as the world adds the body.
        let defaults = Material::default();
        let number = |name: &str, default: f32| -> mlua::Result<f32> {
            Ok(options.get::<Option<f32>>(name)?.unwrap_or(default))
        };
        let material = Material {
            density: number("density", defaults.density)?,
            friction: number("friction", defaults.friction)?,
            bounce: number("bounce", defaults.bounce)?,
        };
        Ok(Self {
            kind,
            shaped,
            material,
            character,
            sensor,
        })
    }
}

/// A character from `world:add`'s `character` option, sized by what its node shows unless it
/// says otherwise.
fn read_character(options: &mlua::Table, scene: &Scene, id: NodeId) -> mlua::Result<Character3d> {
    let shown = geometry(scene.node(id).kind())
        .and_then(|(points, _)| Bounds3::from_points(points))
        .map(|bounds| {
            let [scale_x, scale_y] = scene.node(id).scale();
            let scale_z = scene.node(id).placement3().map_or(1.0, |p| p.scale_z);
            let [x, y, z] = bounds.size();
            let across = (x * scale_x).abs().max((z * scale_z).abs());
            (across / 2.0, (y * scale_y).abs())
        });
    let size = |name: &str, shown: Option<f32>| -> mlua::Result<f32> {
        let value = match (options.get::<Option<f32>>(name)?, shown) {
            (Some(value), _) | (None, Some(value)) => value,
            (None, None) => {
                return Err(mlua::Error::runtime(format!(
                    "{} shows nothing to size a character by: give it a {name}",
                    scene.describe(id)
                )));
            }
        };
        if !(value.is_finite() && value > 0.0) {
            let message = format!("a character's {name} must be more than 0, not {value}");
            return Err(mlua::Error::runtime(message));
        }
        Ok(value)
    };
    let radius = size("radius", shown.map(|(radius, _)| radius))?;
    let height = size("height", shown.map(|(_, height)| height))?;
    let step = options.get::<Option<f32>>("step")?.unwrap_or(height / 3.0);
    let slope = options
        .get::<Option<f32>>("slope")?
        .unwrap_or(std::f32::consts::FRAC_PI_4);
    let mass = options.get::<Option<f32>>("mass")?.unwrap_or(1.0);
    check_finite(&[step, slope, mass], "a character's step, slope and mass")?;
    if step < 0.0 || !(0.0..std::f32::consts::FRAC_PI_2).contains(&slope) || mass <= 0.0 {
        return Err(mlua::Error::runtime(format!(
            "a character's step must be 0 or more, its slope from 0 to less than a quarter turn, \
             and its mass more than 0, not {step}, {slope} and {mass}"
        )));
    }
    Ok(Character3d {
        radius,
        height,
        step,
        slope,
        mass,
    })
}

/// `transform` split into a pose, where it puts the origin and how it turns, and a shaping, the
/// rest, if it can be: if its axes are at right angles to each other, as they are when it only
/// moves, turns and stretches, and not when a parent stretches what's in it across how it's
/// turned.
///
/// The shaping stretches along the node's own axes, a mirrored one by less than 0 along `x`,
/// rounded to within a millionth, so that a node turned by sines that don't square to 1 exactly,
/// as most don't, isn't stretched by a hair: copies of a model, however they're turned, share a
/// shape. A node turned by quarter turns, as a kit's pieces are, is turned by its shaping, its
/// axes exactly the world's, and its pose only moves it: so what's asked of it isn't turned into
/// its coordinates, and where pieces meet edge to edge they meet to the bit, as they did when
/// fixed bodies were shaped where they are, with no crack between them for a ray to pass.
fn split(transform: &Affine3) -> Option<(Pose3d, Affine3)> {
    // To within rounding of the sines it was turned by.
    const SQUARE: f32 = 1.0e-5;
    // How near an axis of the world one of its axes is, to be along it.
    const ALONG: f32 = 1.0e-6;
    let axes = [transform.x_axis, transform.y_axis, transform.z_axis];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let lengths = axes.map(|axis| dot(axis, axis).sqrt());
    if !lengths
        .iter()
        .all(|&length| length > 0.0 && length.is_finite())
    {
        return None;
    }
    let square = [(0, 1), (0, 2), (1, 2)]
        .iter()
        .all(|&(a, b)| dot(axes[a], axes[b]).abs() <= SQUARE * lengths[a] * lengths[b]);
    if !square {
        return None;
    }
    let mirrored = transform.determinant() < 0.0;
    let turned = [0, 1, 2].map(|i| {
        let length = if mirrored && i == 0 {
            -lengths[i]
        } else {
            lengths[i]
        };
        axes[i].map(|value| value / length)
    });
    // To the nearest of every sixteenth of the numbers an `f32` can be, rounding the bits of its
    // length's mantissa, which leaves it with 20.
    let mut stretch = lengths.map(|length| f32::from_bits((length.to_bits() + 8) & !15));
    if mirrored {
        stretch[0] = -stretch[0];
    }
    // Which axis of the world each of its own lies along, and which way, if each does.
    let along = |axis: [f32; 3]| {
        let k = (0..3).find(|&k| (axis[k].abs() - 1.0).abs() <= ALONG)?;
        let rest = (0..3).all(|j| j == k || axis[j].abs() <= ALONG);
        rest.then_some((k, axis[k].signum()))
    };
    if let [Some(x), Some(y), Some(z)] = turned.map(along) {
        let column = |(k, sign): (usize, f32), stretch: f32| {
            let mut column = [0.0; 3];
            column[k] = sign * stretch;
            column
        };
        let shaping = Affine3 {
            x_axis: column(x, stretch[0]),
            y_axis: column(y, stretch[1]),
            z_axis: column(z, stretch[2]),
            translation: [0.0; 3],
        };
        return Some((Pose3d::at(transform.translation), shaping));
    }
    let pose = Pose3d::from_axes(transform.translation, turned);
    Some((pose, Affine3::from_scale(stretch)))
}

/// Where a body of `kind` for the node `id` goes, and how it's placed, or why it can't be.
fn place(scene: &Scene, id: NodeId, kind: BodyKind) -> Result<(Pose3d, Placed), String> {
    let local = Local::of(scene.node(id)).expect("bodies' nodes are in 3D views");
    if kind == BodyKind::Fixed {
        let transform = exact_transform(scene, id);
        let (pose, shaping) = match split(&transform) {
            Some((pose, shaping)) => (pose, Some(shaping)),
            None => (Pose3d::default(), None),
        };
        return Ok((pose, Placed::Fixed { transform, shaping }));
    }
    let origin = parent_origin(scene, id)?;
    let pose = Pose3d {
        position: [0, 1, 2].map(|i| origin[i] + local.position[i]),
        yaw: local.yaw,
        pitch: local.pitch,
        roll: local.roll,
    };
    Ok((pose, Placed::Moving { local, origin }))
}

/// The error for a body of the node `id` that shows nothing to shape it by.
fn shows_nothing(scene: &Scene, id: NodeId) -> String {
    format!(
        "{} shows nothing to shape a body by: only meshes and models do",
        scene.describe(id)
    )
}

/// The corners and triangles of what the node `id` shows, in its own coordinates, and the box
/// around them, or why it shows nothing to shape a body by.
fn shown(scene: &Scene, id: NodeId) -> Result<(Triangles, Bounds3), String> {
    let (points, triangles) =
        geometry(scene.node(id).kind()).ok_or_else(|| shows_nothing(scene, id))?;
    let bounds = Bounds3::from_points(points.iter().copied())
        .ok_or_else(|| format!("{} shows nothing, having no corners", scene.describe(id)))?;
    Ok(((points, triangles), bounds))
}

/// A body's shape of `points` and `triangles`, whose box is `bounds`, shaped as `shaped` says,
/// their points moved by `transform`: mirrored, if it mirrors them. If `aligned`, it keeps a box
/// along the axes, as a shaping does, and a box is a box; otherwise a box is the hull of its
/// corners.
fn shape_of(
    points: Vec<[f32; 3]>,
    triangles: Vec<[u32; 3]>,
    bounds: &Bounds3,
    shaped: Shaped,
    transform: &Affine3,
    aligned: bool,
) -> Shape3d {
    let points = || points.iter().map(|&point| transform.apply(point)).collect();
    match shaped {
        Shaped::Triangles => {
            // Mirrored, its triangles face the other way round.
            let mirrored = transform.determinant() < 0.0;
            let triangles = triangles
                .iter()
                .map(|&[a, b, c]| if mirrored { [a, c, b] } else { [a, b, c] })
                .collect();
            Shape3d::Triangles {
                points: points(),
                triangles,
            }
        }
        Shaped::Hull => Shape3d::Hull(points()),
        Shaped::Box if aligned => {
            let moved = Bounds3::from_points(corners(bounds).map(|c| transform.apply(c)));
            Shape3d::Box(moved.expect("a box has corners"))
        }
        Shaped::Box => Shape3d::Hull(corners(bounds).map(|c| transform.apply(c)).to_vec()),
        Shaped::Ball => {
            let (center, radius) = ball_of(bounds);
            Shape3d::Ball {
                center: transform.apply(center),
                radius: radius * longest_axis(transform),
            }
        }
    }
}

/// The shape of a body of the node `id`, placed as `placed` says, shaped as `shaped` says, or a
/// character's: one of `shapes`, unless it can't be posed.
fn shape(
    shapes: &mut Shapes,
    scene: &Scene,
    id: NodeId,
    shaped: Shaped,
    character: Option<&Character3d>,
    placed: &Placed,
) -> Result<Shape3d, String> {
    if let Some(character) = character {
        return Ok(Shape3d::Cylinder {
            radius: character.radius,
            height: character.height,
        });
    }
    match placed {
        Placed::Fixed {
            shaping: Some(shaping),
            ..
        } => shapes.kept(scene, id, shaped, shaping),
        Placed::Moving { local, .. } => {
            shapes.kept(scene, id, shaped, &Affine3::from_scale(local.scale))
        }
        // Where it is in the view's world, by its node's whole transform, which no pose can be.
        Placed::Fixed {
            transform,
            shaping: None,
        } => {
            let ((points, triangles), bounds) = shown(scene, id)?;
            shapes.made += 1;
            Ok(shape_of(
                points, triangles, &bounds, shaped, transform, false,
            ))
        }
    }
}

/// Brings `bodied`'s body up to date with its node, which is in `scene`, shaping it anew from
/// `shapes` if it's stretched otherwise. A body whose node can no longer be placed stays where
/// it was.
fn follow_one(world: &mut World3d, shapes: &mut Shapes, scene: &Scene, bodied: &mut Bodied) {
    let Ok((pose, placed)) = place(scene, bodied.node, bodied.kind) else {
        return;
    };
    if placed == bodied.placed {
        return;
    }
    let reshaped = match (&placed, &bodied.placed) {
        (Placed::Moving { local: now, .. }, Placed::Moving { local: was, .. }) => {
            now.scale != was.scale
        }
        (
            Placed::Fixed {
                shaping: Some(now), ..
            },
            Placed::Fixed {
                shaping: Some(was), ..
            },
        ) => now != was,
        _ => true,
    };
    if reshaped {
        let character = bodied.character.as_ref();
        // A shape that can't be made, such as one squashed flat that falls, stays as it was.
        if let Ok(shape) = shape(
            shapes,
            scene,
            bodied.node,
            bodied.shaped,
            character,
            &placed,
        ) {
            let _ = world.set_shape(bodied.body, &shape);
        }
    }
    match bodied.character {
        // Upright, however its node is turned: over the next step to where it walked, and at
        // once anywhere else, as when it starts over.
        Some(_) if bodied.walked == Some(pose.position) => {
            world.set_pose(bodied.body, Pose3d::at(pose.position));
        }
        Some(_) => world.put(bodied.body, Pose3d::at(pose.position)),
        // A fixed body at once, wherever it is.
        None => world.set_pose(bodied.body, pose),
    }
    bodied.placed = placed;
}

/// The eight corners of `bounds`.
fn corners(bounds: &Bounds3) -> [[f32; 3]; 8] {
    let [min, max] = [bounds.min, bounds.max];
    std::array::from_fn(|corner| {
        std::array::from_fn(|axis| {
            if corner & (1 << axis) == 0 {
                min[axis]
            } else {
                max[axis]
            }
        })
    })
}

impl PhysicsWorld3d {
    /// A world whose dynamic bodies fall with `gravity`.
    pub fn new(gravity: [f32; 3]) -> Self {
        let mut world = World3d::new();
        world.set_gravity(gravity);
        Self {
            world,
            ..Self::default()
        }
    }

    /// What the world keeps to follow its nodes by, and how many bodies it has looked at doing
    /// so: what shows that a change to a node costs by the bodies it holds.
    pub fn following(&self) -> Following {
        Following {
            bodies: self.bodies.len(),
            holders: self.holders.len(),
            looked: self.looked,
            shapes: self.shapes.made,
        }
    }

    /// Fails unless `node` is in the world's view, which becomes its view if it has none yet.
    fn check(&mut self, node: &SceneNode) -> mlua::Result<()> {
        let view =
            node.read(|scene, id| scene.view_of(id).ok_or_else(|| not_3d(&scene.describe(id))))??;
        match &self.view {
            Some((scene, ours)) if !Rc::ptr_eq(scene, &node.scene) || *ours != view => {
                let described = node.read(Scene::describe)?;
                Err(mlua::Error::runtime(format!(
                    "{described} is in another 3D view than the physics world's nodes"
                )))
            }
            Some(_) => Ok(()),
            None => {
                // From here on the scene says which nodes change, for `follow`.
                node.scene
                    .try_borrow_mut()
                    .map_err(|_| mlua::Error::runtime("the scene is in use"))?
                    .keep_changes();
                self.view = Some((Rc::clone(&node.scene), view));
                Ok(())
            }
        }
    }

    /// Gives `id` a body as `asked`, or replaces the one it has.
    fn add(&mut self, scene: &Scene, id: NodeId, asked: Asked) -> Result<(), String> {
        let (pose, placed) = place(scene, id, asked.kind)?;
        // A character stands upright, however its node is turned.
        let pose = match asked.character {
            Some(_) => Pose3d::at(pose.position),
            None => pose,
        };
        let character = asked.character.as_ref();
        let shape = shape(
            &mut self.shapes,
            scene,
            id,
            asked.shaped,
            character,
            &placed,
        )?;
        let body = self.world.add(&Body3d {
            kind: asked.kind,
            shape,
            pose,
            material: asked.material,
            sensor: asked.sensor,
        })?;
        // Its node's tags, of those queries have named, which they go through it by.
        let tags = self.tags.of(scene.node(id));
        if tags != 0 {
            self.world.set_tags(body, tags);
        }
        let bodied = Bodied {
            node: id,
            body,
            kind: asked.kind,
            shaped: asked.shaped,
            character: asked.character,
            placed,
            walked: None,
        };
        let index = match self.by_node.get(&id) {
            Some(&index) => {
                let old = std::mem::replace(&mut self.bodies[index], bodied);
                self.world.remove(old.body);
                self.by_body.remove(&old.body);
                index
            }
            None => {
                self.bodies.push(bodied);
                self.by_node.insert(id, self.bodies.len() - 1);
                self.hold(scene, id);
                self.bodies.len() - 1
            }
        };
        self.by_body.insert(self.bodies[index].body, index);
        Ok(())
    }

    /// Finds where each of `bodies` is in it again, after some have gone.
    fn reindex(&mut self) {
        self.by_body.clear();
        self.by_node.clear();
        for (index, bodied) in self.bodies.iter().enumerate() {
            self.by_body.insert(bodied.body, index);
            self.by_node.insert(bodied.node, index);
        }
    }

    /// Counts a new body at `node`, which is in `scene`, in the holder of each node from it up to
    /// the view, making those there aren't yet. One there already keeps how it says its node
    /// was placed: the bodies inside it may yet have to follow a change to it.
    fn hold(&mut self, scene: &Scene, node: NodeId) {
        let mut at = Some(node);
        while let Some(id) = at {
            let node = scene.node(id);
            let Some(placed) = Local::of(node) else {
                break;
            };
            let parent = node
                .parent()
                .filter(|&parent| Local::of(scene.node(parent)).is_some());
            self.holders
                .entry(id)
                .and_modify(|holder| holder.bodies += 1)
                .or_insert(Holder {
                    bodies: 1,
                    placed,
                    parent,
                });
            at = parent;
        }
    }

    /// Counts the body at `node` gone, from each holder that counted it, by the parents they
    /// kept: `node` and the nodes it was in may be gone themselves. A holder with no bodies left
    /// goes too.
    fn unhold(&mut self, node: NodeId) {
        let mut at = Some(node);
        while let Some(id) = at {
            let Some(holder) = self.holders.get_mut(&id) else {
                break;
            };
            holder.bodies -= 1;
            at = holder.parent;
            if holder.bodies == 0 {
                self.holders.remove(&id);
            }
        }
    }

    /// Adds to `found` which of `bodies` are at `id`, a node of `scene`, and inside it, in the
    /// order their nodes draw, going down only through nodes that have bodies inside.
    fn inside(&self, scene: &Scene, id: NodeId, found: &mut Vec<usize>) {
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            found.extend(self.by_node.get(&id));
            let children = scene.node(id).children().iter().rev();
            stack.extend(children.filter(|child| self.holders.contains_key(child)));
        }
    }

    /// Brings the bodies up to date with their nodes: moved, by scripts or with the nodes
    /// they're in, reshaped, or gone. A body whose node can no longer be placed, such as one whose
    /// parent has been turned since it was added, stays where it was. Nothing is read again if the
    /// scene hasn't changed since they last followed it, as between queries.
    ///
    /// If it has, only the bodies of the nodes that changed and of the nodes inside those are
    /// looked at ([`Scene::changes_since`]), unless the scene no longer has all the changes since,
    /// when every body is. Either way the world is told the same things in the same order: of
    /// its bodies in the order they were added, each gone or moved at its turn.
    fn follow(&mut self, scene: &Scene) {
        if self.followed == Some(scene.revision()) {
            return;
        }
        let changes = self.followed.and(scene.changes_since(self.seen));
        match changes {
            Some(changes) => self.follow_changed(scene, changes),
            None => self.follow_all(scene),
        }
        self.followed = Some(scene.revision());
        self.seen = scene.changes_seen();
        #[cfg(debug_assertions)]
        self.check_followed(scene);
    }

    /// Brings every body up to date with its node, and every holder with its.
    fn follow_all(&mut self, scene: &Scene) {
        self.looked += self.bodies.len() as u64;
        let world = &mut self.world;
        let shapes = &mut self.shapes;
        let mut gone = Vec::new();
        self.bodies.retain_mut(|bodied| {
            if !scene.contains_node(bodied.node) {
                world.remove(bodied.body);
                gone.push(bodied.node);
                return false;
            }
            follow_one(world, shapes, scene, bodied);
            true
        });
        if !gone.is_empty() {
            for node in gone {
                self.unhold(node);
            }
            self.reindex();
        }
        for (&id, holder) in &mut self.holders {
            if let Some(placed) = scene.get(id).and_then(Local::of) {
                holder.placed = placed;
            }
        }
    }

    /// Brings the bodies up to date with `changes`, the nodes of `scene` changed since they last
    /// followed it: the body of one that's gone goes, and the bodies at and inside one that's
    /// placed otherwise than its holder says follow their nodes. A node with no holder, or
    /// placed as it was, changed nothing a body is placed by.
    fn follow_changed(&mut self, scene: &Scene, changes: &[NodeId]) {
        let mut marked = Vec::new();
        let mut last = None;
        for &id in changes {
            // A node written several times over, as setting where it is and how it's turned.
            if last == Some(id) {
                continue;
            }
            last = Some(id);
            let Some(node) = scene.get(id) else {
                marked.extend(self.by_node.get(&id));
                continue;
            };
            let Some(holder) = self.holders.get_mut(&id) else {
                continue;
            };
            let Some(placed) = Local::of(node) else {
                continue;
            };
            if placed == holder.placed {
                continue;
            }
            holder.placed = placed;
            self.inside(scene, id, &mut marked);
        }
        if marked.is_empty() {
            return;
        }
        // In the order the bodies were added, as following them all goes through them.
        marked.sort_unstable();
        marked.dedup();
        self.looked += marked.len() as u64;
        let mut gone = Vec::new();
        for &index in &marked {
            let bodied = &mut self.bodies[index];
            if scene.contains_node(bodied.node) {
                follow_one(&mut self.world, &mut self.shapes, scene, bodied);
            } else {
                self.world.remove(bodied.body);
                gone.push(index);
            }
        }
        if gone.is_empty() {
            return;
        }
        for &index in &gone {
            self.unhold(self.bodies[index].node);
        }
        let mut gone = gone.into_iter().peekable();
        let mut index = 0;
        self.bodies.retain(|_| {
            let goes = gone.next_if_eq(&index).is_some();
            index += 1;
            !goes
        });
        self.reindex();
    }

    /// In debug builds, that following left what reading every node again would: no body whose
    /// node is gone or placed otherwise than the body is, and no holder behind its node.
    #[cfg(debug_assertions)]
    fn check_followed(&self, scene: &Scene) {
        for bodied in &self.bodies {
            let described = || scene.describe(bodied.node);
            assert!(
                scene.contains_node(bodied.node),
                "a body was left behind by a node that's gone"
            );
            if let Ok((_, placed)) = place(scene, bodied.node, bodied.kind) {
                // Unless it's placed by a number that isn't one, which equals nothing.
                let same = placed;
                assert!(
                    placed == bodied.placed || placed != same,
                    "the body of {} didn't follow it",
                    described()
                );
            }
            assert!(
                self.holders.contains_key(&bodied.node),
                "the body of {} has no holder",
                described()
            );
        }
        let mut held = 0;
        for (&id, holder) in &self.holders {
            let placed = scene.get(id).and_then(Local::of);
            let followed = placed.is_some_and(|placed| {
                let same = placed;
                placed == holder.placed || placed != same
            });
            assert!(followed, "a holder didn't follow its node");
            held += usize::from(self.by_node.contains_key(&id));
        }
        assert_eq!(held, self.bodies.len(), "bodies' nodes without holders");
    }

    /// Brings the bodies of `moved` and the nodes inside them up to date, as the world moved
    /// them, and if the rest were up to date before, `was` the scene's revision then, counts them
    /// all as up to date: so walking a character leaves the next query nothing to follow. If
    /// they weren't, it does nothing, and the next [`PhysicsWorld3d::follow`] finds `moved`
    /// among the scene's changes.
    ///
    /// Scripts can tell this is done as a walk or a step ends, not at the next query:
    /// `is_resting` and `velocity` follow nothing, and a body inside a node the world moved is
    /// awake by then. Each of `moved` is followed in turn, then what's inside it, in the order
    /// they draw.
    fn follow_moved(&mut self, scene: &Scene, moved: &[NodeId], was: u64) {
        if self.followed != Some(was) {
            return;
        }
        let mut inside = Vec::new();
        for &id in moved {
            inside.clear();
            self.inside(scene, id, &mut inside);
            self.looked += inside.len() as u64;
            for &index in &inside {
                let bodied = &mut self.bodies[index];
                follow_one(&mut self.world, &mut self.shapes, scene, bodied);
            }
            let placed = Local::of(scene.node(id));
            if let (Some(holder), Some(placed)) = (self.holders.get_mut(&id), placed) {
                holder.placed = placed;
            }
        }
        self.followed = Some(scene.revision());
        self.seen = scene.changes_seen();
    }

    /// Moves the nodes of dynamic bodies to where their bodies are.
    fn lead(&mut self, scene: &mut Scene) {
        let was = scene.revision();
        let mut moved = Vec::new();
        for bodied in &mut self.bodies {
            let Placed::Moving { local, origin: was } = &mut bodied.placed else {
                continue;
            };
            if bodied.kind != BodyKind::Dynamic || self.world.is_resting(bodied.body) {
                continue;
            }
            let (Some(pose), Ok(origin)) = (
                self.world.pose(bodied.body),
                parent_origin(scene, bodied.node),
            ) else {
                continue;
            };
            local.position = [0, 1, 2].map(|i| pose.position[i] - origin[i]);
            *was = origin;
            local.yaw = pose.yaw;
            local.pitch = pose.pitch;
            local.roll = pose.roll;
            let node = scene.node_mut(bodied.node);
            node.set_position([local.position[0], local.position[1]]);
            if let Some(mut placement) = node.placement3() {
                placement.z = local.position[2];
                placement.yaw = pose.yaw;
                placement.pitch = pose.pitch;
                placement.roll = pose.roll;
                let _ = node.set_placement3(placement);
            }
            moved.push(bodied.node);
        }
        self.follow_moved(scene, &moved, was);
    }

    /// The body of `node`, if it has one in this world, with which of `bodies` it is.
    fn body_of(&self, node: &SceneNode) -> Option<usize> {
        let (scene, _) = self.view.as_ref()?;
        if !Rc::ptr_eq(scene, &node.scene) {
            return None;
        }
        self.by_node.get(&node.id).copied()
    }

    /// Brings the bodies up to date with their nodes before a query, and returns their scene, or
    /// `None` if the world has no nodes yet.
    fn followed(&mut self) -> mlua::Result<Option<Rc<RefCell<Scene>>>> {
        let scene = self.view.as_ref().map(|(scene, _)| Rc::clone(scene));
        followed(scene.as_ref(), |scene| self.follow(scene))
    }

    /// The nodes of `found`, nodes of `scene`, in the order they were added to the world.
    fn nodes(&self, scene: &Rc<RefCell<Scene>>, found: &[BodyId]) -> Vec<SceneNode> {
        let mut indices: Vec<usize> = found
            .iter()
            .filter_map(|body| self.by_body.get(body).copied())
            .collect();
        indices.sort_unstable();
        indices.dedup();
        indices
            .into_iter()
            .map(|index| SceneNode::new(Rc::clone(scene), self.bodies[index].node))
            .collect()
    }

    /// The bodies of the nodes a query's `except` option names: a node, or a list of them. Nodes
    /// without bodies in the world are in nothing's way already.
    fn except(&self, options: Option<&mlua::Table>) -> mlua::Result<Vec<BodyId>> {
        let nodes = excepted(options)?;
        let bodies = nodes.iter().filter_map(|node| self.body_of(node));
        Ok(bodies.map(|index| self.bodies[index].body).collect())
    }

    /// The nodes whose bodies `shape` overlaps, but those `options` excepts.
    fn overlapping(
        &mut self,
        shape: &Shape3d,
        options: Option<&mlua::Table>,
    ) -> mlua::Result<Vec<SceneNode>> {
        let asked = self.passing(options, |world, passing| {
            world.overlapping_passing(shape, passing)
        })?;
        let Some((scene, found)) = asked else {
            return Ok(Vec::new());
        };
        let found = found.map_err(mlua::Error::runtime)?;
        Ok(self.nodes(&scene, &found))
    }

    /// Asks `ask` of its world, once the bodies have followed their nodes, going through what a
    /// query's `options` say: the bodies of the nodes of `except`, and of the nodes tagged with
    /// any of `except_tagged`. Returns the scene and the answer, or `None` if the world has no
    /// nodes yet.
    ///
    /// A tag is read only once the bodies have followed, since a body whose node has gone is
    /// the world's until then, and its node can't be asked. A tag no query has named before is
    /// given a bit then, and the bodies of the nodes with it are given that bit, but for one
    /// whose node isn't in the scene, of which there should be none.
    fn passing<T>(
        &mut self,
        options: Option<&mlua::Table>,
        ask: impl FnOnce(&mut World3d, &Passing<BodyId>) -> T,
    ) -> mlua::Result<Option<(Rc<RefCell<Scene>>, T)>> {
        let except = self.except(options)?;
        let names = except_tagged(options)?;
        let Some(scene) = self.followed()? else {
            return Ok(None);
        };
        let answer = {
            let borrowed = scene
                .try_borrow()
                .map_err(|_| mlua::Error::runtime("the scene is being changed"))?;
            let (world, bodies, by_body) = (&mut self.world, &self.bodies, &self.by_body);
            let (tags, past) = self.tags.named(&names, |name, bit| {
                for bodied in bodies {
                    if borrowed.contains_node(bodied.node)
                        && borrowed.node(bodied.node).has_tag(name)
                    {
                        let had = world.tags(bodied.body);
                        world.set_tags(bodied.body, had | bit);
                    }
                }
            });
            // Tags past those with bits, asked of each body's node by name.
            let also = |body: BodyId| {
                by_body.get(&body).is_some_and(|&index| {
                    let node = bodies[index].node;
                    borrowed.contains_node(node)
                        && past.iter().any(|tag| borrowed.node(node).has_tag(tag))
                })
            };
            let passing = Passing {
                except: &except,
                tags,
                also: if past.is_empty() { None } else { Some(&also) },
            };
            ask(world, &passing)
        };
        Ok(Some((scene, answer)))
    }

    /// The body of `node`, or an error saying it has none.
    fn body(&self, node: &SceneNode) -> mlua::Result<&Bodied> {
        match self.body_of(node) {
            Some(index) => Ok(&self.bodies[index]),
            None => {
                let described = node.read(Scene::describe)?;
                Err(mlua::Error::runtime(format!(
                    "{described} has no body in the physics world: add it first"
                )))
            }
        }
    }
}

impl mlua::UserData for PhysicsWorld3d {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut(
            "add",
            |_, world, (node, options): (mlua::UserDataRef<SceneNode>, Option<mlua::Table>)| {
                world.check(&node)?;
                let scene = node
                    .scene
                    .try_borrow()
                    .map_err(|_| mlua::Error::runtime("the scene is being changed"))?;
                if !scene.contains_node(node.id) {
                    return Err(super::removed());
                }
                let asked = Asked::read(options, &scene, node.id)?;
                world.add(&scene, node.id, asked).map_err(|err| {
                    let described = scene.describe(node.id);
                    mlua::Error::runtime(format!("{described} can't have that body: {err}"))
                })
            },
        );
        methods.add_method_mut("remove", |_, world, node: mlua::UserDataRef<SceneNode>| {
            if let Some(index) = world.body_of(&node) {
                let bodied = world.bodies.remove(index);
                world.world.remove(bodied.body);
                world.unhold(bodied.node);
                world.reindex();
            }
            Ok(())
        });
        methods.add_method_mut("step", |_, world, dt: f32| {
            check_finite(&[dt], "a step's time")?;
            let Some((scene, _)) = world.view.clone() else {
                return Ok(());
            };
            let mut scene = scene
                .try_borrow_mut()
                .map_err(|_| mlua::Error::runtime("the scene is in use"))?;
            world.follow(&scene);
            world.world.step(dt);
            world.lead(&mut scene);
            Ok(())
        });
        methods.add_method_mut(
            "walk",
            |_, world, (node, motion): (mlua::UserDataRef<SceneNode>, mlua::Vector)| {
                let motion = vector3(motion);
                check_finite(&motion, "a walk")?;
                let bodied = world.body(&node)?;
                let (body, character) = match bodied.character {
                    Some(character) => (bodied.body, character),
                    None => {
                        let described = node.read(Scene::describe)?;
                        return Err(mlua::Error::runtime(format!(
                            "{described} isn't a character: add it with a `character` option"
                        )));
                    }
                };
                // Where its feet are, unless the nodes it's in have since been turned or
                // stretched, which would make its walk theirs too.
                let feet = node
                    .read(|scene, id| {
                        world.follow(scene);
                        let origin = parent_origin(scene, id)?;
                        let local =
                            Local::of(scene.node(id)).expect("bodies' nodes are in 3D views");
                        Ok::<_, String>([0, 1, 2].map(|i| origin[i] + local.position[i]))
                    })?
                    .map_err(mlua::Error::runtime)?;
                let walked = world.world.walk(&character, feet, motion, Some(body));
                let mut was = 0;
                let feet = node.write(|scene, id| {
                    was = scene.revision();
                    let node = scene.node_mut(id);
                    let [x, y] = node.position();
                    node.set_position([x + walked.moved[0], y + walked.moved[1]]);
                    if let Some(mut placement) = node.placement3() {
                        placement.z += walked.moved[2];
                        let _ = node.set_placement3(placement);
                    }
                    // As following the node will find it.
                    place(scene, id, BodyKind::Kinematic).map(|(pose, _)| pose.position)
                })?;
                if let Some(index) = world.body_of(&node) {
                    world.bodies[index].walked = feet.ok();
                }
                node.read(|scene, id| world.follow_moved(scene, &[id], was))?;
                Ok((vector(walked.moved), walked.grounded))
            },
        );
        methods.add_method(
            "velocity",
            |_, world, node: mlua::UserDataRef<SceneNode>| {
                let body = world.body(&node)?.body;
                Ok(vector(world.world.velocity(body)))
            },
        );
        methods.add_method_mut(
            "set_velocity",
            |_, world, (node, velocity): (mlua::UserDataRef<SceneNode>, mlua::Vector)| {
                let velocity = vector3(velocity);
                check_finite(&velocity, "a velocity")?;
                let body = world.body(&node)?.body;
                world.world.set_velocity(body, velocity);
                Ok(())
            },
        );
        methods.add_method_mut(
            "push",
            |_, world, (node, impulse): (mlua::UserDataRef<SceneNode>, mlua::Vector)| {
                let impulse = vector3(impulse);
                check_finite(&impulse, "a push")?;
                let body = world.body(&node)?.body;
                world.world.push(body, impulse);
                Ok(())
            },
        );
        methods.add_method(
            "is_resting",
            |_, world, node: mlua::UserDataRef<SceneNode>| {
                let body = world.body(&node)?.body;
                Ok(world.world.is_resting(body))
            },
        );
        methods.add_method_mut(
            "ray",
            |lua,
             world,
             (origin, direction, options): (mlua::Vector, mlua::Vector, Option<mlua::Table>)| {
                let (origin, direction) = (vector3(origin), vector3(direction));
                check_finite(&origin, "a ray's start")?;
                check_finite(&direction, "a ray's direction")?;
                if direction == [0.0; 3] {
                    return Err(mlua::Error::runtime("a ray's direction can't be zero"));
                }
                let distance = ray_distance(options.as_ref())?;
                let asked = world.passing(options.as_ref(), |world, passing| {
                    world.cast_ray_passing(origin, direction, distance, passing)
                })?;
                let Some((scene, hit)) = asked else {
                    return Ok(None);
                };
                let Some((hit, bodied)) = hit.and_then(|hit| {
                    let bodied = &world.bodies[*world.by_body.get(&hit.body)?];
                    Some((hit, bodied))
                }) else {
                    return Ok(None);
                };
                let table = lua.create_table_with_capacity(0, 4)?;
                table.raw_set("node", SceneNode::new(scene, bodied.node))?;
                table.raw_set("distance", hit.distance)?;
                table.raw_set("point", vector(hit.point))?;
                table.raw_set("normal", vector(hit.normal))?;
                Ok(Some(table))
            },
        );
        methods.add_method_mut(
            "overlap_ball",
            |_, world, (center, radius, options): (mlua::Vector, f32, Option<mlua::Table>)| {
                let center = vector3(center);
                check_finite(&center, "a ball's middle")?;
                if !(radius.is_finite() && radius > 0.0) {
                    let message = format!("a ball's radius must be more than 0, not {radius}");
                    return Err(mlua::Error::runtime(message));
                }
                world.overlapping(&Shape3d::Ball { center, radius }, options.as_ref())
            },
        );
        methods.add_method_mut(
            "overlap_box",
            |_, world, (a, b, options): (mlua::Vector, mlua::Vector, Option<mlua::Table>)| {
                let (a, b) = (vector3(a), vector3(b));
                check_finite(&a, "a box's corner")?;
                check_finite(&b, "a box's corner")?;
                // Its corners either way round.
                let bounds = Bounds3 {
                    min: [0, 1, 2].map(|i| a[i].min(b[i])),
                    max: [0, 1, 2].map(|i| a[i].max(b[i])),
                };
                world.overlapping(&Shape3d::Box(bounds), options.as_ref())
            },
        );
        methods.add_method_mut("inside", |_, world, node: mlua::UserDataRef<SceneNode>| {
            let body = world.body(&node)?.body;
            if !world.world.is_sensor(body) {
                let described = node.read(Scene::describe)?;
                return Err(mlua::Error::runtime(format!(
                    "{described} isn't a sensor, so nothing's in it: add it with `sensor = true`"
                )));
            }
            let Some(scene) = world.followed()? else {
                return Ok(Vec::new());
            };
            let found = world.world.inside(body);
            Ok(world.nodes(&scene, &found))
        });
    }
}
