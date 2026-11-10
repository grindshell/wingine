//! The 2D physics world as scripts hold it, from `physics.world()`.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use super::{excepted, followed, ray_distance};
use crate::{
    lua::{SceneNode, check_finite},
    math::{Affine, Rect},
    physics::{Body, BodyKind, Character, ColliderId, Material, Pose, Shape, Solid, World},
    scene::{Kind, NodeId, Scene},
};

/// A [`World`] of a scene's nodes, as scripts hold it, from `physics.world()`: colliders that
/// cover nodes as they're drawn, or a tile map's solid tiles, following them as they move and
/// going when they do; bodies, which follow their nodes or, dynamic, move them as the world steps;
/// nodes moved and characters walked among them; and sensors. Queries (`ray`, `overlap_circle`,
/// `overlap_box` and a sensor's `inside`) follow the nodes first, and return nodes, in the order
/// they were added.
pub struct PhysicsWorld {
    world: World,
    /// How many of the scene's units make a metre: densities are of a square metre.
    metre: f32,
    /// The scene its nodes are of: the first node's given to it.
    scene: Option<Rc<RefCell<Scene>>>,
    /// The nodes with colliders, in the order they were added.
    covered: Vec<Covered>,
    /// Where each of `covered` is in it, by its collider and by its node.
    by_collider: HashMap<ColliderId, usize>,
    by_node: HashMap<NodeId, usize>,
    /// The [`Scene::revision`] the colliders last followed their nodes at.
    followed: Option<u64>,
}

/// A node of a [`PhysicsWorld`] with a collider.
struct Covered {
    node: NodeId,
    collider: ColliderId,
    what: What,
}

/// What a node's collider is, and how its node was when it was last placed.
enum What {
    /// Fixed, covering the node where it's drawn, as solid as `solid` says, placed by where the
    /// node was, or nowhere if it covered nothing.
    Drawn {
        solid: Solid,
        placed: Option<(Affine, Rect)>,
    },
    /// A tile map's solid tiles, on `layer` or on any, from where the map's top-left corner was,
    /// as the boxes `boxes` of the solid cells' revision `revision`.
    Tiles {
        layer: Option<usize>,
        origin: [f32; 2],
        revision: u64,
        boxes: Vec<Rect>,
    },
    Body(Box<Bodied>),
}

/// A node's body.
struct Bodied {
    kind: BodyKind,
    shaped: Shaped,
    character: Option<Character>,
    placed: Placed,
    /// Where a character's last walk left its feet in the scene: its body goes there over the
    /// next step, pushing what's in its way, but anywhere else a script puts its node, at once.
    walked: Option<[f32; 2]>,
}

/// What a body is shaped as, from what its node covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shaped {
    Box,
    Circle,
}

/// How a node with a body was placed.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Placed {
    /// A fixed body's node, by its transform from its own coordinates to the scene's, and what it
    /// covers in them, if anything.
    Fixed(Option<(Affine, Rect)>),
    /// A body's that moves.
    Moving(Local),
}

/// Where a node with a body that moves is: where it is in its parent, how far that is from where
/// it is in the scene, how it's turned and stretched, and what it covers in its own coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Local {
    position: [f32; 2],
    offset: [f32; 2],
    rotation: f32,
    scale: [f32; 2],
    rect: Option<Rect>,
}

impl Local {
    /// Where its origin is in the scene.
    fn at(&self) -> [f32; 2] {
        [
            self.position[0] + self.offset[0],
            self.position[1] + self.offset[1],
        ]
    }
}

