//! Physics worlds as scripts hold them, from `physics.world()` and `physics.world_3d()`: Rapier's
//! colliders and bodies, each of a node, following the node as it moves, or moving it, characters
//! walked among them, and asked where things are by rays, overlaps and sensors.

mod world2d;

use std::{cell::RefCell, collections::HashMap, rc::Rc};

pub use world2d::PhysicsWorld;

use super::{SceneNode, check_finite, not_3d, vector, vector3};
use crate::{
    math::{Affine3, Bounds3},
    physics::{Body3d, BodyId, BodyKind, Character3d, Material, Pose3d, Shape3d, World3d},
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// A fixed body's node, by its transform from its own coordinates to the view's world's.
    Fixed(Affine3),
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

/// Where a body of `kind` for the node `id` goes, and how it's placed, or why it can't be.
fn place(scene: &Scene, id: NodeId, kind: BodyKind) -> Result<(Pose3d, Placed), String> {
    let local = Local::of(scene.node(id)).expect("bodies' nodes are in 3D views");
    if kind == BodyKind::Fixed {
        return Ok((Pose3d::default(), Placed::Fixed(exact_transform(scene, id))));
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

/// The shape of a body of the node `id`, placed as `placed` says, shaped as `shaped` says, or a
/// character's.
fn shape(
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
    let (points, triangles) = geometry(scene.node(id).kind()).ok_or_else(|| {
        format!(
            "{} shows nothing to shape a body by: only meshes and models do",
            scene.describe(id)
        )
    })?;
    let bounds = Bounds3::from_points(points.iter().copied())
        .ok_or_else(|| format!("{} shows nothing, having no corners", scene.describe(id)))?;
    Ok(match placed {
        // In the view's world, by the node's whole transform.
        Placed::Fixed(transform) => {
            let points = || points.iter().map(|&point| transform.apply(point)).collect();
            match shaped {
                Shaped::Triangles => {
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
                Shaped::Box => Shape3d::Hull(corners(&bounds).map(|c| transform.apply(c)).to_vec()),
                Shaped::Ball => {
                    let (center, radius) = ball_of(&bounds);
                    Shape3d::Ball {
                        center: transform.apply(center),
                        radius: radius * longest_axis(transform),
                    }
                }
            }
        }
        // In the body's own coordinates, stretched as the node is.
        Placed::Moving { local, .. } => {
            let stretch = |point: [f32; 3]| [0, 1, 2].map(|i| point[i] * local.scale[i]);
            match shaped {
                Shaped::Triangles => Shape3d::Triangles {
                    points: points.into_iter().map(stretch).collect(),
                    triangles,
                },
                Shaped::Hull => Shape3d::Hull(points.into_iter().map(stretch).collect()),
                Shaped::Box => {
                    let stretched = Bounds3::from_points(corners(&bounds).map(stretch));
                    Shape3d::Box(stretched.expect("a box has corners"))
                }
                Shaped::Ball => {
                    let (center, radius) = ball_of(&bounds);
                    let most = local
                        .scale
                        .iter()
                        .fold(0.0_f32, |most, s| most.max(s.abs()));
                    Shape3d::Ball {
                        center: stretch(center),
                        radius: radius * most,
                    }
                }
            }
        }
    })
}

/// Brings `bodied`'s body up to date with its node, which is in `scene`. A body whose node can no
/// longer be placed stays where it was.
fn follow_one(world: &mut World3d, scene: &Scene, bodied: &mut Bodied) {
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
        _ => true,
    };
    if reshaped {
        let character = bodied.character.as_ref();
        // A shape that can't be made, such as one squashed flat that falls, stays as it was.
        if let Ok(shape) = shape(scene, bodied.node, bodied.shaped, character, &placed) {
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
        None if bodied.kind != BodyKind::Fixed => world.set_pose(bodied.body, pose),
        None => {}
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
        let shape = shape(scene, id, asked.shaped, asked.character.as_ref(), &placed)?;
        let body = self.world.add(&Body3d {
            kind: asked.kind,
            shape,
            pose,
            material: asked.material,
            sensor: asked.sensor,
        })?;
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

    /// Brings the bodies up to date with their nodes: moved, by scripts or with the nodes
    /// they're in, reshaped, or gone. A body whose node can no longer be placed, such as one whose
    /// parent has been turned since it was added, stays where it was. Nothing is read again if the
    /// scene hasn't changed since they last followed it, as between queries.
    fn follow(&mut self, scene: &Scene) {
        if self.followed == Some(scene.revision()) {
            return;
        }
        self.followed = Some(scene.revision());
        let world = &mut self.world;
        let count = self.bodies.len();
        self.bodies.retain_mut(|bodied| {
            if !scene.contains_node(bodied.node) {
                world.remove(bodied.body);
                return false;
            }
            follow_one(world, scene, bodied);
            true
        });
        if self.bodies.len() != count {
            self.reindex();
        }
    }

    /// Brings the bodies of `moved` and the nodes inside them up to date, as the world moved
    /// them, and if the rest were up to date before, `was` the scene's revision then, counts them
    /// all as up to date: so walking a character doesn't make the next query read every node.
    fn follow_moved(&mut self, scene: &Scene, moved: &[NodeId], was: u64) {
        if self.followed != Some(was) {
            return;
        }
        for &id in moved {
            for inside in scene.descendants(id) {
                if let Some(&index) = self.by_node.get(&inside) {
                    follow_one(&mut self.world, scene, &mut self.bodies[index]);
                }
            }
        }
        self.followed = Some(scene.revision());
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
        let except = self.except(options)?;
        let Some(scene) = self.followed()? else {
            return Ok(Vec::new());
        };
        let found = self
            .world
            .overlapping(shape, &except)
            .map_err(mlua::Error::runtime)?;
        Ok(self.nodes(&scene, &found))
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
                let except = world.except(options.as_ref())?;
                let Some(scene) = world.followed()? else {
                    return Ok(None);
                };
                let hit = world.world.cast_ray(origin, direction, distance, &except);
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
