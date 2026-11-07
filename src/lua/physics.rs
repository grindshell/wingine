//! Physics worlds as scripts hold them, from `physics.world()` and `physics.world_3d()`: Rapier's
//! colliders and bodies, each of a node, following the node as it moves, or moving it.

use std::{cell::RefCell, rc::Rc};

use super::{SceneNode, check_finite, not_3d, vector, vector3};
use crate::{
    math::{Affine, Affine3, Bounds3, Rect},
    physics::{
        Body3d, BodyId, BodyKind, Character, ColliderId, Material, Pose3d, Shape3d, Solid, World,
        World3d,
    },
    scene::{Kind, Node, NodeId, Scene},
};

/// A [`World`] of a scene's nodes, as scripts hold it, from `physics.world()`: colliders that
/// cover nodes as they're drawn, following them as they move and going when they do, and nodes
/// moved among them.
#[derive(Default)]
pub struct PhysicsWorld {
    world: World,
    /// The scene its nodes are of: the first node's given to it.
    scene: Option<Rc<RefCell<Scene>>>,
    /// The nodes with colliders, in the order they were added.
    covered: Vec<Covered>,
}

/// A node of a [`PhysicsWorld`] with a collider.
struct Covered {
    node: NodeId,
    solid: Solid,
    collider: ColliderId,
    /// Where the node was when its collider was last placed, or `None` if it covered nothing.
    placed: Option<(Affine, Rect)>,
}

impl PhysicsWorld {
    /// Fails unless `node` is of the 2D part of the world's scene, which becomes its scene if the
    /// world has none yet.
    fn check(&mut self, node: &SceneNode) -> mlua::Result<()> {
        node.check_2d(
            "place in a physics world",
            "physics.world_3d() is for nodes in 3D views",
        )?;
        match &self.scene {
            Some(scene) if !Rc::ptr_eq(scene, &node.scene) => {
                let described = node.read(Scene::describe)?;
                Err(mlua::Error::runtime(format!(
                    "{described} is of another scene than the physics world's nodes"
                )))
            }
            Some(_) => Ok(()),
            None => {
                self.scene = Some(Rc::clone(&node.scene));
                Ok(())
            }
        }
    }

    /// Covers `node` with a collider, solid as `solid` says, or changes how solid it is if it
    /// has one already.
    fn add(&mut self, scene: &Scene, node: NodeId, solid: Solid) {
        let placed = scene.placement(node);
        let (transform, rect) = placed.unwrap_or((Affine::IDENTITY, Rect::default()));
        match self.covered.iter_mut().find(|covered| covered.node == node) {
            Some(covered) => {
                self.world.set(covered.collider, transform, rect, solid);
                covered.solid = solid;
                covered.placed = placed;
            }
            None => {
                let collider = self.world.add(transform, rect, solid);
                self.covered.push(Covered {
                    node,
                    solid,
                    collider,
                    placed,
                });
            }
        }
    }

    /// Brings the colliders up to date with their nodes: moved, reshaped, or gone.
    fn follow(&mut self, scene: &Scene) {
        let world = &mut self.world;
        self.covered.retain_mut(|covered| {
            if !scene.contains_node(covered.node) {
                world.remove(covered.collider);
                return false;
            }
            let placed = scene.placement(covered.node);
            if placed != covered.placed {
                let (transform, rect) = placed.unwrap_or((Affine::IDENTITY, Rect::default()));
                world.set(covered.collider, transform, rect, covered.solid);
                covered.placed = placed;
            }
            true
        });
    }
}

impl mlua::UserData for PhysicsWorld {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut(
            "add",
            |_, world, (node, options): (mlua::UserDataRef<SceneNode>, Option<mlua::Table>)| {
                let edges = match options {
                    Some(options) => options.get::<Option<bool>>("edges")?.unwrap_or(false),
                    None => false,
                };
                let solid = if edges { Solid::Edges } else { Solid::Whole };
                world.check(&node)?;
                node.read(|scene, id| world.add(scene, id, solid))
            },
        );
        methods.add_method_mut("remove", |_, world, node: mlua::UserDataRef<SceneNode>| {
            let ours = world
                .scene
                .as_ref()
                .is_some_and(|scene| Rc::ptr_eq(scene, &node.scene));
            let found = world
                .covered
                .iter()
                .position(|covered| covered.node == node.id);
            if let (true, Some(index)) = (ours, found) {
                let covered = world.covered.remove(index);
                world.world.remove(covered.collider);
            }
            Ok(())
        });
        methods.add_method_mut(
            "move",
            |_, world, (node, dx, dy, area): (mlua::UserDataRef<SceneNode>, f32, f32, Rect)| {
                check_finite(&[dx, dy], "a move")?;
                check_finite(&[area.x, area.y, area.w, area.h], "a box")?;
                if !(area.w > 0.0 && area.h > 0.0) {
                    let message = format!(
                        "a box must be wider and taller than 0, not {} by {}",
                        area.w, area.h
                    );
                    return Err(mlua::Error::runtime(message));
                }
                world.check(&node)?;
                let moved = node.read(|scene, id| {
                    world.follow(scene);
                    let [x, y] = scene.transform(id).apply([0.0, 0.0]);
                    let at = Rect::new(x + area.x, y + area.y, area.w, area.h);
                    // The node's own collider isn't in its way.
                    let own = world.covered.iter().find(|covered| covered.node == id);
                    let own = own.map(|covered| covered.collider);
                    world.world.slide(at, [dx, dy], own)
                })?;
                // Moved in the scene's coordinates, so in its parent's, by however much the
                // parent turns and stretches it.
                let moved = node.write(|scene, id| {
                    let local = match scene.node(id).parent() {
                        Some(parent) => match scene.transform(parent).inverse() {
                            Some(inverse) => Affine {
                                translation: [0.0, 0.0],
                                ..inverse
                            }
                            .apply(moved),
                            None => return [0.0, 0.0],
                        },
                        None => moved,
                    };
                    let [x, y] = scene.node(id).position();
                    scene
                        .node_mut(id)
                        .set_position([x + local[0], y + local[1]]);
                    moved
                })?;
                Ok((moved[0], moved[1]))
            },
        );
    }
}