/// Where the node `id` is, if it can have a body that moves, or an error if a node it's in turns
/// or stretches it: bodies that move are placed only by where they are and how they're turned.
fn local(scene: &Scene, id: NodeId) -> Result<Local, String> {
    let node = scene.node(id);
    let (parent_transform, transform, rect) = scene.transforms_and_rect(id);
    if let (Some(parent), Some(parent_transform)) = (node.parent(), parent_transform)
        && (parent_transform.x_axis != [1.0, 0.0] || parent_transform.y_axis != [0.0, 1.0])
    {
        // The nearest that turns or stretches what's in it.
        let mut turning = parent;
        let mut at = Some(parent);
        while let Some(above) = at {
            let above_node = scene.node(above);
            if above_node.rotation() != 0.0 || above_node.scale() != [1.0, 1.0] {
                turning = above;
                break;
            }
            at = above_node.parent();
        }
        return Err(format!(
            "{} turns or stretches what's in it, so {} can't move in a physics world: only \
             nodes that just move what's in them can be around bodies that move",
            scene.describe(turning),
            scene.describe(id)
        ));
    }
    let at = transform.translation;
    let position = node.position();
    Ok(Local {
        position,
        offset: [at[0] - position[0], at[1] - position[1]],
        rotation: node.rotation(),
        scale: node.scale(),
        rect,
    })
}

/// `rect` stretched by `scale` about the origin, mirrored the right way round.
fn stretch(rect: Rect, [sx, sy]: [f32; 2]) -> Rect {
    let [x0, x1] = [rect.x * sx, (rect.x + rect.w) * sx];
    let [y0, y1] = [rect.y * sy, (rect.y + rect.h) * sy];
    Rect::new(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs())
}

/// The circle inside `rect`, as wide as its widest side.
fn circle_in(rect: Rect) -> ([f32; 2], f32) {
    let center = [rect.x + rect.w / 2.0, rect.y + rect.h / 2.0];
    (center, rect.w.max(rect.h) / 2.0)
}

/// The shape of a body of the node `id`, placed as `local` says, shaped as `shaped` says, or a
/// character's: a box standing on its origin.
fn body_shape(
    scene: &Scene,
    id: NodeId,
    shaped: Shaped,
    character: Option<&Character>,
    local: &Local,
) -> Result<Shape, String> {
    if let Some(character) = character {
        let [width, height] = [character.width, character.height];
        return Ok(Shape::Box(Rect::new(-width / 2.0, -height, width, height)));
    }
    let rect = local.rect.ok_or_else(|| covers_nothing(scene, id))?;
    let stretched = stretch(rect, local.scale);
    Ok(match shaped {
        Shaped::Box => Shape::Box(stretched),
        Shaped::Circle => {
            let (center, radius) = circle_in(stretched);
            Shape::Circle { center, radius }
        }
    })
}

fn covers_nothing(scene: &Scene, id: NodeId) -> String {
    format!(
        "{} covers nothing to shape a body by: only nodes that are drawn do",
        scene.describe(id)
    )
}

/// Where a fixed circle of the node placed as `placed` says goes, and its shape: the circle
/// inside what it covers, as it's drawn, as wide as its widest side.
fn fixed_circle(
    scene: &Scene,
    id: NodeId,
    placed: Option<(Affine, Rect)>,
) -> Result<(Pose, Shape), String> {
    let (transform, rect) = placed.ok_or_else(|| covers_nothing(scene, id))?;
    let (center, radius) = circle_in(rect);
    let length = |[x, y]: [f32; 2]| (x * x + y * y).sqrt();
    let longest = length(transform.x_axis).max(length(transform.y_axis));
    let shape = Shape::Circle {
        center: [0.0; 2],
        radius: radius * longest,
    };
    Ok((Pose::at(transform.apply(center)), shape))
}

/// Where the top-left corner of the tile map `id` is, or of the map of the tile layer `id`, and
/// which revision of its solid cells on `layer`, or on any, it has.
fn tiles_placed(scene: &Scene, id: NodeId, layer: Option<usize>) -> ([f32; 2], u64) {
    let revision = scene
        .tilemap(id)
        .map_or(0, |(_, tilemap)| tilemap.map.solid_revision(layer));
    let origin = match scene.placement(id) {
        Some((transform, rect)) => transform.apply([rect.x, rect.y]),
        None => scene.transform(id).translation,
    };
    (origin, revision)
}

/// The solid tiles of the tile map `id`, on `layer` or on any, as boxes from its top-left corner.
fn tile_boxes(scene: &Scene, id: NodeId, layer: Option<usize>) -> Vec<Rect> {
    scene
        .tilemap(id)
        .map_or_else(Vec::new, |(_, tilemap)| tilemap.map.solid_rects(layer))
}

/// What a script asked a node's collider to be.
enum Asked {
    Drawn {
        solid: Solid,
        material: Option<Material>,
    },
    Tiles(Option<usize>),
    Body {
        kind: BodyKind,
        shaped: Shaped,
        material: Material,
        character: Option<Character>,
        sensor: bool,
    },
}

/// The options `world:add` takes in 2D.
const OPTIONS: [&str; 8] = [
    "edges",
    "sensor",
    "body",
    "shape",
    "density",
    "friction",
    "bounce",
    "character",
];

impl Asked {
    /// From `world:add`'s options, for `id`, in a world with `metre` of the scene's units to a
    /// metre.
    fn read(
        options: Option<&mlua::Table>,
        scene: &Scene,
        id: NodeId,
        metre: f32,
    ) -> mlua::Result<Self> {
        let tiles = match scene.node(id).kind() {
            Kind::Tilemap(_) => Some(None),
            Kind::TileLayer(layer) => Some(Some(layer.layer)),
            _ => None,
        };
        if let Some(layer) = tiles {
            if let Some(options) = options {
                for name in OPTIONS {
                    if !options.get::<mlua::Value>(name)?.is_nil() {
                        return Err(mlua::Error::runtime(format!(
                            "{} is solid where its solid tiles are, which takes no options, \
                             not `{name}`",
                            scene.describe(id)
                        )));
                    }
                }
            }
            return Ok(Self::Tiles(layer));
        }
        let Some(options) = options else {
            return Ok(Self::Drawn {
                solid: Solid::Whole,
                material: None,
            });
        };
        let flag = |name: &str| -> mlua::Result<bool> {
            Ok(options.get::<Option<bool>>(name)?.unwrap_or(false))
        };
        let (edges, sensor) = (flag("edges")?, flag("sensor")?);
        if edges && sensor {
            return Err(mlua::Error::runtime(
                "a sensor is all of its area, not only its edges: give a node `edges` or \
                 `sensor`, not both",
            ));
        }
        let character = match options.get::<Option<mlua::Table>>("character")? {
            Some(character) => Some(read_character_2d(&character, scene, id)?),
            None => None,
        };
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
        let shape = options.get::<Option<String>>("shape")?;
        if character.is_some() && shape.is_some() {
            return Err(mlua::Error::runtime(
                "a character is a box: size it with its `width` and `height`, not a shape",
            ));
        }
        let shaped = match shape.as_deref() {
            None | Some("box") => Shaped::Box,
            Some("circle") => Shaped::Circle,
            Some(other) => {
                return Err(mlua::Error::runtime(format!(
                    "a shape in 2D is \"box\" or \"circle\", not {other:?}"
                )));
            }
        };
        if edges && (kind != BodyKind::Fixed || shaped != Shaped::Box) {
            return Err(mlua::Error::runtime(
                "only a fixed box can be solid only along its edges",
            ));
        }
        // Checked as the world adds the body.
        let defaults = Material::default();
        let number = |name: &str| options.get::<Option<f32>>(name);
        let (density, friction, bounce) =
            (number("density")?, number("friction")?, number("bounce")?);
        let given = density.is_some() || friction.is_some() || bounce.is_some();
        let material = Material {
            // Of a square metre, as Rapier has it of a square unit.
            density: density.unwrap_or(defaults.density) / (metre * metre),
            friction: friction.unwrap_or(defaults.friction),
            bounce: bounce.unwrap_or(defaults.bounce),
        };
        // A fixed box covers its node as it's drawn, turned and stretched as it is.
        if kind == BodyKind::Fixed && shaped == Shaped::Box {
            let solid = match (edges, sensor) {
                (true, _) => Solid::Edges,
                (_, true) => Solid::Sensor,
                _ => Solid::Whole,
            };
            return Ok(Self::Drawn {
                solid,
                material: given.then_some(material),
            });
        }
        Ok(Self::Body {
            kind,
            shaped,
            material,
            character,
            sensor,
        })
    }
}