/// A [`World3d`] of the nodes of a 3D view, as scripts hold it, from `physics.world_3d()`: a body
/// for each node given to it, shaped like what the node shows. Fixed and kinematic bodies follow
/// their nodes as they move, dynamic bodies move their nodes as the world steps, and characters
/// walk their nodes among them. Bodies go when their nodes do.
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
}

/// A node of a [`PhysicsWorld3d`] with a body.
struct Bodied {
    node: NodeId,
    body: BodyId,
    kind: BodyKind,
    shaped: Shaped,
    character: Option<Character>,
    /// How the node was placed when the body was last put where it is, or put the node there.
    placed: Placed,
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
    character: Option<Character>,
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
            });
        };
        let character = match options.get::<Option<mlua::Table>>("character")? {
            Some(character) => Some(read_character(&character, scene, id)?),
            None => None,
        };
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
            None if kind == BodyKind::Fixed => Shaped::Triangles,
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
        let defaults = Material::default();
        let number = |name: &str, default: f32| -> mlua::Result<f32> {
            let value = options.get::<Option<f32>>(name)?.unwrap_or(default);
            if !(value.is_finite() && value >= 0.0) {
                let message = format!("a body's {name} must be 0 or more, not {value}");
                return Err(mlua::Error::runtime(message));
            }
            Ok(value)
        };
        let material = Material {
            density: number("density", defaults.density)?,
            friction: number("friction", defaults.friction)?,
            bounce: number("bounce", defaults.bounce)?,
        };
        if kind == BodyKind::Dynamic && material.density == 0.0 {
            return Err(mlua::Error::runtime(
                "a dynamic body's density must be more than 0, so it weighs something",
            ));
        }
        Ok(Self {
            kind,
            shaped,
            material,
            character,
        })
    }
}

/// A character from `world:add`'s `character` option, sized by what its node shows unless it
/// says otherwise.
fn read_character(options: &mlua::Table, scene: &Scene, id: NodeId) -> mlua::Result<Character> {
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
    Ok(Character {
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
    character: Option<&Character>,
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
        })?;
        let bodied = Bodied {
            node: id,
            body,
            kind: asked.kind,
            shaped: asked.shaped,
            character: asked.character,
            placed,
        };
        match self.bodies.iter_mut().find(|bodied| bodied.node == id) {
            Some(old) => {
                self.world.remove(old.body);
                *old = bodied;
            }
            None => self.bodies.push(bodied),
        }
        Ok(())
    }

    /// Brings the bodies up to date with their nodes: moved, by scripts or with the nodes
    /// they're in, reshaped, or gone. A body whose
    /// node can no longer be placed, such as one whose parent has been turned since it was added,
    /// stays where it was.
    fn follow(&mut self, scene: &Scene) {
        let world = &mut self.world;
        self.bodies.retain_mut(|bodied| {
            if !scene.contains_node(bodied.node) {
                world.remove(bodied.body);
                return false;
            }
            let Ok((pose, placed)) = place(scene, bodied.node, bodied.kind) else {
                return true;
            };
            if placed == bodied.placed {
                return true;
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
            if bodied.kind != BodyKind::Fixed {
                let pose = match bodied.character {
                    // Upright, however its node is turned.
                    Some(_) => Pose3d::at(pose.position),
                    None => pose,
                };
                world.set_pose(bodied.body, pose);
            }
            bodied.placed = placed;
            true
        });
    }

    /// Moves the nodes of dynamic bodies to where their bodies are.
    fn lead(&mut self, scene: &mut Scene) {
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
        }
    }

    /// The body of `node`, if it has one in this world, with which of `bodies` it is.
    fn body_of(&self, node: &SceneNode) -> Option<usize> {
        let (scene, _) = self.view.as_ref()?;
        if !Rc::ptr_eq(scene, &node.scene) {
            return None;
        }
        self.bodies.iter().position(|bodied| bodied.node == node.id)
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
                let feet = node.read(|scene, id| {
                    world.follow(scene);
                    let origin = parent_origin(scene, id).unwrap_or_default();
                    let local = Local::of(scene.node(id)).expect("bodies' nodes are in 3D views");
                    [0, 1, 2].map(|i| origin[i] + local.position[i])
                })?;
                let walked = world.world.walk(&character, feet, motion, Some(body));
                node.write(|scene, id| {
                    let node = scene.node_mut(id);
                    let [x, y] = node.position();
                    node.set_position([x + walked.moved[0], y + walked.moved[1]]);
                    if let Some(mut placement) = node.placement3() {
                        placement.z += walked.moved[2];
                        let _ = node.set_placement3(placement);
                    }
                })?;
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
    }
}