/// A character from `world:add`'s `character` option, sized by what its node covers unless it
/// says otherwise.
fn read_character_2d(options: &mlua::Table, scene: &Scene, id: NodeId) -> mlua::Result<Character> {
    let shown = scene.rect(id).map(|rect| {
        let [sx, sy] = scene.node(id).scale();
        ((rect.w * sx).abs(), (rect.h * sy).abs())
    });
    let size = |name: &str, shown: Option<f32>| -> mlua::Result<f32> {
        let value = match (options.get::<Option<f32>>(name)?, shown) {
            (Some(value), _) | (None, Some(value)) => value,
            (None, None) => {
                return Err(mlua::Error::runtime(format!(
                    "{} covers nothing to size a character by: give it a {name}",
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
    let width = size("width", shown.map(|(width, _)| width))?;
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
        width,
        height,
        step,
        slope,
        mass,
    })
}

impl PhysicsWorld {
    /// A world whose dynamic bodies fall down the scene at `gravity` units a second each second,
    /// with `metre` of its units to a metre, which Rapier tunes itself for and densities are of.
    pub fn new(gravity: f32, metre: f32) -> Self {
        let mut world = World::new();
        world.set_length_unit(metre);
        world.set_gravity([0.0, gravity]);
        Self {
            world,
            metre,
            scene: None,
            covered: Vec::new(),
            by_collider: HashMap::new(),
            by_node: HashMap::new(),
            followed: None,
        }
    }

    /// Finds where each of `covered` is in it again, after some have gone.
    fn reindex(&mut self) {
        self.by_collider.clear();
        self.by_node.clear();
        for (index, covered) in self.covered.iter().enumerate() {
            self.by_collider.insert(covered.collider, index);
            self.by_node.insert(covered.node, index);
        }
    }

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

    /// Gives `id` a collider as `asked`, or one in place of the one it has, keeping its place
    /// among the world's nodes.
    fn add(&mut self, scene: &Scene, id: NodeId, asked: Asked) -> Result<(), String> {
        let (collider, what) = match asked {
            Asked::Drawn { solid, material } => {
                let placed = scene.placement(id);
                let (transform, rect) = placed.unwrap_or((Affine::IDENTITY, Rect::default()));
                let collider = self.world.add(transform, rect, solid);
                if let Some(material) = material
                    && let Err(err) = self.world.set_material(collider, &material)
                {
                    self.world.remove(collider);
                    return Err(err);
                }
                (collider, What::Drawn { solid, placed })
            }
            Asked::Tiles(layer) => {
                let (origin, revision) = tiles_placed(scene, id, layer);
                let boxes = tile_boxes(scene, id, layer);
                let collider = self.world.add_tiles(origin, &boxes);
                let what = What::Tiles {
                    layer,
                    origin,
                    revision,
                    boxes,
                };
                (collider, what)
            }
            Asked::Body {
                kind,
                shaped,
                material,
                character,
                sensor,
            } => {
                let (pose, shape, placed) = if kind == BodyKind::Fixed {
                    let placement = scene.placement(id);
                    let (pose, shape) = fixed_circle(scene, id, placement)?;
                    (pose, shape, Placed::Fixed(placement))
                } else {
                    let local = local(scene, id)?;
                    // A character stands upright, however its node is turned.
                    let angle = if character.is_some() {
                        0.0
                    } else {
                        local.rotation
                    };
                    let pose = Pose {
                        position: local.at(),
                        angle,
                    };
                    let shape = body_shape(scene, id, shaped, character.as_ref(), &local)?;
                    (pose, shape, Placed::Moving(local))
                };
                let collider = self.world.add_body(&Body {
                    kind,
                    shape,
                    pose,
                    material,
                    sensor,
                })?;
                let bodied = Bodied {
                    kind,
                    shaped,
                    character,
                    placed,
                    walked: None,
                };
                (collider, What::Body(Box::new(bodied)))
            }
        };
        let covered = Covered {
            node: id,
            collider,
            what,
        };
        let index = match self.by_node.get(&id) {
            Some(&index) => {
                let old = std::mem::replace(&mut self.covered[index], covered);
                self.world.remove(old.collider);
                self.by_collider.remove(&old.collider);
                index
            }
            None => {
                self.covered.push(covered);
                self.by_node.insert(id, self.covered.len() - 1);
                self.covered.len() - 1
            }
        };
        self.by_collider.insert(self.covered[index].collider, index);
        Ok(())
    }

    /// Brings the colliders up to date with their nodes: moved, by scripts or with the nodes
    /// they're in, reshaped, or gone. A body whose node can no longer be placed, such as one
    /// whose parent has been turned since it was added, stays where it was. Nothing is read again
    /// if the scene hasn't changed since they last followed it, as between queries.
    fn follow(&mut self, scene: &Scene) {
        if self.followed == Some(scene.revision()) {
            return;
        }
        self.followed = Some(scene.revision());
        let world = &mut self.world;
        let count = self.covered.len();
        self.covered.retain_mut(|covered| {
            if !scene.contains_node(covered.node) {
                world.remove(covered.collider);
                return false;
            }
            follow_one(world, scene, covered);
            true
        });
        if self.covered.len() != count {
            self.reindex();
        }
    }

    /// Brings the colliders of `moved` and the nodes inside them up to date, as the world moved
    /// them, and if the rest were up to date before, `was` the scene's revision then, counts them
    /// all as up to date: so walking a character doesn't make the next query read every node.
    fn follow_moved(&mut self, scene: &Scene, moved: &[NodeId], was: u64) {
        if self.followed != Some(was) {
            return;
        }
        for &id in moved {
            for inside in scene.descendants(id) {
                if let Some(&index) = self.by_node.get(&inside) {
                    follow_one(&mut self.world, scene, &mut self.covered[index]);
                }
            }
        }
        self.followed = Some(scene.revision());
    }

    /// Moves and turns the nodes of dynamic bodies to where their bodies are.
    fn lead(&mut self, scene: &mut Scene) {
        let was = scene.revision();
        let mut moved = Vec::new();
        for covered in &mut self.covered {
            let What::Body(bodied) = &mut covered.what else {
                continue;
            };
            let Placed::Moving(placed) = &mut bodied.placed else {
                continue;
            };
            if bodied.kind != BodyKind::Dynamic || self.world.is_resting(covered.collider) {
                continue;
            }
            let id = covered.node;
            let (Some(pose), Ok(now)) = (self.world.pose(covered.collider), local(scene, id))
            else {
                continue;
            };
            let node = scene.node_mut(id);
            node.set_position([
                pose.position[0] - now.offset[0],
                pose.position[1] - now.offset[1],
            ]);
            node.set_rotation(pose.angle);
            // As following the node will find it.
            if let Ok(after) = local(scene, id) {
                *placed = after;
            }
            moved.push(id);
        }
        self.follow_moved(scene, &moved, was);
    }

    /// Which of `covered` is `node`'s, if it's of this world's scene and has a collider.
    fn covered_of(&self, node: &SceneNode) -> Option<usize> {
        let scene = self.scene.as_ref()?;
        if !Rc::ptr_eq(scene, &node.scene) {
            return None;
        }
        self.by_node.get(&node.id).copied()
    }

    /// The collider of `node`, or an error saying it has none.
    fn collider_of(&self, node: &SceneNode) -> mlua::Result<ColliderId> {
        match self.covered_of(node) {
            Some(index) => Ok(self.covered[index].collider),
            None => {
                let described = node.read(Scene::describe)?;
                Err(mlua::Error::runtime(format!(
                    "{described} isn't in the physics world: add it first"
                )))
            }
        }
    }

    /// Brings the colliders up to date with their nodes before a query, and returns their
    /// scene, or `None` if the world has no nodes yet.
    fn followed(&mut self) -> mlua::Result<Option<Rc<RefCell<Scene>>>> {
        let scene = self.scene.clone();
        followed(scene.as_ref(), |scene| self.follow(scene))
    }

    /// The nodes of `found`, nodes of `scene`, in the order they were added to the world.
    fn nodes(&self, scene: &Rc<RefCell<Scene>>, found: &[ColliderId]) -> Vec<SceneNode> {
        let mut indices: Vec<usize> = found
            .iter()
            .filter_map(|collider| self.by_collider.get(collider).copied())
            .collect();
        indices.sort_unstable();
        indices.dedup();
        indices
            .into_iter()
            .map(|index| SceneNode::new(Rc::clone(scene), self.covered[index].node))
            .collect()
    }

    /// The colliders of the nodes a query's `except` option names. Nodes without colliders in
    /// the world are in nothing's way already.
    fn except(&self, options: Option<&mlua::Table>) -> mlua::Result<Vec<ColliderId>> {
        let nodes = excepted(options)?;
        let colliders = nodes.iter().filter_map(|node| self.covered_of(node));
        Ok(colliders
            .map(|index| self.covered[index].collider)
            .collect())
    }

    /// The nodes whose colliders `query` finds, but those `options` excepts.
    fn query_nodes(
        &mut self,
        options: Option<&mlua::Table>,
        query: impl FnOnce(&mut World, &[ColliderId]) -> Vec<ColliderId>,
    ) -> mlua::Result<Vec<SceneNode>> {
        let except = self.except(options)?;
        let Some(scene) = self.followed()? else {
            return Ok(Vec::new());
        };
        let found = query(&mut self.world, &except);
        Ok(self.nodes(&scene, &found))
    }
}

/// Brings `covered`'s collider up to date with its node, which is in `scene`.
fn follow_one(world: &mut World, scene: &Scene, covered: &mut Covered) {
    let (id, collider) = (covered.node, covered.collider);
    match &mut covered.what {
        What::Drawn { solid, placed } => {
            let now = scene.placement(id);
            if now != *placed {
                let (transform, rect) = now.unwrap_or((Affine::IDENTITY, Rect::default()));
                world.set(collider, transform, rect, *solid);
                *placed = now;
            }
        }
        What::Tiles {
            layer,
            origin,
            revision,
            boxes,
        } => {
            // Reshaped only when the map's solid cells make other boxes, and moved only when it
            // moves, since either disturbs what rests on it.
            let (now_origin, now_revision) = tiles_placed(scene, id, *layer);
            if now_revision != *revision {
                *revision = now_revision;
                let now_boxes = tile_boxes(scene, id, *layer);
                if now_boxes != *boxes {
                    world.set_tiles(collider, now_origin, &now_boxes);
                    *boxes = now_boxes;
                    *origin = now_origin;
                }
            }
            if now_origin != *origin {
                world.move_tiles(collider, now_origin);
                *origin = now_origin;
            }
        }
        What::Body(bodied) => follow_body(world, scene, id, collider, bodied),
    }
}

/// Brings `bodied`, the body of the node `id` whose collider is `collider`, up to date with its
/// node.
fn follow_body(
    world: &mut World,
    scene: &Scene,
    id: NodeId,
    collider: ColliderId,
    bodied: &mut Bodied,
) {
    match bodied.placed {
        Placed::Fixed(was) => {
            let now = scene.placement(id);
            if now == was {
                return;
            }
            // One that covers nothing now stays as it was.
            if let Ok((pose, shape)) = fixed_circle(scene, id, now) {
                let _ = world.set_shape(collider, &shape);
                world.put(collider, pose);
            }
            bodied.placed = Placed::Fixed(now);
        }
        Placed::Moving(was) => {
            let Ok(now) = local(scene, id) else {
                return;
            };
            if now == was {
                return;
            }
            if now.rect != was.rect || now.scale != was.scale {
                let character = bodied.character.as_ref();
                // A shape that can't be made, such as one squashed flat that falls, stays as it
                // was.
                if let Ok(shape) = body_shape(scene, id, bodied.shaped, character, &now) {
                    let _ = world.set_shape(collider, &shape);
                }
            }
            let at = now.at();
            match bodied.character {
                // Upright, however its node is turned: over the next step to where it walked,
                // and at once anywhere else, as when it starts over.
                Some(_) if bodied.walked == Some(at) => world.set_pose(collider, Pose::at(at)),
                Some(_) => world.put(collider, Pose::at(at)),
                None => world.set_pose(
                    collider,
                    Pose {
                        position: at,
                        angle: now.rotation,
                    },
                ),
            }
            bodied.placed = Placed::Moving(now);
        }
    }
}

impl mlua::UserData for PhysicsWorld {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut(
            "add",
            |_, world, (node, options): (mlua::UserDataRef<SceneNode>, Option<mlua::Table>)| {
                world.check(&node)?;
                let metre = world.metre;
                node.read(|scene, id| {
                    let asked = Asked::read(options.as_ref(), scene, id, metre)?;
                    world.add(scene, id, asked).map_err(|err| {
                        let described = scene.describe(id);
                        mlua::Error::runtime(format!("{described} can't have that body: {err}"))
                    })
                })?
            },
        );
        methods.add_method_mut("remove", |_, world, node: mlua::UserDataRef<SceneNode>| {
            if let Some(index) = world.covered_of(&node) {
                let covered = world.covered.remove(index);
                world.world.remove(covered.collider);
                world.reindex();
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
                    let own = world.by_node.get(&id);
                    let own = own.map(|&index| world.covered[index].collider);
                    world.world.slide(at, [dx, dy], own)
                })?;
                // Moved in the scene's coordinates, so in its parent's, by however much the
                // parent turns and stretches it.
                let mut was = 0;
                let moved = node.write(|scene, id| {
                    was = scene.revision();
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
                node.read(|scene, id| world.follow_moved(scene, &[id], was))?;
                Ok((moved[0], moved[1]))
            },
        );
        methods.add_method_mut("step", |_, world, dt: f32| {
            check_finite(&[dt], "a step's time")?;
            let Some(scene) = world.scene.clone() else {
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
            |_, world, (node, dx, dy): (mlua::UserDataRef<SceneNode>, f32, f32)| {
                check_finite(&[dx, dy], "a walk")?;
                let collider = world.collider_of(&node)?;
                let character =
                    world
                        .covered_of(&node)
                        .and_then(|index| match &world.covered[index].what {
                            What::Body(bodied) => bodied.character,
                            _ => None,
                        });
                let Some(character) = character else {
                    let described = node.read(Scene::describe)?;
                    return Err(mlua::Error::runtime(format!(
                        "{described} isn't a character: add it with a `character` option"
                    )));
                };
                // Where its feet are, unless the nodes it's in have since been turned or
                // stretched, which would make its walk theirs too.
                let feet = node
                    .read(|scene, id| {
                        world.follow(scene);
                        local(scene, id).map(|local| local.at())
                    })?
                    .map_err(mlua::Error::runtime)?;
                let walked = world.world.walk(&character, feet, [dx, dy], Some(collider));
                let [mx, my] = walked.moved;
                let mut was = 0;
                let at = node.write(|scene, id| {
                    was = scene.revision();
                    let node = scene.node_mut(id);
                    let [x, y] = node.position();
                    node.set_position([x + mx, y + my]);
                    // As following the node will find it.
                    local(scene, id).map(|local| local.at())
                })?;
                if let Some(index) = world.covered_of(&node)
                    && let What::Body(bodied) = &mut world.covered[index].what
                {
                    bodied.walked = at.ok();
                }
                node.read(|scene, id| world.follow_moved(scene, &[id], was))?;
                Ok((mx, my, walked.grounded))
            },
        );
        methods.add_method(
            "velocity",
            |_, world, node: mlua::UserDataRef<SceneNode>| {
                let [vx, vy] = world.world.velocity(world.collider_of(&node)?);
                Ok((vx, vy))
            },
        );
        methods.add_method_mut(
            "set_velocity",
            |_, world, (node, vx, vy): (mlua::UserDataRef<SceneNode>, f32, f32)| {
                check_finite(&[vx, vy], "a velocity")?;
                let collider = world.collider_of(&node)?;
                world.world.set_velocity(collider, [vx, vy]);
                Ok(())
            },
        );
        methods.add_method_mut(
            "push",
            |_, world, (node, x, y): (mlua::UserDataRef<SceneNode>, f32, f32)| {
                check_finite(&[x, y], "a push")?;
                let collider = world.collider_of(&node)?;
                world.world.push(collider, [x, y]);
                Ok(())
            },
        );
        methods.add_method(
            "is_resting",
            |_, world, node: mlua::UserDataRef<SceneNode>| {
                Ok(world.world.is_resting(world.collider_of(&node)?))
            },
        );
        methods.add_method_mut(
            "ray",
            |lua, world, (x, y, dx, dy, options): (f32, f32, f32, f32, Option<mlua::Table>)| {
                check_finite(&[x, y], "a ray's start")?;
                check_finite(&[dx, dy], "a ray's direction")?;
                if [dx, dy] == [0.0; 2] {
                    return Err(mlua::Error::runtime("a ray's direction can't be zero"));
                }
                let distance = ray_distance(options.as_ref())?;
                let except = world.except(options.as_ref())?;
                let Some(scene) = world.followed()? else {
                    return Ok(None);
                };
                let hit = world.world.cast_ray([x, y], [dx, dy], distance, &except);
                let Some((hit, covered)) = hit.and_then(|hit| {
                    let covered = &world.covered[*world.by_collider.get(&hit.collider)?];
                    Some((hit, covered))
                }) else {
                    return Ok(None);
                };
                let table = lua.create_table_with_capacity(0, 6)?;
                table.raw_set("node", SceneNode::new(scene, covered.node))?;
                table.raw_set("distance", hit.distance)?;
                table.raw_set("x", hit.point[0])?;
                table.raw_set("y", hit.point[1])?;
                table.raw_set("normal_x", hit.normal[0])?;
                table.raw_set("normal_y", hit.normal[1])?;
                Ok(Some(table))
            },
        );
        methods.add_method_mut(
            "overlap_circle",
            |_, world, (x, y, radius, options): (f32, f32, f32, Option<mlua::Table>)| {
                check_finite(&[x, y], "a circle's middle")?;
                if !(radius.is_finite() && radius > 0.0) {
                    let message = format!("a circle's radius must be more than 0, not {radius}");
                    return Err(mlua::Error::runtime(message));
                }
                world.query_nodes(options.as_ref(), |world, except| {
                    world.overlapping_circle([x, y], radius, except)
                })
            },
        );
        methods.add_method_mut(
            "overlap_box",
            |_, world, (area, options): (Rect, Option<mlua::Table>)| {
                check_finite(&[area.x, area.y, area.w, area.h], "a box")?;
                if !(area.w > 0.0 && area.h > 0.0) {
                    let message = format!(
                        "a box must be wider and taller than 0, not {} by {}",
                        area.w, area.h
                    );
                    return Err(mlua::Error::runtime(message));
                }
                world.query_nodes(options.as_ref(), |world, except| {
                    world.overlapping(Affine::IDENTITY, area, except)
                })
            },
        );
        methods.add_method_mut("inside", |_, world, node: mlua::UserDataRef<SceneNode>| {
            let collider = world.collider_of(&node)?;
            if !world.world.is_sensor(collider) {
                let described = node.read(Scene::describe)?;
                return Err(mlua::Error::runtime(format!(
                    "{described} isn't a sensor, so nothing's in it: add it with `sensor = true`"
                )));
            }
            world.query_nodes(None, |world, _| world.inside(collider))
        });
    }
}
