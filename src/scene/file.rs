//! What scene, tileset and font files hold, and reading them.
//!
//! A file runs as Luau with nothing in scope, and what it returns is turned into plain [`Value`]s
//! first, then read into a [`SceneFile`], a [`TilesetFile`] or a [`FontFile`]. Instances'
//! overrides are checked by the same reading: the node they change, as plain values, with their
//! fields put in.

use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

use super::{
    MAX_NODE_DEPTH,
    clip::{Clip, Interpolation, Keys, Property, Track},
};
use crate::{
    Error, Result,
    graphics::{Align, Blend, Camera3d, Color, Light, Lighting, MeshData, Projection},
    lua::{
        DEFAULT_FOV, is_identifier,
        mlua::{self, chunk::ChunkMode},
        parse_hex, unpacked_color,
    },
    particles::ParticleSettings,
};

/// The version of the scene, tileset and font formats this engine writes, their `format`. It reads
/// files of this format and every one before it: format 2 added 3D views and their nodes, format 3
/// stacks and panels, and format 4 particles and blend modes.
pub const FORMAT: u32 = 4;

/// How deeply tables may nest. Files written by hand or by an editor come nowhere near, but one
/// that builds a table holding itself would otherwise never finish being read.
const MAX_DEPTH: usize = 200;

/// How many values plain data may hold, counting a table each time it's reached. Files and saves
/// come nowhere near, but one holding the same table twice in each of a few dozen nested tables
/// holds it exponentially many times, and would otherwise never finish being read.
const MAX_VALUES: usize = 4_000_000;

/// How many times running a file may loop or call before it's stopped: files are plain values,
/// so even one written to build its tables with loops comes nowhere near, but one that loops
/// forever would otherwise never finish.
const MAX_INTERRUPTS: u32 = 1_000_000;

/// How much memory running a file may take, in bytes, before it's stopped.
const MAX_MEMORY: usize = 256 << 20;

/// What a scene file holds: the sprite sheets its sprites come from, the scene's own props, its
/// nodes, and clips that pose them. See the [module docs](super) for the format.
///
/// Reading a file and writing it back with [`SceneFile::to_luau`] gives the file in its canonical
/// layout, and reading that gives the same `SceneFile`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneFile {
    /// Sprite sheets by name, for sprite nodes to refer to.
    pub sheets: BTreeMap<String, SheetDef>,
    /// Data for the game about the scene as a whole, such as its music.
    pub props: Props,
    /// The nodes at the top of the scene's tree, in the order they draw.
    pub children: Vec<NodeDef>,
    /// Animations of the scene's nodes by name, such as a character's walk, with paths from the
    /// top of the scene.
    pub clips: BTreeMap<String, Clip>,
}

/// An image divided into a grid of sprites the same size, as `graphics.load_sprite_sheet` divides
/// one.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetDef {
    /// The PNG's path, relative to the game's folder.
    pub path: String,
    pub tile_width: u32,
    pub tile_height: u32,
}

/// A node of a scene file: its name, its kind and what that holds, where it is relative to its
/// parent, and its children.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeDef {
    /// Unique among its siblings, without `/`, which separates the names in a path.
    pub name: String,
    pub kind: KindDef,
    /// Where its origin is in its parent's coordinates: from the parent's origin, or with an
    /// `anchor`, from that point of the parent's rectangle.
    pub x: f32,
    pub y: f32,
    /// The point of its parent's rectangle that `x` and `y` are measured from, as fractions of
    /// its width and height from its top-left corner: `[0.5, 0.5]` is its middle. The rectangle
    /// is what the parent covers, the canvas for the nodes at the top of a scene, or for a plain
    /// node, its own parent's, carried along with it.
    pub anchor: Option<[f32; 2]>,
    /// Radians, clockwise, about its origin.
    pub rotation: f32,
    /// How much it and its children are stretched along its own axes, about its origin. Negative
    /// scales mirror.
    pub scale_x: f32,
    pub scale_y: f32,
    /// For a node placed in 3D, in a 3D view, the rest of where it is: `x`, `y`, `scale_x` and
    /// `scale_y` are along the view's world's axes, with `y` up, and this has the rest. Such a
    /// node has no `rotation` or `anchor`. `None` for a node of the 2D scene.
    pub placement3: Option<Placement3>,
    /// Whether it and its children draw.
    pub visible: bool,
    /// Names for finding it and nodes like it, such as `"solid"`.
    pub tags: Vec<String>,
    /// Data for the game, which the engine doesn't read.
    pub props: Props,
    /// The nodes inside it, in the order they draw, after it.
    pub children: Vec<NodeDef>,
}

/// Where a node placed in 3D is, besides its `x`, `y`, `scale_x` and `scale_y`: how far it is
/// along `z`, how it's turned, and how much it's stretched along `z`. It's stretched, then
/// rolled about `z`, pitched about `x` and turned about `y`, then moved, all about its origin, as
/// `graphics.draw_mesh` places a mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement3 {
    pub z: f32,
    /// Radians about `y`, counter-clockwise seen from above.
    pub yaw: f32,
    /// Radians about `x`, counter-clockwise seen from `+x`.
    pub pitch: f32,
    /// Radians about `z`, counter-clockwise seen from `+z`.
    pub roll: f32,
    pub scale_z: f32,
}

impl Default for Placement3 {
    /// At its parent's origin, unturned and unstretched.
    fn default() -> Self {
        Self {
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            scale_z: 1.0,
        }
    }
}

impl NodeDef {
    /// A node of `kind` named `name` at its parent's origin, unturned and unstretched, visible,
    /// with no tags, props or children. A mesh, model or billboard is placed in 3D.
    pub fn new(name: impl Into<String>, kind: KindDef) -> Self {
        Self {
            name: name.into(),
            placement3: kind.is_3d().then(Placement3::default),
            kind,
            x: 0.0,
            y: 0.0,
            anchor: None,
            rotation: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            visible: true,
            tags: Vec::new(),
            props: Props::new(),
            children: Vec::new(),
        }
    }

    /// The node's own fields, as a scene file holds them, without its children: what an
    /// instance's `overrides` change.
    pub fn fields(&self) -> BTreeMap<String, Value> {
        node_fields(self)
    }

    /// The node with the fields `change` sets, as an instance's `overrides` set them: its props
    /// one by one, and any other field but its `name`, `kind` and `children` replaced. `sheets`
    /// are the sheets of the file it's from, and `tilemap` the columns and rows of the tilemap
    /// it's in, if it's a layer. A panel's new appearance replaces the previous one's fields;
    /// a stack or panel's `width` or `height` of `false` restores fitting that axis. Fails as
    /// reading the changed node from a file would.
    pub fn overridden(
        &self,
        change: &BTreeMap<String, Value>,
        sheets: &BTreeMap<String, SheetDef>,
        tilemap: Option<[u32; 2]>,
    ) -> Result<NodeDef, String> {
        let parent = Parent {
            path: "",
            tilemap,
            in_3d: false,
        };
        let mut node = override_node(self, change, sheets, &parent)?;
        node.children = self.children.clone();
        Ok(node)
    }
}

/// `node`, inside `parent`, with the fields `change` sets, as [`NodeDef::overridden`] says, but
/// without its children.
pub(super) fn override_node(
    node: &NodeDef,
    change: &BTreeMap<String, Value>,
    sheets: &BTreeMap<String, SheetDef>,
    parent: &Parent,
) -> Result<NodeDef, String> {
    let mut fields = node_fields(node);
    if let KindDef::Panel(panel) = &node.kind {
        // Replacing a panel's appearance replaces its old variant's fields. Fields explicitly
        // mixed in `change` are still merged below, so the reader rejects conflicting looks.
        let replaced: &[&str] = match &panel.look {
            PanelLookDef::Fill(_)
                if change.contains_key("sheet") || change.contains_key("cell") =>
            {
                &["color", "border", "border_color"]
            }
            PanelLookDef::Sprite { .. }
                if ["color", "border", "border_color"]
                    .iter()
                    .any(|key| change.contains_key(*key)) =>
            {
                &["sheet", "cell", "slice", "tint"]
            }
            _ => &[],
        };
        for &field in replaced {
            fields.remove(field);
        }
    }
    for (field, value) in change {
        match (field.as_str(), value) {
            ("name" | "kind" | "children", _) => {
                return Err(format!("a node's `{field}` can't be overridden"));
            }
            ("props", Value::Map(props)) => {
                if let Some(Value::Map(old)) = fields.get_mut("props") {
                    old.extend(props.clone());
                }
            }
            // An empty table reads as a list, and changes no props.
            ("props", Value::List(items)) if items.is_empty() => {}
            _ => {
                fields.insert(field.clone(), value.clone());
            }
        }
    }
    read_node(Value::Map(fields), parent, 1, sheets, 0)
}

/// What a node is, and what it holds.
#[derive(Clone, Debug, PartialEq)]
pub enum KindDef {
    /// Nothing but a place, for grouping other nodes, turning them together, or marking a spot.
    Node,
    Sprite(SpriteDef),
    Rect(RectDef),
    Text(TextDef),
    Area(AreaDef),
    Tilemap(TilemapDef),
    /// A layer of a tilemap, which must be its parent.
    TileLayer(TileLayerDef),
    /// Another scene, placed here.
    Instance(InstanceDef),
    /// A rectangle of the 2D scene showing a 3D world through a camera, whose nodes are its
    /// children.
    View3d(View3dDef),
    /// A built-in shape, in a 3D view.
    Mesh(MeshDef),
    /// The parts of a model file, in a 3D view.
    Model(ModelDef),
    /// A sprite standing upright in a 3D view, turned to face the camera.
    Billboard(BillboardDef),
    /// A light at a place in a 3D view.
    Light(LightDef),
    /// A rectangle that doesn't draw, laying the nodes inside it out one after another.
    Stack(Stack),
    /// A stack that draws behind what it holds.
    Panel(PanelDef),
    /// An emitter of particles.
    Particles(ParticlesDef),
}

impl KindDef {
    /// Its name in scene files, such as `"sprite"`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Sprite(_) => "sprite",
            Self::Rect(_) => "rect",
            Self::Text(_) => "text",
            Self::Area(_) => "area",
            Self::Tilemap(_) => "tilemap",
            Self::TileLayer(_) => "tile_layer",
            Self::Instance(_) => "instance",
            Self::View3d(_) => "view3d",
            Self::Mesh(_) => "mesh",
            Self::Model(_) => "model",
            Self::Billboard(_) => "billboard",
            Self::Light(_) => "light",
            Self::Stack(_) => "stack",
            Self::Panel(_) => "panel",
            Self::Particles(_) => "particles",
        }
    }

    /// How what it draws mixes with what's under it, if it draws: a sprite, rect, text, tile
    /// layer or particles.
    pub fn blend(&self) -> Option<Blend> {
        match self {
            Self::Sprite(sprite) => Some(sprite.blend),
            Self::Rect(rect) => Some(rect.blend),
            Self::Text(text) => Some(text.blend),
            Self::TileLayer(layer) => Some(layer.blend),
            Self::Particles(particles) => Some(particles.settings.blend),
            _ => None,
        }
    }

    pub fn blend_mut(&mut self) -> Option<&mut Blend> {
        match self {
            Self::Sprite(sprite) => Some(&mut sprite.blend),
            Self::Rect(rect) => Some(&mut rect.blend),
            Self::Text(text) => Some(&mut text.blend),
            Self::TileLayer(layer) => Some(&mut layer.blend),
            Self::Particles(particles) => Some(&mut particles.settings.blend),
            _ => None,
        }
    }

    /// Whether it's a kind only 3D views hold: a mesh, model, billboard or light.
    pub fn is_3d(&self) -> bool {
        is_3d_kind(self.name())
    }

    /// How a stack or a panel is laid out and lays out what it holds.
    pub fn stack(&self) -> Option<&Stack> {
        match self {
            Self::Stack(stack) => Some(stack),
            Self::Panel(panel) => Some(&panel.stack),
            _ => None,
        }
    }

    pub fn stack_mut(&mut self) -> Option<&mut Stack> {
        match self {
            Self::Stack(stack) => Some(stack),
            Self::Panel(panel) => Some(&mut panel.stack),
            _ => None,
        }
    }
}

/// Whether the kind named `kind` is one only 3D views hold.
pub(crate) fn is_3d_kind(kind: &str) -> bool {
    matches!(kind, "mesh" | "model" | "billboard" | "light")
}

/// Whether nodes of the kind named `kind` may be placed in 3D: those only views hold, and plain
/// nodes and instances, which views hold too.
pub(super) fn can_be_3d(kind: &str) -> bool {
    is_3d_kind(kind) || matches!(kind, "node" | "instance")
}

/// A sprite from one of the scene's sheets.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteDef {
    /// The name of the sheet, one of the scene's `sheets`.
    pub sheet: String,
    /// The sprite's column and row in the sheet, counting from 0 at the top-left.
    pub cell: [u32; 2],
    /// The width and height it's stretched to, or its own size in texels.
    pub size: Option<[f32; 2]>,
    /// The fractions of its parent's width and height added to its own: see [`RectDef`].
    pub stretch: [f32; 2],
    /// The point of the sprite, from its top-left corner, at the node's origin, which it turns
    /// and stretches about.
    pub pivot: [f32; 2],
    /// A color its colors are multiplied by.
    pub tint: Color,
    /// How it mixes with what's under it.
    pub blend: Blend,
}

/// A filled rectangle.
#[derive(Clone, Debug, PartialEq)]
pub struct RectDef {
    pub size: [f32; 2],
    /// The fractions of its parent's width and height added to `size`, so it grows and shrinks
    /// with its parent: with a width of 0 and a stretch of `[1, 0]`, it's as wide as its parent.
    /// The parent's rectangle is the one anchors are in: see [`NodeDef::anchor`].
    pub stretch: [f32; 2],
    /// As a sprite's.
    pub pivot: [f32; 2],
    pub color: Color,
    /// As a sprite's.
    pub blend: Blend,
}

/// Text in a font: a font file's, or the built-in one's.
#[derive(Clone, Debug, PartialEq)]
pub struct TextDef {
    pub text: String,
    /// The font file it's in, by its path from the game's folder, or the built-in font.
    pub font: Option<String>,
    /// How many times the font's size it's drawn, such as 2 for a title.
    pub font_scale: f32,
    /// The width it wraps to between words, if any.
    pub wrap: Option<f32>,
    /// How its lines line up, and which part of them is at the node's origin: their left edges,
    /// their middles or their right edges.
    pub align: Align,
    /// The point of the text at the node's origin, from the top of the edge or middle it's
    /// aligned by. Text moves with its node but doesn't turn or stretch.
    pub pivot: [f32; 2],
    pub color: Color,
    /// As a sprite's.
    pub blend: Blend,
}

/// A rectangle that doesn't draw, for marking out part of a scene, such as somewhere the player
/// can't walk or something happens, or for laying out the nodes inside it.
#[derive(Clone, Debug, PartialEq)]
pub struct AreaDef {
    pub size: [f32; 2],
    /// As a rect's.
    pub stretch: [f32; 2],
    /// As a sprite's.
    pub pivot: [f32; 2],
}

/// How a stack or panel is sized and placed, and how it lays out the nodes inside it: one after
/// another, down or across, each taking the space it covers. It's the same in scene files and in
/// scenes loaded to play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stack {
    /// Its width and height, or `None` along an axis it fits what it holds, with its padding.
    pub size: [Option<f32>; 2],
    /// As a rect's, along the axes it has a size of its own.
    pub stretch: [f32; 2],
    /// The point of it at the node's origin, which it turns and stretches about: from the point
    /// of it its anchor names, as fractions of its width and height, or from its top-left
    /// corner without an anchor. So an anchored stack stays lined up with its parent however
    /// much it holds.
    pub pivot: [f32; 2],
    pub direction: StackDirection,
    /// The space between each child and the next.
    pub gap: f32,
    /// The space between its edges and what it holds: left, top, right and bottom.
    pub padding: [f32; 4],
}

impl Stack {
    /// A stack laying out a column, fitting what it holds both ways, with no gap or padding.
    pub const FITTING: Self = Self {
        size: [None, None],
        stretch: [0.0, 0.0],
        pivot: [0.0, 0.0],
        direction: StackDirection::Column,
        gap: 0.0,
        padding: [0.0; 4],
    };
}

/// Which way a stack's children follow one another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackDirection {
    /// Down, from the top.
    Column,
    /// Across, from the left.
    Row,
}

impl StackDirection {
    /// Its name in scene files: `"column"` or `"row"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Column => "column",
            Self::Row => "row",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "column" => Some(Self::Column),
            "row" => Some(Self::Row),
            _ => None,
        }
    }

    /// The axis children follow one another along: 0 across, 1 down.
    pub fn axis(self) -> usize {
        match self {
            Self::Row => 0,
            Self::Column => 1,
        }
    }
}

/// A stack that draws behind what it holds: a fill with a border, or a sprite sliced so its
/// edges keep their size.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelDef {
    pub stack: Stack,
    pub look: PanelLookDef,
}

/// What a panel draws.
#[derive(Clone, Debug, PartialEq)]
pub enum PanelLookDef {
    Fill(PanelFill),
    /// A sprite of one of the scene's sheets, sliced: see [`Canvas::draw_sprite_sliced`].
    ///
    /// [`Canvas::draw_sprite_sliced`]: crate::graphics::Canvas::draw_sprite_sliced
    Sprite {
        sprite: CellDef,
        /// The texels along its left, top, right and bottom edges that keep their size.
        slice: [f32; 4],
        tint: Color,
    },
}

/// A panel filled with a color, within a border of another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelFill {
    pub color: Color,
    /// How wide the border is, inside the panel's edges: 0 for none.
    pub border: f32,
    pub border_color: Color,
}

/// A grid of tiles from a tileset file, its top-left corner at the node's origin, with its layers
/// as its children. A tilemap moves with its node, but doesn't turn or stretch.
#[derive(Clone, Debug, PartialEq)]
pub struct TilemapDef {
    /// The tileset file's path, relative to the game's folder.
    pub tileset: String,
    pub columns: u32,
    pub rows: u32,
}

/// A layer of a tilemap: which tile is in each cell, or none.
#[derive(Clone, Debug, PartialEq)]
pub struct TileLayerDef {
    /// The tile each character in `tiles` stands for, by its name in the tileset. A space is an
    /// empty cell.
    pub legend: BTreeMap<char, String>,
    /// The cells, a string for each row with a character for each column.
    pub tiles: Vec<String>,
    /// How its tiles mix with what's under them.
    pub blend: Blend,
}

/// An emitter of particles: what it sends out and how, from where. They start from random points
/// of its rectangle, laid out as an area's, or from its origin while it's 0 by 0, and draw where
/// it is among the scene's nodes. See [`ParticleSettings`].
#[derive(Clone, Debug, PartialEq)]
pub struct ParticlesDef {
    pub settings: ParticleSettings,
    /// What each particle shows, or a white square without one.
    pub sprite: Option<CellDef>,
    /// The rectangle particles start in, 0 by 0 by default.
    pub size: [f32; 2],
    /// As a rect's.
    pub stretch: [f32; 2],
    /// As a sprite's.
    pub pivot: [f32; 2],
}

impl ParticlesDef {
    /// Particles from a point, with `settings`, showing white squares.
    pub fn new(settings: ParticleSettings) -> Self {
        Self {
            settings,
            sprite: None,
            size: [0.0, 0.0],
            stretch: [0.0, 0.0],
            pivot: [0.0, 0.0],
        }
    }
}

/// Another scene file, placed at the node, with changes to some of its nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct InstanceDef {
    /// The scene file's path, relative to the game's folder.
    pub scene: String,
    /// Changes to the scene's nodes, by their paths from its top: the fields to set, as a scene
    /// file would hold them. Props are changed one by one rather than replaced.
    pub overrides: BTreeMap<String, BTreeMap<String, Value>>,
}

/// A rectangle of the 2D scene showing a 3D world, laid out as an area is, which moves with its
/// node but doesn't turn or stretch. Its children are the world's nodes, placed in 3D.
#[derive(Clone, Debug, PartialEq)]
pub struct View3dDef {
    pub size: [f32; 2],
    /// As a rect's.
    pub stretch: [f32; 2],
    /// As a sprite's.
    pub pivot: [f32; 2],
    pub camera: Camera3d,
    pub lighting: Lighting,
}

/// A sprite of one of a scene's sheets: the sheet's name and the sprite's column and row in it.
#[derive(Clone, Debug, PartialEq)]
pub struct CellDef {
    pub sheet: String,
    pub cell: [u32; 2],
}

/// A built-in shape in a 3D view, showing a sprite or white, times its tint.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshDef {
    pub shape: Shape,
    pub sprite: Option<CellDef>,
    pub tint: Color,
}

/// A built-in shape, centered on its node's origin: see [`MeshData`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// A box, its width, height and depth along `x`, `y` and `z`.
    Cuboid([f32; 3]),
    /// A flat rectangle facing up, its width along `x` and depth along `z`.
    Plane([f32; 2]),
    /// A ball of a radius.
    Sphere(f32),
}

impl Shape {
    /// Its name in scene files: `"cuboid"`, `"plane"` or `"sphere"`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Cuboid(_) => "cuboid",
            Self::Plane(_) => "plane",
            Self::Sphere(_) => "sphere",
        }
    }

    /// Its triangles.
    pub fn mesh_data(&self) -> MeshData {
        match *self {
            Self::Cuboid(size) => MeshData::cuboid(size),
            Self::Plane(size) => MeshData::plane(size),
            Self::Sphere(radius) => MeshData::sphere(radius, SPHERE_SEGMENTS, SPHERE_RINGS),
        }
    }
}

/// How many slices round and rings from top to bottom spheres of scenes are made of, as
/// `graphics.new_sphere` makes them by default.
const SPHERE_SEGMENTS: u32 = 16;
const SPHERE_RINGS: u32 = 8;

/// A model file in a 3D view, times its tint.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelDef {
    /// The model file's path, relative to the game's folder.
    pub model: String,
    pub tint: Color,
}

/// A sprite standing upright in a 3D view with the middle of its bottom edge at the node's origin,
/// turned to face the camera across the ground. It moves with its node but doesn't turn or
/// stretch.
#[derive(Clone, Debug, PartialEq)]
pub struct BillboardDef {
    pub sheet: String,
    pub cell: [u32; 2],
    /// Its width and height in world units. A negative width mirrors it.
    pub size: [f32; 2],
    pub tint: Color,
}

/// A light at its node's place in a 3D view, lighting what's within its range: see [`Light`].
/// With a `cone` it's a spot light, shining along its node's `+z`, the way a model's front
/// faces. It moves and turns with its node, but doesn't stretch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightDef {
    /// Its light on a surface facing it squarely, at its place.
    pub color: Color,
    /// How far it reaches, in world units.
    pub range: f32,
    /// How many times its color it gives.
    pub energy: f32,
    /// Half the angle of a spot light's cone, in radians, up to pi. `None` is a point light.
    pub cone: Option<f32>,
    /// The part of the cone, from its rim inward, across which a spot light fades.
    pub edge: f32,
}

impl Default for LightDef {
    /// A white point light reaching 5 world units.
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            range: 5.0,
            energy: 1.0,
            cone: None,
            edge: 0.0,
        }
    }
}

impl LightDef {
    /// The light it is at `position`, shining along `direction` if it's a spot light.
    pub fn light(&self, position: [f32; 3], direction: [f32; 3]) -> Light {
        Light {
            position,
            color: self.color,
            range: self.range,
            energy: self.energy,
            direction,
            cone: self.cone,
            edge: self.edge,
        }
    }

    /// What's wrong with it, if it can't light anything as it is, naming its fields as scene
    /// files do.
    pub fn problem(&self) -> Option<String> {
        if !(self.range > 0.0 && self.range.is_finite()) {
            return Some(format!("`range` must be above 0, not {}", self.range));
        }
        if !(self.energy >= 0.0 && self.energy.is_finite()) {
            return Some(format!(
                "`energy` can't be less than 0, not {}",
                self.energy
            ));
        }
        if let Some(cone) = self.cone
            && !(cone > 0.0 && cone <= std::f32::consts::PI)
        {
            return Some(format!("`cone` is an angle above 0, up to pi, not {cone}"));
        }
        if !(0.0..=1.0).contains(&self.edge) {
            return Some(format!("`edge` is from 0 to 1, not {}", self.edge));
        }
        if self.cone.is_none() && self.edge != 0.0 {
            return Some("a light has an `edge` only with a `cone`, whose edge it is".into());
        }
        None
    }
}

/// What a tileset file holds: the kinds of tile tilemaps are made of, by name, all the same size.
#[derive(Clone, Debug, PartialEq)]
pub struct TilesetFile {
    /// The size of every tile in a map, which its sprite stretches to.
    pub tile_width: u32,
    pub tile_height: u32,
    /// Sprite sheets by name, for tiles to refer to.
    pub sheets: BTreeMap<String, SheetDef>,
    pub tiles: BTreeMap<String, TileDef>,
}

/// A kind of tile.
#[derive(Clone, Debug, PartialEq)]
pub struct TileDef {
    /// The name of the sheet its sprite is in, one of the tileset's `sheets`.
    pub sheet: String,
    /// The sprite's column and row in the sheet.
    pub cell: [u32; 2],
    pub tint: Color,
    /// Whether a cell holding it, on any layer, blocks movement.
    pub solid: bool,
    /// Data for the game about tiles of this kind.
    pub props: Props,
}

/// What a font file holds, named `*.font.luau` by convention: a bitmap font, whose glyphs are
/// drawn in an image, or a TrueType or OpenType font file at a size. See the [module docs](super)
/// for the format.
#[derive(Clone, Debug, PartialEq)]
pub enum FontFile {
    Bitmap(BitmapFontDef),
    Vector(VectorFontDef),
}

/// A font whose glyphs are drawn in an image, divided into cells as a sprite sheet is.
#[derive(Clone, Debug, PartialEq)]
pub struct BitmapFontDef {
    /// The image, by its path from the font file's folder.
    pub image: String,
    /// How many texels across and down each cell is.
    pub cell: [u32; 2],
    /// Which character each cell draws.
    pub characters: FontCharacters,
    /// How many texels of space follow each glyph.
    pub spacing: u32,
    /// How far a space moves the pen.
    pub space: u32,
    /// How far it is from the top of one line to the top of the next.
    pub line_height: u32,
    /// Whether every glyph is a whole cell wide, rather than as wide as its ink.
    pub monospace: bool,
}

/// Which character each of a bitmap font's cells draws, a space for a cell it skips.
#[derive(Clone, Debug, PartialEq)]
pub enum FontCharacters {
    /// One string, cell by cell, left to right, carrying on down a row at the image's right edge.
    Cells(String),
    /// A string for each row of cells, from the top.
    Rows(Vec<String>),
}

impl FontCharacters {
    /// Each character with its cell's column and row, given how many columns of cells there are,
    /// skipping cells marked by spaces.
    pub fn cells(&self, columns: u32) -> Vec<(char, [u32; 2])> {
        fn index(i: usize) -> u32 {
            u32::try_from(i).unwrap_or(u32::MAX)
        }
        let columns = columns.max(1) as usize;
        let cells: Vec<(char, [u32; 2])> = match self {
            Self::Cells(characters) => characters
                .chars()
                .enumerate()
                .map(|(i, c)| (c, [index(i % columns), index(i / columns)]))
                .collect(),
            Self::Rows(rows) => rows
                .iter()
                .enumerate()
                .flat_map(|(row, characters)| {
                    let cells = characters.chars().enumerate();
                    cells.map(move |(column, c)| (c, [index(column), index(row)]))
                })
                .collect(),
        };
        cells.into_iter().filter(|&(c, _)| c != ' ').collect()
    }
}

/// A TrueType or OpenType font file, rasterized at a size.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorFontDef {
    /// The font file, by its path from the font file's folder.
    pub file: String,
    /// How many pixels its em is: the size a pixel font made as one is drawn at to fit its pixels.
    pub size: f32,
    /// Whether pixels its outlines partly cover are partly opaque, or each opaque or clear,
    /// whichever is nearer, so a pixel font stays crisp.
    pub smooth: bool,
}

/// Data for the game, by name.
pub type Props = BTreeMap<String, Value>;

/// A value in a scene's props: what a Luau table of plain data holds.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    /// Always finite.
    Number(f64),
    String(String),
    /// A table with keys from 1 up, or an empty one.
    List(Vec<Value>),
    /// A table with strings for keys.
    Map(BTreeMap<String, Value>),
}

impl SceneFile {
    /// Reads the scene file at `path`.
    pub fn read(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        Self::parse(&read_file(path)?, path)
    }

    /// Reads a scene from `source`, the contents of a scene file, naming it `path` in errors.
    ///
    /// The file runs as Luau with nothing in scope, so it can't call functions or read globals,
    /// and returns the scene. Anything it holds that isn't part of a scene, such as a field
    /// nodes don't have, is an error naming the node.
    pub fn parse(source: &str, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let error = |message: String| Error::Scene {
            path: path.to_owned(),
            message,
        };
        read_scene(evaluate(source, path).map_err(error)?).map_err(error)
    }
}

impl TilesetFile {
    /// Reads the tileset file at `path`.
    pub fn read(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        Self::parse(&read_file(path)?, path)
    }

    /// Reads a tileset from `source`, the contents of a tileset file, naming it `path` in
    /// errors, as [`SceneFile::parse`] reads a scene.
    pub fn parse(source: &str, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let error = |message: String| Error::Tileset {
            path: path.to_owned(),
            message,
        };
        read_tileset(evaluate(source, path).map_err(error)?).map_err(error)
    }
}

impl FontFile {
    /// Reads the font file at `path`.
    pub fn read(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        Self::parse(&read_file(path)?, path)
    }

    /// Reads a font from `source`, the contents of a font file, naming it `path` in errors, as
    /// [`SceneFile::parse`] reads a scene.
    pub fn parse(source: &str, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let error = |message: String| Error::Font {
            path: path.to_owned(),
            message,
        };
        read_font(evaluate(source, path).map_err(error)?).map_err(error)
    }
}

fn read_file(path: &Path) -> Result<String> {
    crate::files::read_to_string(path).map_err(|source| Error::ReadFile {
        path: path.to_owned(),
        source,
    })
}

/// Runs the Luau `source` with nothing in scope, and returns what it returns, which must be a
/// table, as plain data.
fn evaluate(source: &str, path: &Path) -> Result<Value, String> {
    let lua = file_lua()?;
    let value = run(&lua, source, path)?;
    if !value.is_table() {
        return Err(format!(
            "the file must return a table, not {}",
            describe(&value)
        ));
    }
    plain(value, "the file's table")
}

/// Runs the Luau `source` with nothing in scope, and returns what it returns as plain data,
/// whatever kind of value it is, as a save file holds.
pub(crate) fn evaluate_any(source: &str, path: &Path) -> Result<Value, String> {
    let lua = file_lua()?;
    let value = run(&lua, source, path)?;
    plain(value, "what the file returns")
}

/// A Luau VM to run a file in, which stops it if it runs too long or takes too much memory, as a
/// file that isn't plain values may.
fn file_lua() -> Result<mlua::Lua, String> {
    let lua = mlua::Lua::new();
    lua.set_memory_limit(MAX_MEMORY)
        .map_err(|err| err.to_string())?;
    let interrupts = std::sync::atomic::AtomicU32::new(0);
    lua.set_interrupt(move |_| {
        if interrupts.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < MAX_INTERRUPTS {
            Ok(mlua::VmState::Continue)
        } else {
            Err(mlua::Error::runtime(
                "it runs too long: a file holds values, and must not loop for long",
            ))
        }
    });
    Ok(lua)
}

/// Runs the Luau `source` with nothing in scope, and returns what it returns.
fn run(lua: &mlua::Lua, source: &str, path: &Path) -> Result<mlua::Value, String> {
    lua.load(source)
        .set_name(format!("@{}", path.display()))
        .set_mode(ChunkMode::Text)
        .set_environment(lua.create_table().map_err(|err| err.to_string())?)
        .eval()
        .map_err(|err| match err {
            mlua::Error::MemoryError(_) => format!(
                "it takes more than {} MiB of memory to run",
                MAX_MEMORY >> 20
            ),
            err => err.to_string(),
        })
}

/// `value` as plain data, which errors call `what`, such as "the file's table", and its fields by
/// their paths from it.
pub(crate) fn plain(value: mlua::Value, what: &str) -> Result<Value, String> {
    let mut values = 0;
    plain_at(value, what, "", 0, &mut values)
}

/// `value` as plain data. `at` is where it is in `root`, such as `children[2].props`, for errors.
/// `values` counts those read so far.
fn plain_at(
    value: mlua::Value,
    root: &str,
    at: &str,
    depth: usize,
    values: &mut usize,
) -> Result<Value, String> {
    let here = || {
        if at.is_empty() {
            root.to_owned()
        } else {
            format!("`{at}`")
        }
    };
    if depth >= MAX_DEPTH {
        return Err(format!(
            "{} nests tables more than {MAX_DEPTH} deep, too deep to read",
            here()
        ));
    }
    *values += 1;
    if *values > MAX_VALUES {
        return Err(format!(
            "{root} holds more than {MAX_VALUES} values, counting a table each time it's in \
             another: too many to read"
        ));
    }
    match value {
        mlua::Value::Boolean(value) => Ok(Value::Bool(value)),
        mlua::Value::Integer(number) => Ok(Value::Number(number as f64)),
        mlua::Value::Number(number) if number.is_finite() => Ok(Value::Number(number)),
        mlua::Value::Number(number) => {
            Err(format!("{} must be a finite number, not {number}", here()))
        }
        mlua::Value::String(string) => match string.to_str() {
            Ok(string) => Ok(Value::String(string.to_owned())),
            Err(_) => Err(format!("{} must be text in UTF-8", here())),
        },
        mlua::Value::Table(table) => {
            let mut strings = BTreeMap::new();
            let mut numbers = Vec::new();
            for pair in table.pairs::<mlua::Value, mlua::Value>() {
                let (key, value) = pair.map_err(|err| err.to_string())?;
                match key {
                    mlua::Value::String(key) => {
                        let Ok(key) = key.to_str() else {
                            return Err(format!("{} has a key that isn't UTF-8", here()));
                        };
                        let key = key.to_owned();
                        let at = field_path(at, &key);
                        let value = plain_at(value, root, &at, depth + 1, values)?;
                        strings.insert(key, value);
                    }
                    mlua::Value::Integer(_) | mlua::Value::Number(_) => {
                        let index = match key {
                            mlua::Value::Integer(index) => index as f64,
                            mlua::Value::Number(index) => index,
                            _ => unreachable!(),
                        };
                        let at = format!("{at}[{index}]");
                        let value = plain_at(value, root, &at, depth + 1, values)?;
                        numbers.push((index, value));
                    }
                    key => {
                        return Err(format!("{} has a key that's {}", here(), describe(&key)));
                    }
                }
            }
            if numbers.is_empty() {
                return Ok(if strings.is_empty() {
                    Value::List(Vec::new())
                } else {
                    Value::Map(strings)
                });
            }
            if !strings.is_empty() {
                return Err(format!(
                    "{} has both strings and numbers for keys: a table is a list or has strings \
                     for keys",
                    here()
                ));
            }
            numbers.sort_by(|a, b| a.0.total_cmp(&b.0));
            let is_list = numbers
                .iter()
                .enumerate()
                .all(|(i, (index, _))| *index == (i + 1) as f64);
            if !is_list {
                return Err(format!(
                    "{} has numbers for keys, but not 1, 2, 3 and so on: a table is a list or has \
                     strings for keys",
                    here()
                ));
            }
            Ok(Value::List(
                numbers.into_iter().map(|(_, value)| value).collect(),
            ))
        }
        value => Err(format!(
            "{} is {}, but files hold only booleans, numbers, strings and tables",
            here(),
            describe(&value)
        )),
    }
}

/// The path to the field `key` of the table at `at`, as Luau would write it.
fn field_path(at: &str, key: &str) -> String {
    match (at.is_empty(), is_identifier(key)) {
        (true, true) => key.to_owned(),
        (false, true) => format!("{at}.{key}"),
        (_, false) => format!("{at}[{key:?}]"),
    }
}

fn read_scene(scene: Value) -> Result<SceneFile, String> {
    let at_scene = |err: String| format!("the scene: {err}");
    let mut fields = Fields::new(scene, "a scene").map_err(at_scene)?;
    check_format(fields.take("format"), "scene")?;
    let sheets = read_sheets(fields.take("sheets"))?;
    let props = read_props(fields.take("props")).map_err(at_scene)?;
    let top = Parent {
        path: "",
        tilemap: None,
        in_3d: false,
    };
    let children = read_children(fields.take("children"), &top, &sheets, 0)?;
    let clips = read_clips(fields.take("clips"))?;
    fields.finish("the scene").map_err(at_scene)?;
    Ok(SceneFile {
        sheets,
        props,
        children,
        clips,
    })
}

fn read_clips(clips: Option<Value>) -> Result<BTreeMap<String, Clip>, String> {
    let Some(clips) = clips else {
        return Ok(BTreeMap::new());
    };
    let fields = Fields::new(clips, "`clips`").map_err(|err| format!("the scene: {err}"))?;
    fields
        .fields
        .into_iter()
        .map(|(name, clip)| {
            if name.is_empty() {
                return Err("the scene: a clip's name can't be empty".to_owned());
            }
            let clip = read_clip(clip).map_err(|err| format!("clip `{name}`: {err}"))?;
            Ok((name, clip))
        })
        .collect()
}

fn read_clip(clip: Value) -> Result<Clip, String> {
    let mut fields = Fields::new(clip, "a clip")?;
    let duration = fields.take("duration").ok_or("a clip has a `duration`")?;
    let duration = finite(duration, "duration")?;
    if duration <= 0.0 {
        return Err("`duration` must be above 0".into());
    }
    let looping = match fields.take("loop") {
        Some(looping) => boolean(looping, "loop")?,
        None => false,
    };
    let mut tracks: Vec<Track> = Vec::new();
    if let Some(defs) = fields.take("tracks") {
        for (i, def) in list(defs, "tracks")?.into_iter().enumerate() {
            let at = |err: String| format!("track {}: {err}", i + 1);
            let track = read_track(def, duration).map_err(at)?;
            let property = track.keys.property_name();
            if tracks
                .iter()
                .any(|other| other.node == track.node && other.keys.property_name() == property)
            {
                let node = if track.node.is_empty() {
                    "the node posed".to_owned()
                } else {
                    format!("`{}`", track.node)
                };
                return Err(at(format!(
                    "the clip has a track of the `{property}` of {node} already"
                )));
            }
            tracks.push(track);
        }
    }
    fields.finish("a clip")?;
    Ok(Clip {
        duration,
        looping,
        tracks,
    })
}

fn read_track(track: Value, duration: f32) -> Result<Track, String> {
    let mut fields = Fields::new(track, "a track")?;
    let node = match fields.take("node") {
        Some(node) => {
            let node = string(node, "node")?;
            if node.split('/').any(str::is_empty) {
                return Err(format!(
                    "`node` is a path of names separated by `/`, not `{node}`; leave it out for \
                     the node posed"
                ));
            }
            node
        }
        None => String::new(),
    };
    let property = fields.take("property").ok_or("a track has a `property`")?;
    let property = string(property, "property")?;
    let interpolation = match fields.take("interpolation") {
        Some(interpolation) => {
            let name = string(interpolation, "interpolation")?;
            Some(Interpolation::from_name(&name).ok_or_else(|| {
                format!(r#"`interpolation` must be "linear" or "step", not "{name}""#)
            })?)
        }
        None => None,
    };
    let keys = fields.take("keys").ok_or("a track has `keys`")?;
    let keys = read_keys(keys, duration)?;
    let keys = if let Some(property) = Property::from_name(&property) {
        Keys::Number {
            property,
            interpolation: interpolation.unwrap_or_default(),
            keys: keys
                .into_iter()
                .map(|(time, value)| Ok((time, finite(value, "keys")?)))
                .collect::<Result<_, String>>()?,
        }
    } else {
        if interpolation == Some(Interpolation::Linear) {
            return Err(format!(
                "`{property}` steps from key to key, so its `interpolation` can't be \"linear\""
            ));
        }
        match property.as_str() {
            "visible" => Keys::Visible(
                keys.into_iter()
                    .map(|(time, value)| Ok((time, boolean(value, "keys")?)))
                    .collect::<Result<_, String>>()?,
            ),
            "cell" => Keys::Cell(
                keys.into_iter()
                    .map(|(time, value)| Ok((time, cell(value)?)))
                    .collect::<Result<_, String>>()?,
            ),
            other => {
                return Err(format!(
                    "there's no property `{other}` for a track: it's x, y, rotation, scale_x, \
                     scale_y, visible or cell, or for nodes in 3D views, z, yaw, pitch, roll and \
                     scale_z"
                ));
            }
        }
    };
    fields.finish("a track")?;
    Ok(Track { node, keys })
}

/// A track's keys, a time and a value each, with their times checked: from 0 to the clip's
/// `duration`, each after the one before.
fn read_keys(keys: Value, duration: f32) -> Result<Vec<(f32, Value)>, String> {
    let keys = list(keys, "keys")?;
    if keys.is_empty() {
        return Err("a track has at least one key".into());
    }
    let mut read: Vec<(f32, Value)> = Vec::with_capacity(keys.len());
    for (i, key) in keys.into_iter().enumerate() {
        let at = |err: String| format!("key {}: {err}", i + 1);
        let Ok([time, value]) = <[Value; 2]>::try_from(list(key, "keys").map_err(at)?) else {
            return Err(at("a key is a time and a value, { time, value }".into()));
        };
        let time = finite(time, "keys").map_err(at)?;
        if !(0.0..=duration).contains(&time) {
            return Err(at(format!(
                "its time must be from 0 to the clip's duration, {duration}, not {time}"
            )));
        }
        if let Some(&(before, _)) = read.last()
            && time <= before
        {
            return Err(at(format!(
                "its time, {time}, must be after the time of the key before, {before}"
            )));
        }
        read.push((time, value));
    }
    Ok(read)
}

fn read_tileset(tileset: Value) -> Result<TilesetFile, String> {
    let at_tileset = |err: String| format!("the tileset: {err}");
    let mut fields = Fields::new(tileset, "a tileset").map_err(at_tileset)?;
    check_format(fields.take("format"), "tileset")?;
    let mut size = |field: &str| match fields.take(field) {
        Some(value) => positive_whole(value, field),
        None => Err(format!("a tileset has a `{field}`")),
    };
    let tile_width = size("tile_width").map_err(at_tileset)?;
    let tile_height = size("tile_height").map_err(at_tileset)?;
    let sheets = read_sheets(fields.take("sheets"))?;
    let mut tiles = BTreeMap::new();
    if let Some(defs) = fields.take("tiles") {
        let defs = Fields::new(defs, "`tiles`").map_err(at_tileset)?;
        for (name, def) in defs.fields {
            let at = |err: String| format!("tile `{name}`: {err}");
            if name.is_empty() {
                return Err(at_tileset("a tile's name can't be empty".into()));
            }
            let tile = read_tile(def, &sheets).map_err(at)?;
            tiles.insert(name, tile);
        }
    }
    fields.finish("the tileset").map_err(at_tileset)?;
    Ok(TilesetFile {
        tile_width,
        tile_height,
        sheets,
        tiles,
    })
}

fn read_tile(def: Value, sheets: &BTreeMap<String, SheetDef>) -> Result<TileDef, String> {
    let mut fields = Fields::new(def, "a tile")?;
    let (sheet, cell) = read_sheet_cell(&mut fields, sheets, "a tile")?;
    let tint = optional_color(&mut fields, "tint")?;
    let solid = match fields.take("solid") {
        Some(solid) => boolean(solid, "solid")?,
        None => false,
    };
    let props = read_props(fields.take("props"))?;
    fields.finish("a tile")?;
    Ok(TileDef {
        sheet,
        cell,
        tint,
        solid,
        props,
    })
}

fn read_font(font: Value) -> Result<FontFile, String> {
    let mut fields = Fields::new(font, "a font")?;
    check_format(fields.take("format"), "font")?;
    let font = match (fields.take("image"), fields.take("file")) {
        (Some(image), None) => FontFile::Bitmap(read_bitmap_font(image, &mut fields)?),
        (None, Some(file)) => {
            let file = string(file, "file")?;
            let size = fields
                .take("size")
                .ok_or("a font with a `file` has a `size`, how many pixels its em is")?;
            let size = positive(finite(size, "size")?, "size")?;
            let smooth = match fields.take("smooth") {
                Some(smooth) => boolean(smooth, "smooth")?,
                None => true,
            };
            FontFile::Vector(VectorFontDef { file, size, smooth })
        }
        (Some(_), Some(_)) => {
            return Err("a font has an `image` of its glyphs or a font `file`, not both".into());
        }
        (None, None) => {
            return Err(
                "a font has an `image` of its glyphs, or a TrueType or OpenType `file`".into(),
            );
        }
    };
    fields.finish("a font")?;
    Ok(font)
}

/// A bitmap font's fields, but its `image`.
fn read_bitmap_font(image: Value, fields: &mut Fields) -> Result<BitmapFontDef, String> {
    let image = string(image, "image")?;
    let [width, height] = match fields.take("cell") {
        Some(value) => match <[Value; 2]>::try_from(list(value, "cell")?) {
            Ok([width, height]) => [
                positive_whole(width, "cell")?,
                positive_whole(height, "cell")?,
            ],
            Err(_) => return Err("`cell` must be a width and a height, { width, height }".into()),
        },
        None => return Err("a font with an `image` has a `cell`, { width, height }".into()),
    };
    let characters = match fields.take("characters") {
        Some(Value::String(characters)) => FontCharacters::Cells(characters),
        Some(Value::List(rows)) => FontCharacters::Rows(
            rows.into_iter()
                .map(|row| string(row, "characters"))
                .collect::<Result<_, _>>()?,
        ),
        Some(value) => {
            return Err(format!(
                "`characters` must be a string, or a list of them, one for each row of cells, not {}",
                describe_value(&value)
            ));
        }
        None => return Err("a font with an `image` has its `characters`".into()),
    };
    let mut seen = HashSet::new();
    for (c, _) in characters.cells(u32::MAX) {
        if c.is_control() {
            return Err(format!(
                "`characters` can't hold control characters, such as {}",
                c.escape_default()
            ));
        }
        if !seen.insert(c) {
            return Err(format!("`characters` has `{c}` twice"));
        }
    }
    let monospace = match fields.take("monospace") {
        Some(monospace) => boolean(monospace, "monospace")?,
        None => false,
    };
    let spacing = match fields.take("spacing") {
        Some(spacing) => whole(spacing, "spacing")?,
        None => 1,
    };
    let space = match fields.take("space") {
        Some(space) => whole(space, "space")?,
        None if monospace => width.saturating_add(spacing),
        None => width.div_ceil(2),
    };
    let line_height = match fields.take("line_height") {
        Some(line_height) => positive_whole(line_height, "line_height")?,
        None => height.saturating_add(1),
    };
    Ok(BitmapFontDef {
        image,
        cell: [width, height],
        characters,
        spacing,
        space,
        line_height,
        monospace,
    })
}

fn check_format(format: Option<Value>, what: &str) -> Result<(), String> {
    let Some(format) = format else {
        return Err(format!(
            "the {what} has no `format`; this engine reads formats 1 to {FORMAT}"
        ));
    };
    match whole(format, "format").map_err(|err| format!("the {what}: {err}"))? {
        1..=FORMAT => Ok(()),
        newer if newer > FORMAT => Err(format!(
            "the {what} is format {newer}, newer than this engine reads ({FORMAT})"
        )),
        other => Err(format!("there's no {what} format {other}")),
    }
}

fn read_sheets(sheets: Option<Value>) -> Result<BTreeMap<String, SheetDef>, String> {
    let Some(sheets) = sheets else {
        return Ok(BTreeMap::new());
    };
    let fields = Fields::new(sheets, "`sheets`").map_err(|err| format!("the file: {err}"))?;
    fields
        .fields
        .into_iter()
        .map(|(name, sheet)| {
            let at = |err: String| format!("sheet `{name}`: {err}");
            let mut fields = Fields::new(sheet, "a sheet").map_err(at)?;
            let mut take = |field: &str| {
                fields
                    .take(field)
                    .ok_or_else(|| at(format!("a sheet has a `{field}`")))
            };
            let path = string(take("path")?, "path").map_err(at)?;
            let tile_width = positive_whole(take("tile_width")?, "tile_width").map_err(at)?;
            let tile_height = positive_whole(take("tile_height")?, "tile_height").map_err(at)?;
            fields.finish("a sheet").map_err(at)?;
            let sheet = SheetDef {
                path,
                tile_width,
                tile_height,
            };
            Ok((name, sheet))
        })
        .collect()
}

/// What the nodes being read are inside: the path of the node, or `""` for the scene, the
/// columns and rows of the map if it's a tilemap, whose layers they may be, and whether it's a 3D
/// view or a node placed in 3D, so they're placed in 3D too.
#[derive(Clone, Copy)]
pub(super) struct Parent<'a> {
    pub path: &'a str,
    pub tilemap: Option<[u32; 2]>,
    pub in_3d: bool,
}

impl Parent<'_> {
    fn describe(&self) -> String {
        if self.path.is_empty() {
            "the scene".to_owned()
        } else {
            format!("node `{}`", self.path)
        }
    }

    /// The path of its child `name`.
    pub(super) fn child_path(&self, name: &str) -> String {
        if self.path.is_empty() {
            name.to_owned()
        } else {
            format!("{}/{name}", self.path)
        }
    }
}

/// The nodes of a list, the children of `parent`.
fn read_children(
    children: Option<Value>,
    parent: &Parent,
    sheets: &BTreeMap<String, SheetDef>,
    depth: usize,
) -> Result<Vec<NodeDef>, String> {
    let Some(children) = children else {
        return Ok(Vec::new());
    };
    let owner = parent.describe();
    if depth >= MAX_NODE_DEPTH {
        return Err(format!(
            "{owner}: nodes nest more than {MAX_NODE_DEPTH} deep, too deep to read"
        ));
    }
    let list = list(children, "children").map_err(|err| format!("{owner}: {err}"))?;
    let mut nodes: Vec<NodeDef> = Vec::with_capacity(list.len());
    for (i, child) in list.into_iter().enumerate() {
        let node = read_node(child, parent, i + 1, sheets, depth)?;
        if nodes.iter().any(|sibling| sibling.name == node.name) {
            return Err(format!(
                "{owner}: two of its children are named `{}`, but siblings' names are unique",
                node.name
            ));
        }
        nodes.push(node);
    }
    Ok(nodes)
}

/// The node `node`, child `number` of `parent`, counting from 1, and its children.
pub(super) fn read_node(
    node: Value,
    parent: &Parent,
    number: usize,
    sheets: &BTreeMap<String, SheetDef>,
    depth: usize,
) -> Result<NodeDef, String> {
    let unnamed = |err: String| format!("{}'s child {number}: {err}", parent.describe());
    let mut fields = Fields::new(node, "a node").map_err(unnamed)?;
    let name = fields
        .take("name")
        .ok_or_else(|| unnamed("a node has a `name`".into()))?;
    let name = string(name, "name").map_err(unnamed)?;
    check_name(&name).map_err(unnamed)?;
    let path = parent.child_path(&name);
    let at = |err: String| format!("node `{path}`: {err}");

    let kind = fields
        .take("kind")
        .ok_or_else(|| at("a node has a `kind`".into()))?;
    let kind_name = string(kind, "kind").map_err(at)?;
    let mut node = NodeDef::new(name, KindDef::Node);
    node.placement3 = read_placement3(&mut fields, &kind_name, parent).map_err(at)?;
    let mut number = |field: &str, default: f32| match fields.take(field) {
        Some(value) => finite(value, field),
        None => Ok(default),
    };
    node.x = number("x", 0.0).map_err(at)?;
    node.y = number("y", 0.0).map_err(at)?;
    node.rotation = number("rotation", 0.0).map_err(at)?;
    node.scale_x = number("scale_x", 1.0).map_err(at)?;
    node.scale_y = number("scale_y", 1.0).map_err(at)?;
    if let Some(anchor) = fields.take("anchor") {
        node.anchor = Some(pair(anchor, "anchor").map_err(at)?);
    }
    if let Some(visible) = fields.take("visible") {
        node.visible = boolean(visible, "visible").map_err(at)?;
    }
    if let Some(tags) = fields.take("tags") {
        node.tags = list(tags, "tags")
            .and_then(|tags| tags.into_iter().map(|tag| string(tag, "tags")).collect())
            .map_err(at)?;
    }
    node.props = read_props(fields.take("props")).map_err(at)?;
    node.kind = read_kind(&kind_name, &mut fields, parent, sheets).map_err(at)?;
    check_placement(&node).map_err(at)?;
    let tilemap = match &node.kind {
        KindDef::Tilemap(map) => Some([map.columns, map.rows]),
        _ => None,
    };
    let inside = Parent {
        path: &path,
        tilemap,
        in_3d: matches!(node.kind, KindDef::View3d(_)) || node.placement3.is_some(),
    };
    node.children = read_children(fields.take("children"), &inside, sheets, depth + 1)?;
    fields.finish(&super::a_node_of(&kind_name)).map_err(at)?;
    Ok(node)
}

/// Where a node of the kind `kind`, inside `parent`, is placed in 3D, if it is: if it's of a kind
/// only views hold, if its parent is placed in 3D or is a view, or if it has a field only nodes
/// placed in 3D have. Such a node has no `rotation` or `anchor`.
fn read_placement3(
    fields: &mut Fields,
    kind: &str,
    parent: &Parent,
) -> Result<Option<Placement3>, String> {
    const FIELDS: [&str; 5] = ["z", "yaw", "pitch", "roll", "scale_z"];
    if !can_be_3d(kind) {
        return Ok(None);
    }
    let given = FIELDS.map(|field| fields.take(field));
    if !(is_3d_kind(kind) || parent.in_3d || given.iter().any(Option::is_some)) {
        return Ok(None);
    }
    // A rotation of 0 is no rotation, as a plain node's fields have it before an override places
    // it in 3D.
    if fields.fields.get("rotation") == Some(&Value::Number(0.0)) {
        fields.take("rotation");
    }
    for (field, says) in [
        (
            "rotation",
            "turns by `yaw`, `pitch` and `roll`, and has no `rotation`",
        ),
        (
            "anchor",
            "has no `anchor`, which places nodes in the 2D scene",
        ),
    ] {
        if fields.has(field) {
            return Err(format!("a node placed in 3D, in a 3D view, {says}"));
        }
    }
    let default = Placement3::default();
    let defaults = [
        default.z,
        default.yaw,
        default.pitch,
        default.roll,
        default.scale_z,
    ];
    let mut numbers = [0.0; 5];
    for (i, value) in given.into_iter().enumerate() {
        numbers[i] = match value {
            Some(value) => finite(value, FIELDS[i])?,
            None => defaults[i],
        };
    }
    let [z, yaw, pitch, roll, scale_z] = numbers;
    Ok(Some(Placement3 {
        z,
        yaw,
        pitch,
        roll,
        scale_z,
    }))
}

fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("a node's `name` can't be empty".into());
    }
    if name.contains('/') {
        return Err(format!(
            "`{name}` can't be a node's name: `/` separates the names in a path"
        ));
    }
    Ok(())
}

/// Fails if the node is placed in a way its kind can't be: a tilemap only moves, and a tile
/// layer is where its tilemap is.
fn check_placement(node: &NodeDef) -> Result<(), String> {
    let turned = node.rotation != 0.0 || node.scale_x != 1.0 || node.scale_y != 1.0;
    let moved = node.x != 0.0 || node.y != 0.0 || node.anchor.is_some();
    match node.kind {
        KindDef::Tilemap(_) if turned => Err(
            "a tilemap moves, but doesn't turn or stretch: it has no `rotation`, `scale_x` \
                 or `scale_y`"
                .into(),
        ),
        KindDef::View3d(_) if turned => Err(
            "a 3D view moves, but doesn't turn or stretch: it has no `rotation`, `scale_x` or \
             `scale_y`"
                .into(),
        ),
        KindDef::TileLayer(_) if turned || moved => Err(
            "a tile layer is where its tilemap is: it has no `x`, `y`, `anchor`, `rotation`, \
             `scale_x` or `scale_y`"
                .into(),
        ),
        _ => Ok(()),
    }
}

/// What a node of the kind `kind` holds, from the fields it has left.
fn read_kind(
    kind: &str,
    fields: &mut Fields,
    parent: &Parent,
    sheets: &BTreeMap<String, SheetDef>,
) -> Result<KindDef, String> {
    let kind = match kind {
        "node" => KindDef::Node,
        "sprite" => {
            let (sheet, cell) = read_sheet_cell(fields, sheets, "a sprite node")?;
            let size = match (fields.take("width"), fields.take("height")) {
                (Some(width), Some(height)) => {
                    Some([finite(width, "width")?, finite(height, "height")?])
                }
                (None, None) => None,
                _ => {
                    return Err("a sprite with a `width` has a `height`, and the other way".into());
                }
            };
            KindDef::Sprite(SpriteDef {
                sheet,
                cell,
                size,
                stretch: read_stretch(fields)?,
                pivot: read_pivot(fields)?,
                tint: optional_color(fields, "tint")?,
                blend: read_blend(fields)?,
            })
        }
        "rect" => KindDef::Rect(RectDef {
            size: read_size(fields, "a rect node")?,
            stretch: read_stretch(fields)?,
            pivot: read_pivot(fields)?,
            color: optional_color(fields, "color")?,
            blend: read_blend(fields)?,
        }),
        "text" => {
            let text = fields.take("text").ok_or("a text node has a `text`")?;
            let font = match fields.take("font") {
                Some(font) => Some(string(font, "font")?),
                None => None,
            };
            let font_scale = match fields.take("font_scale") {
                Some(scale) => finite(scale, "font_scale")?,
                None => 1.0,
            };
            if font_scale <= 0.0 {
                return Err("`font_scale` must be above 0".into());
            }
            let wrap = match fields.take("wrap") {
                Some(wrap) => Some(finite(wrap, "wrap")?),
                None => None,
            };
            let align = match fields.take("align") {
                Some(align) => {
                    let name = string(align, "align")?;
                    Align::from_name(&name).ok_or_else(|| {
                        format!(r#"`align` must be "left", "center" or "right", not "{name}""#)
                    })?
                }
                None => Align::Left,
            };
            KindDef::Text(TextDef {
                text: string(text, "text")?,
                font,
                font_scale,
                wrap,
                align,
                pivot: read_pivot(fields)?,
                color: optional_color(fields, "color")?,
                blend: read_blend(fields)?,
            })
        }
        "area" => KindDef::Area(AreaDef {
            size: read_size(fields, "an area node")?,
            stretch: read_stretch(fields)?,
            pivot: read_pivot(fields)?,
        }),
        "tilemap" => {
            let mut take = |field: &str| {
                fields
                    .take(field)
                    .ok_or_else(|| format!("a tilemap node has a `{field}`"))
            };
            let tileset = string(take("tileset")?, "tileset")?;
            let columns = positive_whole(take("columns")?, "columns")?;
            let rows = positive_whole(take("rows")?, "rows")?;
            KindDef::Tilemap(TilemapDef {
                tileset,
                columns,
                rows,
            })
        }
        "tile_layer" => {
            let Some(size) = parent.tilemap else {
                return Err("a tile layer must be the child of a tilemap".into());
            };
            KindDef::TileLayer(read_tile_layer(fields, size)?)
        }
        "instance" => {
            let scene = fields
                .take("scene")
                .ok_or("an instance node has a `scene`")?;
            let mut overrides = BTreeMap::new();
            if let Some(changes) = fields.take("overrides") {
                for (path, change) in Fields::new(changes, "`overrides`")?.fields {
                    let at = |err: String| format!("`overrides[{path:?}]`: {err}");
                    let change = Fields::new(change, "an override").map_err(at)?.fields;
                    for field in ["name", "kind", "children"] {
                        if change.contains_key(field) {
                            return Err(at(format!("a node's `{field}` can't be overridden")));
                        }
                    }
                    overrides.insert(path, change);
                }
            }
            KindDef::Instance(InstanceDef {
                scene: string(scene, "scene")?,
                overrides,
            })
        }
        "view3d" => {
            let size = read_size(fields, "a view3d node")?;
            let stretch = read_stretch(fields)?;
            let pivot = read_pivot(fields)?;
            let camera = fields
                .take("camera")
                .ok_or("a view3d node has a `camera`, { position, target }")?;
            let lighting = match fields.take("lighting") {
                Some(lighting) => read_lighting(lighting)?,
                None => Lighting::default(),
            };
            KindDef::View3d(View3dDef {
                size,
                stretch,
                pivot,
                camera: read_camera(camera)?,
                lighting,
            })
        }
        "mesh" => {
            let shape = fields
                .take("shape")
                .ok_or(r#"a mesh node has a `shape`: "cuboid", "plane" or "sphere""#)?;
            let shape = match string(shape, "shape")?.as_str() {
                "cuboid" => Shape::Cuboid(match fields.take("size") {
                    Some(size) => positive_triple(size, "size")?,
                    None => [1.0; 3],
                }),
                "plane" => Shape::Plane(match fields.take("size") {
                    Some(size) => {
                        let [width, depth] = pair(size, "size")?;
                        [positive(width, "size")?, positive(depth, "size")?]
                    }
                    None => [1.0; 2],
                }),
                "sphere" => Shape::Sphere(match fields.take("radius") {
                    Some(radius) => positive(finite(radius, "radius")?, "radius")?,
                    None => 0.5,
                }),
                other => {
                    return Err(format!(
                        r#"`shape` must be "cuboid", "plane" or "sphere", not "{other}""#
                    ));
                }
            };
            let sprite = if fields.has("sheet") || fields.has("cell") {
                let (sheet, cell) = read_sheet_cell(fields, sheets, "a mesh node with a sprite")?;
                Some(CellDef { sheet, cell })
            } else {
                None
            };
            KindDef::Mesh(MeshDef {
                shape,
                sprite,
                tint: optional_color(fields, "tint")?,
            })
        }
        "model" => {
            let model = fields
                .take("model")
                .ok_or("a model node has a `model`, the path of a model file")?;
            KindDef::Model(ModelDef {
                model: string(model, "model")?,
                tint: optional_color(fields, "tint")?,
            })
        }
        "billboard" => {
            let (sheet, cell) = read_sheet_cell(fields, sheets, "a billboard node")?;
            let mut number = |field: &str| match fields.take(field) {
                Some(value) => finite(value, field),
                None => Ok(1.0),
            };
            let size = [number("width")?, number("height")?];
            KindDef::Billboard(BillboardDef {
                sheet,
                cell,
                size,
                tint: optional_color(fields, "tint")?,
            })
        }
        "light" => {
            let default = LightDef::default();
            let mut number = |field: &str, default: f32| match fields.take(field) {
                Some(value) => finite(value, field),
                None => Ok(default),
            };
            let range = number("range", default.range)?;
            let energy = number("energy", default.energy)?;
            let edge = number("edge", default.edge)?;
            let cone = match fields.take("cone") {
                Some(cone) => Some(finite(cone, "cone")?),
                None => None,
            };
            let light = LightDef {
                color: optional_color(fields, "color")?,
                range,
                energy,
                cone,
                edge,
            };
            if let Some(problem) = light.problem() {
                return Err(problem);
            }
            KindDef::Light(light)
        }
        "stack" => KindDef::Stack(read_stack(fields, "a stack")?),
        "panel" => {
            let stack = read_stack(fields, "a panel")?;
            let look = if fields.has("sheet") || fields.has("cell") {
                for field in ["color", "border", "border_color"] {
                    if fields.has(field) {
                        return Err(format!(
                            "a panel showing a sprite has no `{field}`: the sprite is how it looks"
                        ));
                    }
                }
                let (sheet, cell) = read_sheet_cell(fields, sheets, "a panel showing a sprite")?;
                let slice = match fields.take("slice") {
                    Some(slice) => edges(slice, "slice")?,
                    None => [0.0; 4],
                };
                if slice.iter().any(|&texels| texels < 0.0) {
                    return Err("`slice` can't be less than 0".into());
                }
                PanelLookDef::Sprite {
                    sprite: CellDef { sheet, cell },
                    slice,
                    tint: optional_color(fields, "tint")?,
                }
            } else {
                for field in ["slice", "tint"] {
                    if fields.has(field) {
                        return Err(format!(
                            "a panel has a `{field}` only if it shows a sprite, with a `sheet` and \
                             a `cell`"
                        ));
                    }
                }
                let border = match fields.take("border") {
                    Some(border) => finite(border, "border")?,
                    None => 0.0,
                };
                if border < 0.0 {
                    return Err("`border` can't be less than 0".into());
                }
                PanelLookDef::Fill(PanelFill {
                    color: optional_color(fields, "color")?,
                    border,
                    border_color: optional_color(fields, "border_color")?,
                })
            };
            KindDef::Panel(PanelDef { stack, look })
        }
        "particles" => {
            let sprite = if fields.has("sheet") || fields.has("cell") {
                let (sheet, cell) =
                    read_sheet_cell(fields, sheets, "a particles node with a sprite")?;
                Some(CellDef { sheet, cell })
            } else {
                None
            };
            let mut number = |field: &str| match fields.take(field) {
                Some(value) => finite(value, field),
                None => Ok(0.0),
            };
            let size = [number("width")?, number("height")?];
            if size.iter().any(|&side| side < 0.0) {
                return Err("a particles node's `width` and `height` can't be less than 0".into());
            }
            KindDef::Particles(ParticlesDef {
                sprite,
                size,
                stretch: read_stretch(fields)?,
                pivot: read_pivot(fields)?,
                settings: read_particle_settings(fields, &ParticleSettings::default())?,
            })
        }
        other => {
            return Err(format!(
                "there's no kind of node `{other}`: a node is a node, sprite, rect, text, area, \
                 stack, panel, particles, tilemap, tile_layer, instance, view3d, mesh, model, \
                 billboard or light"
            ));
        }
    };
    Ok(kind)
}

/// A drawing node's `blend`, normal without one.
fn read_blend(fields: &mut Fields) -> Result<Blend, String> {
    let Some(blend) = fields.take("blend") else {
        return Ok(Blend::Normal);
    };
    let name = string(blend, "blend")?;
    Blend::from_name(&name).ok_or_else(|| {
        format!(r#"`blend` must be "normal", "add", "multiply" or "screen", not "{name}""#)
    })
}

/// The settings of particles among `fields`, with those that aren't there as `base` has them.
fn read_particle_settings(
    fields: &mut Fields,
    base: &ParticleSettings,
) -> Result<ParticleSettings, String> {
    let mut settings = base.clone();
    if let Some(emitting) = fields.take("emitting") {
        settings.emitting = boolean(emitting, "emitting")?;
    }
    let mut number = |field: &str, value: &mut f32| -> Result<(), String> {
        if let Some(given) = fields.take(field) {
            *value = finite(given, field)?;
        }
        Ok(())
    };
    number("rate", &mut settings.rate)?;
    number("warmup", &mut settings.warmup)?;
    number("direction", &mut settings.direction)?;
    number("spread", &mut settings.spread)?;
    number("drag", &mut settings.drag)?;
    if let Some(limit) = fields.take("limit") {
        settings.limit = positive_whole(limit, "limit")?;
    }
    if let Some(seed) = fields.take("seed") {
        settings.seed = Some(whole(seed, "seed")?);
    }
    for (field, range) in [
        ("lifetime", &mut settings.lifetime),
        ("speed", &mut settings.speed),
        ("angle", &mut settings.angle),
        ("spin", &mut settings.spin),
    ] {
        if let Some(value) = fields.take(field) {
            *range = self::range(value, field)?;
        }
    }
    if let Some(gravity) = fields.take("gravity") {
        settings.gravity = pair(gravity, "gravity")?;
    }
    if let Some(colors) = fields.take("colors") {
        settings.colors = list(colors, "colors")?
            .into_iter()
            .map(|value| color(value, "colors"))
            .collect::<Result<_, _>>()?;
    }
    if let Some(scales) = fields.take("scales") {
        settings.scales = list(scales, "scales")?
            .into_iter()
            .map(|value| finite(value, "scales"))
            .collect::<Result<_, _>>()?;
    }
    if fields.has("blend") {
        settings.blend = read_blend(fields)?;
    }
    if let Some(attached) = fields.take("attached") {
        settings.attached = boolean(attached, "attached")?;
    }
    match settings.problem() {
        Some(problem) => Err(problem),
        None => Ok(settings),
    }
}

/// A range, `{ least, most }`, or a number, which is both.
fn range(value: Value, field: &str) -> Result<[f32; 2], String> {
    match value {
        Value::Number(_) => {
            let number = finite(value, field)?;
            Ok([number, number])
        }
        value => match <[Value; 2]>::try_from(list(value, field)?) {
            Ok([least, most]) => Ok([finite(least, field)?, finite(most, field)?]),
            Err(_) => Err(format!(
                "`{field}` must be a number, or two, {{ least, most }}"
            )),
        },
    }
}

/// Particles' settings as `base` has them, with the fields `change` sets, as a scene file
/// holds them: for scripts, which make and change settings with the same fields, checked the
/// same way.
pub(crate) fn particle_settings(
    change: Value,
    base: &ParticleSettings,
) -> Result<ParticleSettings, String> {
    let mut fields = Fields::new(change, "a particle emitter's settings")?;
    let settings = read_particle_settings(&mut fields, base)?;
    fields.finish("a particle emitter")?;
    Ok(settings)
}

/// Particles' settings as a scene file holds them, with every field, for scripts to read.
pub(crate) fn particle_settings_value(settings: &ParticleSettings) -> Value {
    let mut fields = BTreeMap::new();
    particle_fields(settings, &mut |key: &str, value| {
        fields.insert(key.to_owned(), value);
    });
    Value::Map(fields)
}

/// Particles' settings, as a scene file holds them.
fn particle_fields(settings: &ParticleSettings, set: &mut impl FnMut(&str, Value)) {
    let number = |n: f32| Value::Number(n.into());
    let range = |[least, most]: [f32; 2]| {
        if least == most {
            number(least)
        } else {
            Value::List(vec![number(least), number(most)])
        }
    };
    set("emitting", Value::Bool(settings.emitting));
    set("rate", number(settings.rate));
    set("limit", Value::Number(settings.limit.into()));
    set("warmup", number(settings.warmup));
    if let Some(seed) = settings.seed {
        set("seed", Value::Number(seed.into()));
    }
    set("lifetime", range(settings.lifetime));
    set("speed", range(settings.speed));
    set("direction", number(settings.direction));
    set("spread", number(settings.spread));
    let [x, y] = settings.gravity;
    set("gravity", Value::List(vec![number(x), number(y)]));
    set("drag", number(settings.drag));
    set("angle", range(settings.angle));
    set("spin", range(settings.spin));
    set(
        "scales",
        Value::List(settings.scales.iter().map(|&s| number(s)).collect()),
    );
    set(
        "colors",
        Value::List(settings.colors.iter().map(|&c| hex_value(c)).collect()),
    );
    set("blend", Value::String(settings.blend.name().into()));
    set("attached", Value::Bool(settings.attached));
}

/// The fields of a stack or panel, `what`, that size, place and lay it out.
fn read_stack(fields: &mut Fields, what: &str) -> Result<Stack, String> {
    let mut length = |field: &str| match fields.take(field) {
        None | Some(Value::Bool(false)) => Ok(None),
        Some(value) => finite(value, field).map(Some),
    };
    let size = [length("width")?, length("height")?];
    let stretch = read_stretch(fields)?;
    for (i, field) in ["width", "height"].into_iter().enumerate() {
        if size[i].is_none() && stretch[i] != 0.0 {
            return Err(format!(
                "{what} without a `{field}` fits what it holds, so it can't stretch with its \
                 parent that way"
            ));
        }
    }
    let direction = match fields.take("direction") {
        Some(direction) => {
            let name = string(direction, "direction")?;
            StackDirection::from_name(&name)
                .ok_or_else(|| format!(r#"`direction` must be "column" or "row", not "{name}""#))?
        }
        None => StackDirection::Column,
    };
    let gap = match fields.take("gap") {
        Some(gap) => finite(gap, "gap")?,
        None => 0.0,
    };
    let padding = match fields.take("padding") {
        Some(padding) => edges(padding, "padding")?,
        None => [0.0; 4],
    };
    Ok(Stack {
        size,
        stretch,
        pivot: read_pivot(fields)?,
        direction,
        gap,
        padding,
    })
}

/// Numbers for the left, top, right and bottom edges of something: one for them all, or a list
/// of four, `{ left, top, right, bottom }`.
fn edges(value: Value, field: &str) -> Result<[f32; 4], String> {
    if let Value::Number(_) = value {
        return Ok([finite(value, field)?; 4]);
    }
    let shape = || format!("`{field}` must be a number, or four, {{ left, top, right, bottom }}");
    let Value::List(items) = value else {
        return Err(shape());
    };
    match <[Value; 4]>::try_from(items) {
        Ok([left, top, right, bottom]) => Ok([
            finite(left, field)?,
            finite(top, field)?,
            finite(right, field)?,
            finite(bottom, field)?,
        ]),
        Err(_) => Err(shape()),
    }
}

/// A view's camera, `{ position, target }` with `up`, `fov` or `height`, `near` and `far` if
/// they aren't the defaults.
fn read_camera(camera: Value) -> Result<Camera3d, String> {
    let mut fields = Fields::new(camera, "`camera`")?;
    let mut take = |field: &str| {
        fields
            .take(field)
            .ok_or_else(|| format!("a camera has a `{field}`"))
    };
    let position = triple(take("position")?, "camera.position")?;
    let target = triple(take("target")?, "camera.target")?;
    let mut camera = Camera3d::perspective(position, target, DEFAULT_FOV);
    if let Some(up) = fields.take("up") {
        camera.up = triple(up, "camera.up")?;
    }
    camera.projection = match (fields.take("fov"), fields.take("height")) {
        (Some(_), Some(_)) => {
            return Err(
                "a camera has a `fov`, for perspective, or a `height`, for none, not both".into(),
            );
        }
        (Some(fov), None) => Projection::Perspective {
            fov_y: finite(fov, "camera.fov")?,
        },
        (None, Some(height)) => Projection::Orthographic {
            height: finite(height, "camera.height")?,
        },
        (None, None) => camera.projection,
    };
    if let Some(near) = fields.take("near") {
        camera.near = finite(near, "camera.near")?;
    }
    if let Some(far) = fields.take("far") {
        camera.far = finite(far, "camera.far")?;
    }
    fields.finish("a camera")?;
    match camera.problem() {
        Some(problem) => Err(problem),
        None => Ok(camera),
    }
}

/// A view's lighting, `{ direction, color, ambient }`, each the default's if left out.
fn read_lighting(lighting: Value) -> Result<Lighting, String> {
    let mut fields = Fields::new(lighting, "`lighting`")?;
    let default = Lighting::default();
    let direction = match fields.take("direction") {
        Some(direction) => triple(direction, "lighting.direction")?,
        None => default.direction,
    };
    // A color the same as the default's, to the byte, is the default's, so it writes and reads
    // back the same.
    let mut light = |field: &str, default: Color| -> Result<Color, String> {
        let Some(value) = fields.take(field) else {
            return Ok(default);
        };
        let read = color(value, &format!("lighting.{field}"))?;
        Ok(if read.to_rgba8() == default.to_rgba8() {
            default
        } else {
            read
        })
    };
    let color = light("color", default.color)?;
    let ambient = light("ambient", default.ambient)?;
    fields.finish("`lighting`")?;
    Ok(Lighting {
        direction,
        color,
        ambient,
    })
}

/// A sprite's `sheet` and `cell`, which the sprite of a node or a tile must have.
fn read_sheet_cell(
    fields: &mut Fields,
    sheets: &BTreeMap<String, SheetDef>,
    what: &str,
) -> Result<(String, [u32; 2]), String> {
    let sheet = fields
        .take("sheet")
        .ok_or_else(|| format!("{what} has a `sheet`"))?;
    let sheet = string(sheet, "sheet")?;
    if !sheets.contains_key(&sheet) {
        return Err(format!("there's no sheet `{sheet}` in the file's `sheets`"));
    }
    let cell = fields
        .take("cell")
        .ok_or_else(|| format!("{what} has a `cell`"))?;
    Ok((sheet, self::cell(cell)?))
}

/// A sprite's column and row in its sheet, `{ column, row }`.
fn cell(value: Value) -> Result<[u32; 2], String> {
    match <[Value; 2]>::try_from(list(value, "cell")?) {
        Ok([column, row]) => Ok([whole(column, "cell")?, whole(row, "cell")?]),
        Err(_) => Err("`cell` must be a column and a row, { column, row }".into()),
    }
}

/// A tile layer's `legend` and `tiles`, which must have `columns` characters in each of `rows`
/// strings.
fn read_tile_layer(fields: &mut Fields, [columns, rows]: [u32; 2]) -> Result<TileLayerDef, String> {
    let mut legend = BTreeMap::new();
    if let Some(entries) = fields.take("legend") {
        for (key, tile) in Fields::new(entries, "`legend`")?.fields {
            let mut chars = key.chars();
            let (Some(symbol), None) = (chars.next(), chars.next()) else {
                return Err(format!(
                    "`legend` has `{key}`, but each of its keys is a single character"
                ));
            };
            if symbol == ' ' {
                return Err("a space is an empty cell, so it can't be in the `legend`".into());
            }
            legend.insert(symbol, string(tile, "legend")?);
        }
    }
    let tiles = fields
        .take("tiles")
        .ok_or("a tile layer has `tiles`, a string for each row")?;
    let tiles: Vec<String> = list(tiles, "tiles")?
        .into_iter()
        .map(|row| string(row, "tiles"))
        .collect::<Result<_, _>>()?;
    if tiles.len() != rows as usize {
        return Err(format!(
            "`tiles` has {} rows, but its tilemap has {rows}",
            tiles.len()
        ));
    }
    for (row, line) in tiles.iter().enumerate() {
        let width = line.chars().count();
        if width != columns as usize {
            return Err(format!(
                "row {row} of `tiles` is {width} cells wide, but its tilemap is {columns}"
            ));
        }
        if let Some(symbol) = line.chars().find(|c| *c != ' ' && !legend.contains_key(c)) {
            return Err(format!(
                "row {row} of `tiles` has `{symbol}`, which isn't in the `legend`"
            ));
        }
    }
    Ok(TileLayerDef {
        legend,
        tiles,
        blend: read_blend(fields)?,
    })
}

/// A rect's or an area's `width` and `height`, which they must have.
fn read_size(fields: &mut Fields, what: &str) -> Result<[f32; 2], String> {
    let mut take = |field: &str| match fields.take(field) {
        Some(value) => finite(value, field),
        None => Err(format!("{what} has a `{field}`")),
    };
    Ok([take("width")?, take("height")?])
}

fn read_pivot(fields: &mut Fields) -> Result<[f32; 2], String> {
    match fields.take("pivot") {
        Some(pivot) => pair(pivot, "pivot"),
        None => Ok([0.0, 0.0]),
    }
}

fn read_stretch(fields: &mut Fields) -> Result<[f32; 2], String> {
    match fields.take("stretch") {
        Some(stretch) => pair(stretch, "stretch"),
        None => Ok([0.0, 0.0]),
    }
}

/// The color in `field`, or white without one.
fn optional_color(fields: &mut Fields, field: &str) -> Result<Color, String> {
    match fields.take(field) {
        Some(value) => color(value, field),
        None => Ok(Color::WHITE),
    }
}

/// A color: a hex string, a table `{ r, g, b, a }`, or a number `0xRRGGBBAA`.
fn color(value: Value, field: &str) -> Result<Color, String> {
    let color = match &value {
        Value::String(hex) => parse_hex(hex).ok_or_else(|| {
            format!(r##"`{field}`: `{hex}` is not a color like "#f2c14e" or "#00000080""##)
        })?,
        Value::Number(number) => unpacked_color(*number)
            .ok_or_else(|| format!("`{field}`: {number} is not a color like 0xf2c14eff"))?,
        Value::Map(_) => {
            let mut fields = Fields::new(value, &format!("`{field}`"))?;
            let mut component =
                |name: &str, default: Option<f64>| match (fields.take(name), default) {
                    (Some(value), _) => number(value, &format!("{field}.{name}")),
                    (None, Some(default)) => Ok(default),
                    (None, None) => Err(format!("`{field}` has `r`, `g` and `b`")),
                };
            let [r, g, b] = [
                component("r", None)?,
                component("g", None)?,
                component("b", None)?,
            ];
            let a = component("a", Some(1.0))?;
            fields.finish(&format!("`{field}`"))?;
            Color::rgba(r as f32, g as f32, b as f32, a as f32)
        }
        _ => {
            return Err(format!(
                r##"`{field}`: expected a hex string such as "#f2c14e", a table {{ r, g, b, a }}, or a number such as 0xf2c14eff, not {}"##,
                describe_value(&value)
            ));
        }
    };
    // Colors are kept as bytes, as they're written, so writing one and reading it back gives
    // the same color.
    let [r, g, b, a] = color.to_rgba8();
    Ok(Color::rgba8(r, g, b, a))
}

fn read_props(props: Option<Value>) -> Result<Props, String> {
    match props {
        Some(props) => Ok(Fields::new(props, "`props`")?.fields),
        None => Ok(Props::new()),
    }
}

/// A table's fields by name, taken one by one as they're read, so whatever's left over is a
/// field the table shouldn't have.
pub(super) struct Fields {
    pub fields: BTreeMap<String, Value>,
}

impl Fields {
    /// The fields of `value`, which must be a table with strings for keys, such as `"a node"`.
    pub fn new(value: Value, what: &str) -> Result<Self, String> {
        match value {
            Value::Map(fields) => Ok(Self { fields }),
            Value::List(items) if items.is_empty() => Ok(Self {
                fields: BTreeMap::new(),
            }),
            value => Err(format!(
                "{what} must be a table of named fields, not {}",
                describe_value(&value)
            )),
        }
    }

    fn take(&mut self, name: &str) -> Option<Value> {
        self.fields.remove(name)
    }

    fn has(&self, name: &str) -> bool {
        self.fields.contains_key(name)
    }

    /// Fails if any field hasn't been taken, as one `what`, such as `"a sheet"`, doesn't have.
    fn finish(self, what: &str) -> Result<(), String> {
        match self.fields.keys().next() {
            Some(field) => Err(format!("{what} has no field `{field}`")),
            None => Ok(()),
        }
    }
}

/// A list's items.
fn list(value: Value, field: &str) -> Result<Vec<Value>, String> {
    match value {
        Value::List(items) => Ok(items),
        value => Err(format!(
            "`{field}` must be a list, not {}",
            describe_value(&value)
        )),
    }
}

fn number(value: Value, field: &str) -> Result<f64, String> {
    match value {
        Value::Number(number) => Ok(number),
        value => Err(format!(
            "`{field}` must be a number, not {}",
            describe_value(&value)
        )),
    }
}

/// A finite number, as `f32`, for positions and sizes.
fn finite(value: Value, field: &str) -> Result<f32, String> {
    let number = number(value, field)?;
    let single = number as f32;
    if !single.is_finite() {
        return Err(format!("`{field}` must be a finite number, not {number}"));
    }
    Ok(single)
}

/// A whole number from 0 up.
fn whole(value: Value, field: &str) -> Result<u32, String> {
    let number = number(value, field)?;
    if number.fract() != 0.0 || !(0.0..=f64::from(u32::MAX)).contains(&number) {
        return Err(format!(
            "`{field}` must be a whole number from 0 up, not {number}"
        ));
    }
    Ok(number as u32)
}

fn positive_whole(value: Value, field: &str) -> Result<u32, String> {
    match whole(value, field)? {
        0 => Err(format!("`{field}` must be at least 1")),
        number => Ok(number),
    }
}

/// A pair of finite numbers, `{ x, y }`.
fn pair(value: Value, field: &str) -> Result<[f32; 2], String> {
    match <[Value; 2]>::try_from(list(value, field)?) {
        Ok([x, y]) => Ok([finite(x, field)?, finite(y, field)?]),
        Err(_) => Err(format!("`{field}` must be two numbers, {{ x, y }}")),
    }
}

/// Three finite numbers, `{ x, y, z }`.
fn triple(value: Value, field: &str) -> Result<[f32; 3], String> {
    match <[Value; 3]>::try_from(list(value, field)?) {
        Ok([x, y, z]) => Ok([finite(x, field)?, finite(y, field)?, finite(z, field)?]),
        Err(_) => Err(format!("`{field}` must be three numbers, {{ x, y, z }}")),
    }
}

/// Three numbers above 0, such as a box's width, height and depth.
fn positive_triple(value: Value, field: &str) -> Result<[f32; 3], String> {
    let [x, y, z] = triple(value, field)?;
    Ok([
        positive(x, field)?,
        positive(y, field)?,
        positive(z, field)?,
    ])
}

/// `number`, a size, which must be above 0.
fn positive(number: f32, field: &str) -> Result<f32, String> {
    if number > 0.0 {
        Ok(number)
    } else {
        Err(format!("`{field}` must be above 0, not {number}"))
    }
}

fn string(value: Value, field: &str) -> Result<String, String> {
    match value {
        Value::String(string) => Ok(string),
        value => Err(format!(
            "`{field}` must be a string, not {}",
            describe_value(&value)
        )),
    }
}

fn boolean(value: Value, field: &str) -> Result<bool, String> {
    match value {
        Value::Bool(value) => Ok(value),
        value => Err(format!(
            "`{field}` must be true or false, not {}",
            describe_value(&value)
        )),
    }
}

/// What kind of value `value` is, with an article, such as "a string".
fn describe(value: &mlua::Value) -> String {
    match value {
        mlua::Value::Nil => "nil".into(),
        mlua::Value::Integer(_) | mlua::Value::Number(_) => "a number".into(),
        value => {
            let name = value.type_name();
            let article = if name.starts_with(['a', 'e', 'i', 'o', 'u']) {
                "an"
            } else {
                "a"
            };
            format!("{article} {name}")
        }
    }
}

fn describe_value(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::List(_) | Value::Map(_) => "a table",
    }
}

/// The node's own fields, as a scene file holds them, leaving out its children and fields at
/// their defaults: what an override changes, before the node is read again.
pub(super) fn node_fields(node: &NodeDef) -> BTreeMap<String, Value> {
    let mut fields = BTreeMap::new();
    let mut set = |key: &str, value: Value| {
        fields.insert(key.to_owned(), value);
    };
    let number = |n: f32| Value::Number(n.into());
    let pair = |[x, y]: [f32; 2]| Value::List(vec![number(x), number(y)]);
    let string = |s: &str| Value::String(s.to_owned());
    let hex = |color: Color| {
        let [r, g, b, a] = color.to_rgba8();
        Value::String(format!("#{r:02x}{g:02x}{b:02x}{a:02x}"))
    };
    set("name", string(&node.name));
    set("kind", string(node.kind.name()));
    set("x", number(node.x));
    set("y", number(node.y));
    if let Some(anchor) = node.anchor {
        set("anchor", pair(anchor));
    }
    // A node placed in 3D has no rotation, and its 3D fields, all of them, say it's placed in 3D
    // when it's read again.
    match node.placement3 {
        Some(placement) => {
            set("z", number(placement.z));
            set("yaw", number(placement.yaw));
            set("pitch", number(placement.pitch));
            set("roll", number(placement.roll));
            set("scale_z", number(placement.scale_z));
        }
        None => set("rotation", number(node.rotation)),
    }
    set("scale_x", number(node.scale_x));
    set("scale_y", number(node.scale_y));
    set("visible", Value::Bool(node.visible));
    set(
        "tags",
        Value::List(node.tags.iter().map(|tag| string(tag)).collect()),
    );
    set("props", Value::Map(node.props.clone()));
    match &node.kind {
        KindDef::Node => {}
        KindDef::Sprite(sprite) => {
            set("sheet", string(&sprite.sheet));
            let [column, row] = sprite.cell;
            set(
                "cell",
                Value::List(vec![
                    Value::Number(column.into()),
                    Value::Number(row.into()),
                ]),
            );
            if let Some([width, height]) = sprite.size {
                set("width", number(width));
                set("height", number(height));
            }
            set("stretch", pair(sprite.stretch));
            set("pivot", pair(sprite.pivot));
            set("tint", hex(sprite.tint));
            set("blend", string(sprite.blend.name()));
        }
        KindDef::Rect(rect) => {
            set("width", number(rect.size[0]));
            set("height", number(rect.size[1]));
            set("stretch", pair(rect.stretch));
            set("pivot", pair(rect.pivot));
            set("color", hex(rect.color));
            set("blend", string(rect.blend.name()));
        }
        KindDef::Text(text) => {
            set("text", string(&text.text));
            if let Some(font) = &text.font {
                set("font", string(font));
            }
            set("font_scale", number(text.font_scale));
            if let Some(wrap) = text.wrap {
                set("wrap", number(wrap));
            }
            set("align", string(text.align.name()));
            set("pivot", pair(text.pivot));
            set("color", hex(text.color));
            set("blend", string(text.blend.name()));
        }
        KindDef::Area(area) => {
            set("width", number(area.size[0]));
            set("height", number(area.size[1]));
            set("stretch", pair(area.stretch));
            set("pivot", pair(area.pivot));
        }
        KindDef::Tilemap(map) => {
            set("tileset", string(&map.tileset));
            set("columns", Value::Number(map.columns.into()));
            set("rows", Value::Number(map.rows.into()));
        }
        KindDef::TileLayer(layer) => {
            let legend = layer
                .legend
                .iter()
                .map(|(symbol, tile)| (symbol.to_string(), string(tile)))
                .collect();
            set("legend", Value::Map(legend));
            set(
                "tiles",
                Value::List(layer.tiles.iter().map(|row| string(row)).collect()),
            );
            set("blend", string(layer.blend.name()));
        }
        KindDef::Instance(instance) => {
            set("scene", string(&instance.scene));
            let overrides = instance
                .overrides
                .iter()
                .map(|(path, change)| (path.clone(), Value::Map(change.clone())))
                .collect();
            set("overrides", Value::Map(overrides));
        }
        KindDef::View3d(view) => {
            set("width", number(view.size[0]));
            set("height", number(view.size[1]));
            set("stretch", pair(view.stretch));
            set("pivot", pair(view.pivot));
            set("camera", camera_value(&view.camera));
            set("lighting", lighting_value(&view.lighting));
        }
        KindDef::Mesh(mesh) => {
            set("shape", string(mesh.shape.name()));
            match mesh.shape {
                Shape::Cuboid(size) => set("size", triple_value(size)),
                Shape::Plane(size) => set("size", pair(size)),
                Shape::Sphere(radius) => set("radius", number(radius)),
            }
            if let Some(sprite) = &mesh.sprite {
                set("sheet", string(&sprite.sheet));
                set("cell", cell_value(sprite.cell));
            }
            set("tint", hex(mesh.tint));
        }
        KindDef::Model(model) => {
            set("model", string(&model.model));
            set("tint", hex(model.tint));
        }
        KindDef::Billboard(billboard) => {
            set("sheet", string(&billboard.sheet));
            set("cell", cell_value(billboard.cell));
            set("width", number(billboard.size[0]));
            set("height", number(billboard.size[1]));
            set("tint", hex(billboard.tint));
        }
        KindDef::Light(light) => {
            set("color", hex(light.color));
            set("range", number(light.range));
            set("energy", number(light.energy));
            if let Some(cone) = light.cone {
                set("cone", number(cone));
            }
            set("edge", number(light.edge));
        }
        KindDef::Stack(stack) => stack_fields(stack, &mut set),
        KindDef::Panel(panel) => {
            stack_fields(&panel.stack, &mut set);
            match &panel.look {
                PanelLookDef::Fill(fill) => {
                    set("color", hex(fill.color));
                    set("border", number(fill.border));
                    set("border_color", hex(fill.border_color));
                }
                PanelLookDef::Sprite {
                    sprite,
                    slice,
                    tint,
                } => {
                    set("sheet", string(&sprite.sheet));
                    set("cell", cell_value(sprite.cell));
                    set("slice", edges_value(*slice));
                    set("tint", hex(*tint));
                }
            }
        }
        KindDef::Particles(particles) => {
            if let Some(sprite) = &particles.sprite {
                set("sheet", string(&sprite.sheet));
                set("cell", cell_value(sprite.cell));
            }
            set("width", number(particles.size[0]));
            set("height", number(particles.size[1]));
            set("stretch", pair(particles.stretch));
            set("pivot", pair(particles.pivot));
            particle_fields(&particles.settings, &mut set);
        }
    }
    fields
}

/// The fields of a stack or panel that size, place and lay it out, as a scene file holds them.
fn stack_fields(stack: &Stack, set: &mut impl FnMut(&str, Value)) {
    let number = |n: f32| Value::Number(n.into());
    for (i, field) in ["width", "height"].into_iter().enumerate() {
        // An explicit fitting value lets overrides replace a size inherited from the scene.
        set(field, stack.size[i].map_or(Value::Bool(false), number));
    }
    let [x, y] = stack.stretch;
    set("stretch", Value::List(vec![number(x), number(y)]));
    let [x, y] = stack.pivot;
    set("pivot", Value::List(vec![number(x), number(y)]));
    set(
        "direction",
        Value::String(stack.direction.name().to_owned()),
    );
    set("gap", number(stack.gap));
    set("padding", edges_value(stack.padding));
}

fn edges_value(edges: [f32; 4]) -> Value {
    Value::List(edges.map(|n| Value::Number(n.into())).into())
}

fn triple_value(numbers: [f32; 3]) -> Value {
    Value::List(numbers.map(|n| Value::Number(n.into())).into())
}

fn cell_value([column, row]: [u32; 2]) -> Value {
    Value::List(vec![
        Value::Number(column.into()),
        Value::Number(row.into()),
    ])
}

fn hex_value(color: Color) -> Value {
    let [r, g, b, a] = color.to_rgba8();
    Value::String(format!("#{r:02x}{g:02x}{b:02x}{a:02x}"))
}

/// A camera as a scene file holds it, with every field.
fn camera_value(camera: &Camera3d) -> Value {
    let number = |n: f32| Value::Number(n.into());
    let mut fields = BTreeMap::new();
    fields.insert("position".to_owned(), triple_value(camera.position));
    fields.insert("target".to_owned(), triple_value(camera.target));
    fields.insert("up".to_owned(), triple_value(camera.up));
    match camera.projection {
        Projection::Perspective { fov_y } => fields.insert("fov".to_owned(), number(fov_y)),
        Projection::Orthographic { height } => fields.insert("height".to_owned(), number(height)),
    };
    fields.insert("near".to_owned(), number(camera.near));
    fields.insert("far".to_owned(), number(camera.far));
    Value::Map(fields)
}

/// Lighting as a scene file holds it, with every field.
fn lighting_value(lighting: &Lighting) -> Value {
    Value::Map(
        [
            ("direction".to_owned(), triple_value(lighting.direction)),
            ("color".to_owned(), hex_value(lighting.color)),
            ("ambient".to_owned(), hex_value(lighting.ambient)),
        ]
        .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Result<SceneFile> {
        SceneFile::parse(source, "test.scene.luau")
    }

    /// The message of the error reading `source` gives.
    fn error(source: &str) -> String {
        parse(source).unwrap_err().to_string()
    }

    /// A scene of `children`, a list of nodes in Luau.
    fn with_children(children: &str) -> String {
        format!(
            "return {{ format = 1, sheets = {{ tiles = {{ path = 'tiles.png', tile_width = 16, \
             tile_height = 16 }} }}, children = {{ {children} }} }}"
        )
    }

    #[test]
    fn reads_clips_of_tracks() {
        let scene = parse(
            r#"return {
                format = 1,
                children = { { name = "arm", kind = "node" } },
                clips = {
                    wave = {
                        duration = 1.5,
                        loop = true,
                        tracks = {
                            { node = "arm", property = "rotation", keys = { { 0, 0.5 }, { 0.75, -0.5 }, { 1.5, 0.5 } } },
                            { property = "y", interpolation = "step", keys = { { 0, 1 } } },
                            { node = "arm/hand", property = "visible", keys = { { 0.5, false } } },
                            { node = "arm/hand", property = "cell", interpolation = "step", keys = { { 0, { 2, 1 } } } },
                        },
                    },
                    rest = { duration = 1 },
                },
            }"#,
        )
        .unwrap();
        let wave = &scene.clips["wave"];
        assert_eq!(wave.duration, 1.5);
        assert!(wave.looping);
        assert_eq!(
            wave.tracks,
            [
                Track {
                    node: "arm".into(),
                    keys: Keys::Number {
                        property: Property::Rotation,
                        interpolation: Interpolation::Linear,
                        keys: vec![(0.0, 0.5), (0.75, -0.5), (1.5, 0.5)],
                    },
                },
                Track {
                    node: String::new(),
                    keys: Keys::Number {
                        property: Property::Y,
                        interpolation: Interpolation::Step,
                        keys: vec![(0.0, 1.0)],
                    },
                },
                Track {
                    node: "arm/hand".into(),
                    keys: Keys::Visible(vec![(0.5, false)]),
                },
                Track {
                    node: "arm/hand".into(),
                    keys: Keys::Cell(vec![(0.0, [2, 1])]),
                },
            ]
        );
        let rest = &scene.clips["rest"];
        assert!(!rest.looping);
        assert!(rest.tracks.is_empty());
    }

    #[test]
    fn clips_that_cannot_pose_are_errors() {
        let clip =
            |fields: &str| format!("return {{ format = 1, clips = {{ walk = {{ {fields} }} }} }}");
        let track = |fields: &str| clip(&format!("duration = 1, tracks = {{ {{ {fields} }} }}"));
        let cases = [
            (clip("loop = true"), "clip `walk`: a clip has a `duration`"),
            (
                clip("duration = 0"),
                "clip `walk`: `duration` must be above 0",
            ),
            (
                clip("duration = 1, speed = 2"),
                "clip `walk`: a clip has no field `speed`",
            ),
            (
                track("property = 'x', keys = { { 0, 1 } }, ease = 1"),
                "clip `walk`: track 1: a track has no field `ease`",
            ),
            (
                track("keys = { { 0, 1 } }"),
                "track 1: a track has a `property`",
            ),
            (track("property = 'x'"), "track 1: a track has `keys`"),
            (
                track("property = 'x', keys = {}"),
                "a track has at least one key",
            ),
            (
                track("property = 'tint', keys = { { 0, 1 } }"),
                "there's no property `tint` for a track",
            ),
            (
                track("property = 'x', interpolation = 'smooth', keys = { { 0, 1 } }"),
                r#"`interpolation` must be "linear" or "step", not "smooth""#,
            ),
            (
                track("property = 'visible', interpolation = 'linear', keys = { { 0, true } }"),
                "`visible` steps from key to key",
            ),
            (
                track("property = 'x', keys = { 0, 1 }"),
                "key 1: `keys` must be a list",
            ),
            (
                track("property = 'x', keys = { { 0, 1, 2 } }"),
                "key 1: a key is a time and a value",
            ),
            (
                track("property = 'x', keys = { { 0, 1 }, { 1.5, 2 } }"),
                "key 2: its time must be from 0 to the clip's duration, 1, not 1.5",
            ),
            (
                track("property = 'x', keys = { { 0.5, 1 }, { 0.5, 2 } }"),
                "key 2: its time, 0.5, must be after the time of the key before, 0.5",
            ),
            (
                track("property = 'x', keys = { { 0, true } }"),
                "`keys` must be a number, not a boolean",
            ),
            (
                track("property = 'visible', keys = { { 0, 1 } }"),
                "`keys` must be true or false, not a number",
            ),
            (
                track("property = 'cell', keys = { { 0, 1 } }"),
                "`cell` must be a list, not a number",
            ),
            (
                track("node = 'arm//hand', property = 'x', keys = { { 0, 1 } }"),
                "`node` is a path of names separated by `/`",
            ),
            (
                clip(
                    "duration = 1, tracks = { { property = 'x', keys = { { 0, 1 } } }, \
                     { property = 'x', keys = { { 0, 2 } } } }",
                ),
                "track 2: the clip has a track of the `x` of the node posed already",
            ),
        ];
        for (source, expected) in cases {
            let err = error(&source);
            assert!(err.contains(expected), "{source}\n{err}");
        }
    }

    #[test]
    fn reads_every_kind_of_node() {
        let scene = parse(
            r##"return {
                format = 1,
                sheets = { tiles = { path = "tiles.png", tile_width = 16, tile_height = 8 } },
                props = { music = "garden.ogg", calm = true },
                children = {
                    {
                        name = "mill", kind = "sprite", x = 10, y = -4.5, rotation = 0.25,
                        scale_x = -1, sheet = "tiles", cell = { 3, 20 }, pivot = { 8, 16 },
                        tint = "#d8b070", tags = { "building", "solid" },
                        children = {
                            { name = "sails", kind = "node", visible = false },
                        },
                    },
                    {
                        name = "pond", kind = "rect", anchor = { 0.5, 1 }, width = 48, height = 32,
                        stretch = { 1, 0 }, color = "#21406a80", blend = "multiply",
                    },
                    { name = "title", kind = "text", text = "Hi", font = "fonts/title.font.luau", font_scale = 2, wrap = 100, align = "center" },
                    { name = "gate", kind = "area", width = 16, height = 8, props = { to = "town" } },
                    {
                        name = "map", kind = "tilemap", x = 8, tileset = "world.tileset.luau",
                        columns = 3, rows = 2,
                        children = {
                            { name = "ground", kind = "tile_layer", legend = { ["."] = "grass", T = "tree" }, tiles = { "..T", "T. " }, blend = "add" },
                            { name = "people", kind = "node" },
                        },
                    },
                    {
                        name = "elder", kind = "instance", scene = "villager.scene.luau",
                        overrides = { body = { tint = "#c8b8e8", props = { talk = "elder" } } },
                    },
                },
            }"##,
        )
        .unwrap();
        assert_eq!(
            scene.sheets["tiles"],
            SheetDef {
                path: "tiles.png".into(),
                tile_width: 16,
                tile_height: 8
            }
        );
        assert_eq!(scene.props["music"], Value::String("garden.ogg".into()));
        assert_eq!(scene.props["calm"], Value::Bool(true));

        let mill = &scene.children[0];
        assert_eq!(mill.name, "mill");
        assert_eq!([mill.x, mill.y, mill.rotation], [10.0, -4.5, 0.25]);
        assert_eq!([mill.scale_x, mill.scale_y], [-1.0, 1.0]);
        assert_eq!(mill.tags, ["building", "solid"]);
        assert_eq!(mill.anchor, None);
        assert_eq!(
            mill.kind,
            KindDef::Sprite(SpriteDef {
                sheet: "tiles".into(),
                cell: [3, 20],
                size: None,
                stretch: [0.0, 0.0],
                pivot: [8.0, 16.0],
                tint: Color::rgb8(0xd8, 0xb0, 0x70),
                blend: Blend::Normal,
            })
        );
        let sails = &mill.children[0];
        assert_eq!(sails.kind, KindDef::Node);
        assert!(!sails.visible);

        let pond = &scene.children[1];
        assert_eq!(pond.anchor, Some([0.5, 1.0]));
        assert_eq!(
            pond.kind,
            KindDef::Rect(RectDef {
                size: [48.0, 32.0],
                stretch: [1.0, 0.0],
                pivot: [0.0, 0.0],
                color: Color::rgba8(0x21, 0x40, 0x6a, 0x80),
                blend: Blend::Multiply,
            })
        );
        assert_eq!(
            scene.children[2].kind,
            KindDef::Text(TextDef {
                text: "Hi".into(),
                font: Some("fonts/title.font.luau".into()),
                font_scale: 2.0,
                wrap: Some(100.0),
                align: Align::Center,
                pivot: [0.0, 0.0],
                color: Color::WHITE,
                blend: Blend::Normal,
            })
        );
        let gate = &scene.children[3];
        assert_eq!(
            gate.kind,
            KindDef::Area(AreaDef {
                size: [16.0, 8.0],
                stretch: [0.0, 0.0],
                pivot: [0.0, 0.0],
            })
        );
        assert_eq!(gate.props["to"], Value::String("town".into()));

        let map = &scene.children[4];
        assert_eq!(
            map.kind,
            KindDef::Tilemap(TilemapDef {
                tileset: "world.tileset.luau".into(),
                columns: 3,
                rows: 2,
            })
        );
        assert_eq!(
            map.children[0].kind,
            KindDef::TileLayer(TileLayerDef {
                legend: [('.', "grass".to_owned()), ('T', "tree".to_owned())].into(),
                tiles: vec!["..T".into(), "T. ".into()],
                blend: Blend::Add,
            })
        );
        let KindDef::Instance(elder) = &scene.children[5].kind else {
            panic!("{:?}", scene.children[5].kind);
        };
        assert_eq!(elder.scene, "villager.scene.luau");
        assert_eq!(
            elder.overrides["body"]["tint"],
            Value::String("#c8b8e8".into())
        );
    }

    #[test]
    fn reads_particles_and_their_settings() {
        let scene = parse(
            r##"return {
                format = 4,
                sheets = { fx = { path = "fx.png", tile_width = 8, tile_height = 8 } },
                children = {
                    { name = "dust", kind = "particles" },
                    {
                        name = "fire", kind = "particles", sheet = "fx", cell = { 1, 0 },
                        width = 12, height = 2, pivot = { 6, 1 }, emitting = false, rate = 40,
                        limit = 80, warmup = 1.5, seed = 9, lifetime = { 0.5, 1 }, speed = 30,
                        direction = -1.5, spread = 0.5, gravity = { 0, -20 }, drag = 2,
                        angle = { 0, 3 }, spin = { -1, 1 }, scales = { 1, 2, 0 },
                        colors = { "#ffd080", "#ff602000" }, blend = "add", attached = true,
                    },
                },
            }"##,
        )
        .unwrap();
        assert_eq!(
            scene.children[0].kind,
            KindDef::Particles(ParticlesDef::new(ParticleSettings::default()))
        );
        let KindDef::Particles(fire) = &scene.children[1].kind else {
            panic!("{:?}", scene.children[1].kind);
        };
        assert_eq!(
            fire.sprite,
            Some(CellDef {
                sheet: "fx".into(),
                cell: [1, 0]
            })
        );
        assert_eq!([fire.size, fire.pivot], [[12.0, 2.0], [6.0, 1.0]]);
        assert_eq!(
            fire.settings,
            ParticleSettings {
                emitting: false,
                rate: 40.0,
                limit: 80,
                warmup: 1.5,
                seed: Some(9),
                lifetime: [0.5, 1.0],
                speed: [30.0, 30.0],
                direction: -1.5,
                spread: 0.5,
                gravity: [0.0, -20.0],
                drag: 2.0,
                angle: [0.0, 3.0],
                spin: [-1.0, 1.0],
                colors: vec![
                    Color::rgb8(0xff, 0xd0, 0x80),
                    Color::rgba8(0xff, 0x60, 0x20, 0)
                ],
                scales: vec![1.0, 2.0, 0.0],
                blend: Blend::Add,
                attached: true,
            }
        );
        // Written back and read again, the same.
        let again = parse(&scene.to_luau().unwrap()).unwrap();
        assert_eq!(again, scene);
    }

    #[test]
    fn particles_settings_are_checked_in_files_and_from_scripts() {
        let node = |fields: &str| {
            format!(
                "return {{ format = 4, children = {{ {{ name = 'p', kind = 'particles', {fields} }} }} }}"
            )
        };
        let cases = [
            ("rate = -2", "`rate` must be 0 or more, not -2"),
            ("lifetime = { 2, 1 }", "its least no more than its most"),
            (
                "lifetime = { 1, 2, 3 }",
                "`lifetime` must be a number, or two",
            ),
            ("colors = {}", "`colors` must have at least one color"),
            ("limit = 0", "`limit` must be at least 1"),
            ("blend = 'alpha'", r#"`blend` must be "normal", "add""#),
            ("width = -1", "`width` and `height` can't be less than 0"),
            (
                "cell = { 0, 0 }",
                "a particles node with a sprite has a `sheet`",
            ),
            ("colour = 'red'", "a particles node has no field `colour`"),
        ];
        for (fields, expected) in cases {
            let source = node(fields);
            let err = parse(&source).unwrap_err().to_string();
            assert!(err.contains(expected), "{source}\n{err}");
        }

        let mut change = BTreeMap::new();
        change.insert("rate".to_owned(), Value::Number(5.0));
        let base = ParticleSettings {
            drag: 1.0,
            ..ParticleSettings::default()
        };
        let changed = particle_settings(Value::Map(change.clone()), &base).unwrap();
        assert_eq!([changed.rate, changed.drag], [5.0, 1.0]);
        change.insert("colour".to_owned(), Value::String("red".into()));
        assert_eq!(
            particle_settings(Value::Map(change), &base).unwrap_err(),
            "a particle emitter has no field `colour`"
        );
        let Value::Map(all) = particle_settings_value(&base) else {
            panic!("settings are a table");
        };
        assert_eq!(
            particle_settings(Value::Map(all), &ParticleSettings::default()),
            Ok(base)
        );
    }

    #[test]
    fn props_hold_lists_and_tables() {
        let scene = parse(
            "return { format = 1, props = {
                route = { 'left', 2, true },
                nested = { a = { 1, 2 }, b = {} },
            } }",
        )
        .unwrap();
        assert_eq!(
            scene.props["route"],
            Value::List(vec![
                Value::String("left".into()),
                Value::Number(2.0),
                Value::Bool(true),
            ])
        );
        let Value::Map(nested) = &scene.props["nested"] else {
            panic!("{:?}", scene.props["nested"]);
        };
        assert_eq!(
            nested["a"],
            Value::List(vec![Value::Number(1.0), Value::Number(2.0)])
        );
        assert_eq!(
            nested["b"],
            Value::List(Vec::new()),
            "empty tables are lists"
        );
    }

    #[test]
    fn overriding_a_node_sets_its_fields_and_props_one_by_one() {
        let scene = parse(
            "return { format = 1,
                sheets = { tiles = { path = 't.png', tile_width = 8, tile_height = 8 },
                           more = { path = 'm.png', tile_width = 8, tile_height = 8 } },
                children = { { name = 'hat', kind = 'sprite', sheet = 'tiles', cell = { 1, 2 },
                               props = { size = 1, style = 'straw' },
                               children = { { name = 'band', kind = 'node' } } } } }",
        )
        .unwrap();
        let hat = &scene.children[0];
        let change: BTreeMap<String, Value> = [
            ("x".to_owned(), Value::Number(4.0)),
            ("sheet".to_owned(), Value::String("more".into())),
            (
                "props".to_owned(),
                Value::Map([("size".to_owned(), Value::Number(2.0))].into()),
            ),
        ]
        .into();
        let changed = hat.overridden(&change, &scene.sheets, None).unwrap();
        assert_eq!(changed.x, 4.0);
        assert!(matches!(&changed.kind, KindDef::Sprite(sprite) if sprite.sheet == "more"));
        assert_eq!(changed.props["size"], Value::Number(2.0));
        assert_eq!(changed.props["style"], Value::String("straw".into()));
        assert_eq!(changed.children, hat.children);
        // Its fields read back as it is.
        assert_eq!(
            hat.overridden(&BTreeMap::new(), &scene.sheets, None)
                .unwrap(),
            *hat
        );

        let bad = |field: &str, value: Value| {
            let change = [(field.to_owned(), value)].into();
            hat.overridden(&change, &scene.sheets, None).unwrap_err()
        };
        assert!(bad("sheet", Value::String("none".into())).contains("none"));
        assert!(bad("name", Value::String("cap".into())).contains("can't be overridden"));
    }

    #[test]
    fn files_cant_reach_the_engine() {
        let err = error("return { format = 1, props = { n = math.floor(2.5) } }");
        assert!(err.contains("test.scene.luau"), "{err}");
        let err = error("return { format = 1, props = { f = function() end } }");
        assert!(err.contains("`props.f` is a function"), "{err}");
        let err = error("return 3");
        assert!(err.contains("must return a table"), "{err}");
    }

    #[test]
    fn the_format_is_required_and_checked() {
        let err = error("return {}");
        assert!(err.contains("has no `format`"), "{err}");
        let err = error("return { format = 5 }");
        assert!(
            err.contains("format 5, newer than this engine reads (4)"),
            "{err}"
        );
        let err = error("return { format = 0 }");
        assert!(err.contains("there's no scene format 0"), "{err}");
        assert!(
            parse("return { format = 1 }").is_ok(),
            "older files still read"
        );
        assert!(parse("return { format = 2 }").is_ok());
        assert!(parse("return { format = 3 }").is_ok());
        assert!(parse("return { format = 4 }").is_ok());
    }

    #[test]
    fn stacks_and_panels_read_their_layout_and_look() {
        let scene = parse(
            "return { format = 3, sheets = { ui = { path = 'ui.png', tile_width = 5, tile_height = 5 } },
            children = {
                { name = 'menu', kind = 'stack', direction = 'row', gap = 2, padding = 3,
                  width = 40, stretch = { 0.5, 0 } },
                { name = 'box', kind = 'panel', padding = { 1, 2, 3, 4 }, border = 1,
                  color = '#14182c' },
                { name = 'frame', kind = 'panel', sheet = 'ui', cell = { 0, 0 },
                  slice = { 2, 2, 2, 3 } },
            } }",
        )
        .unwrap();
        let menu = scene.children[0].kind.stack().unwrap();
        assert_eq!(menu.size, [Some(40.0), None]);
        assert_eq!(menu.stretch, [0.5, 0.0]);
        assert_eq!(menu.direction, StackDirection::Row);
        assert_eq!((menu.gap, menu.padding), (2.0, [3.0; 4]));
        let KindDef::Panel(panel) = &scene.children[1].kind else {
            panic!("a panel");
        };
        assert_eq!(panel.stack.padding, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(
            panel.look,
            PanelLookDef::Fill(PanelFill {
                color: Color::rgba8(0x14, 0x18, 0x2c, 0xff),
                border: 1.0,
                border_color: Color::WHITE,
            })
        );
        let KindDef::Panel(PanelDef {
            look: PanelLookDef::Sprite { slice, .. },
            ..
        }) = &scene.children[2].kind
        else {
            panic!("a panel showing a sprite");
        };
        assert_eq!(*slice, [2.0, 2.0, 2.0, 3.0]);
    }

    #[test]
    fn panel_overrides_replace_the_previous_appearance() {
        let scene = parse(
            "return { format = 3,
                sheets = { ui = { path = 'ui.png', tile_width = 5, tile_height = 5 } },
                children = {
                    { name = 'fill', kind = 'panel', color = '#123456', border = 2 },
                    { name = 'sprite', kind = 'panel', sheet = 'ui', cell = { 0, 0 },
                      slice = 1, tint = '#abcdef' },
                } }",
        )
        .unwrap();
        let sprite = [
            ("sheet".into(), Value::String("ui".into())),
            ("cell".into(), cell_value([0, 0])),
            ("slice".into(), Value::Number(1.0)),
        ]
        .into();
        let changed = scene.children[0]
            .overridden(&sprite, &scene.sheets, None)
            .unwrap();
        assert!(matches!(
            changed.kind,
            KindDef::Panel(PanelDef {
                look: PanelLookDef::Sprite {
                    tint: Color::WHITE,
                    ..
                },
                ..
            })
        ));
        let fill = [("color".into(), Value::String("#123456".into()))].into();
        let changed = scene.children[1]
            .overridden(&fill, &scene.sheets, None)
            .unwrap();
        assert!(matches!(
            changed.kind,
            KindDef::Panel(PanelDef {
                look: PanelLookDef::Fill(PanelFill { border: 0.0, .. }),
                ..
            })
        ));

        // Explicitly mixing both appearances remains an error, whichever is inherited.
        let mut mixed: BTreeMap<String, Value> = sprite;
        mixed.extend(fill);
        for node in &scene.children {
            assert!(node.overridden(&mixed, &scene.sheets, None).is_err());
        }
    }

    #[test]
    fn stack_overrides_restore_fitting_instead_of_inheriting_a_size() {
        let mut scene = parse(
            "return { format = 3, children = {
                { name = 'list', kind = 'stack', width = 40, height = 50, stretch = { 1, 1 } },
                { name = 'panel', kind = 'panel', width = 40, height = 50 },
            } }",
        )
        .unwrap();
        let change = [
            ("width".into(), Value::Bool(false)),
            ("height".into(), Value::Bool(false)),
            ("stretch".into(), Value::List(vec![Value::Number(0.0); 2])),
        ]
        .into();
        for node in &mut scene.children {
            *node = node.overridden(&change, &scene.sheets, None).unwrap();
            assert_eq!(node.kind.stack().unwrap().size, [None, None]);
            assert_eq!(node.fields()["width"], Value::Bool(false));
            assert_eq!(node.fields()["height"], Value::Bool(false));
        }
        assert_eq!(parse(&scene.to_luau().unwrap()).unwrap(), scene);
        // Fitting and stretching the same axis still conflict.
        let err = error(
            "return { format = 3, children = {
            { name = 'list', kind = 'stack', width = false, stretch = { 1, 0 } }
        } }",
        );
        assert!(err.contains("can't stretch"), "{err}");
        assert!(
            parse(
                "return { format = 3, children = {
            { name = 'list', kind = 'stack', width = true }
        } }"
            )
            .is_err()
        );
    }

    #[test]
    fn stacks_and_panels_that_cannot_be_read_are_errors() {
        let node = |fields: &str| {
            error(&format!(
                "return {{ format = 3, sheets = {{ ui = {{ path = 'ui.png', tile_width = 5, \
                 tile_height = 5 }} }}, children = {{ {{ name = 'n', {fields} }} }} }}"
            ))
        };
        let err = node("kind = 'stack', stretch = { 1, 0 }");
        assert!(
            err.contains("a stack without a `width` fits what it holds, so it can't stretch"),
            "{err}"
        );
        let err = node("kind = 'stack', direction = 'up'");
        assert!(
            err.contains(r#"`direction` must be "column" or "row", not "up""#),
            "{err}"
        );
        let err = node("kind = 'panel', padding = { 1, 2 }");
        assert!(err.contains("`padding` must be a number, or four"), "{err}");
        let err = node("kind = 'panel', sheet = 'ui', cell = { 0, 0 }, color = '#ffffff'");
        assert!(
            err.contains("a panel showing a sprite has no `color`"),
            "{err}"
        );
        let err = node("kind = 'panel', slice = 2");
        assert!(
            err.contains("has a `slice` only if it shows a sprite"),
            "{err}"
        );
        let err = node("kind = 'panel', border = -1");
        assert!(err.contains("`border` can't be less than 0"), "{err}");
        let err = node("kind = 'panel', sheet = 'ui', cell = { 0, 0 }, slice = -1");
        assert!(err.contains("`slice` can't be less than 0"), "{err}");
        let err = node("kind = 'stack', color = '#ffffff'");
        assert!(err.contains("a stack node has no field `color`"), "{err}");
    }

    #[test]
    fn reads_and_writes_lights() {
        let source = |light: &str| {
            format!(
                "return {{ format = 4, children = {{ {{ name = 'view', kind = 'view3d', width = \
                 160, height = 90, camera = {{ position = {{ 0, 5, 4 }}, target = {{ 0, 0, 0 }} \
                 }}, children = {{ {{ name = 'lamp', kind = 'light', {light} }} }} }} }} }}"
            )
        };
        let light = |fields: &str| {
            let scene = parse(&source(fields)).unwrap();
            let lamp = &scene.children[0].children[0];
            assert!(lamp.placement3.is_some(), "a light is placed in 3D");
            match &lamp.kind {
                KindDef::Light(light) => *light,
                other => panic!("{other:?}"),
            }
        };
        // A white point light reaching 5, unless it says otherwise.
        assert_eq!(light("y = 3"), LightDef::default());
        assert_eq!(
            light("color = '#ffd9a0', range = 8, energy = 1.5, cone = 0.6, edge = 0.25, yaw = 1"),
            LightDef {
                color: Color::rgb8(0xff, 0xd9, 0xa0),
                range: 8.0,
                energy: 1.5,
                cone: Some(0.6),
                edge: 0.25,
            }
        );
        for (fields, message) in [
            ("range = 0", "`range` must be above 0"),
            ("energy = -1", "`energy` can't be less than 0"),
            ("cone = 4", "`cone` is an angle above 0, up to pi"),
            ("cone = 0.5, edge = 2", "`edge` is from 0 to 1"),
            ("edge = 0.5", "a light has an `edge` only with a `cone`"),
            ("tint = '#ffffff'", "a light node has no field `tint`"),
        ] {
            let err = error(&source(fields));
            assert!(err.contains(message) && err.contains("lamp"), "{err}");
        }
        // Written, it's read back the same, without what it leaves at the defaults.
        let scene = parse(&source("color = '#ffd9a0', range = 8, cone = 0.6, y = 3")).unwrap();
        let written = scene.to_luau().unwrap();
        assert!(written.contains("kind = \"light\""), "{written}");
        assert!(
            !written.contains("energy") && !written.contains("edge"),
            "{written}"
        );
        assert_eq!(parse(&written).unwrap().to_luau().unwrap(), written);
        assert_eq!(
            parse(&written).unwrap().children[0].children[0].kind,
            scene.children[0].children[0].kind
        );
    }

    /// A level with a 3D view: a crate on a floor, a prop placed in it, and a billboard.
    const CELLAR: &str = r##"return {
        format = 2,
        sheets = { tiles = { path = "tiles.png", tile_width = 16, tile_height = 16 } },
        children = {
            {
                name = "view", kind = "view3d", anchor = { 0.5, 0.5 }, width = 160, height = 90,
                pivot = { 80, 45 },
                camera = { position = { 0, 5, 4 }, target = { 0, 0, 0 }, fov = 0.6, far = 50 },
                lighting = { color = "#ffeedd" },
                children = {
                    { name = "floor", kind = "mesh", shape = "plane", size = { 8, 6 }, tint = "#404040" },
                    {
                        name = "stack", kind = "node", x = 1, z = -2, yaw = 0.5,
                        children = {
                            { name = "crate", kind = "mesh", shape = "cuboid", y = 0.5, sheet = "tiles", cell = { 2, 3 } },
                            { name = "ball", kind = "mesh", shape = "sphere", radius = 0.25, y = 1.25 },
                        },
                    },
                    { name = "barrel", kind = "model", model = "models/barrel.glb", x = -2, scale_z = 2 },
                    { name = "hero", kind = "billboard", sheet = "tiles", cell = { 27, 0 }, width = -1, height = 1.5, tags = { "hero" } },
                    { name = "prop", kind = "instance", scene = "prop.scene.luau", pitch = 0.25 },
                },
            },
        },
    }"##;

    #[test]
    fn reads_3d_views_and_their_nodes() {
        let scene = parse(CELLAR).unwrap();
        let view = &scene.children[0];
        assert_eq!(view.placement3, None, "a view is in the 2D scene");
        let KindDef::View3d(def) = &view.kind else {
            panic!("{:?}", view.kind);
        };
        assert_eq!(def.size, [160.0, 90.0]);
        assert_eq!(def.pivot, [80.0, 45.0]);
        assert_eq!(def.camera.position, [0.0, 5.0, 4.0]);
        assert_eq!(def.camera.up, [0.0, 1.0, 0.0]);
        assert_eq!(
            def.camera.projection,
            Projection::Perspective { fov_y: 0.6 }
        );
        assert_eq!([def.camera.near, def.camera.far], [0.1, 50.0]);
        assert_eq!(def.lighting.color, Color::rgb8(0xff, 0xee, 0xdd));
        assert_eq!(def.lighting.ambient, Lighting::default().ambient);

        let [floor, stack, barrel, hero, prop] = &view.children[..] else {
            panic!("{:?}", view.children);
        };
        assert_eq!(
            floor.kind,
            KindDef::Mesh(MeshDef {
                shape: Shape::Plane([8.0, 6.0]),
                sprite: None,
                tint: Color::rgb8(0x40, 0x40, 0x40),
            })
        );
        assert_eq!(floor.placement3, Some(Placement3::default()));
        // A plain node in a view is placed in 3D, and so is everything inside it.
        assert_eq!(
            stack.placement3,
            Some(Placement3 {
                z: -2.0,
                yaw: 0.5,
                ..Placement3::default()
            })
        );
        assert_eq!([stack.x, stack.y], [1.0, 0.0]);
        let [crate_, ball] = &stack.children[..] else {
            panic!("{:?}", stack.children);
        };
        assert_eq!(
            crate_.kind,
            KindDef::Mesh(MeshDef {
                shape: Shape::Cuboid([1.0; 3]),
                sprite: Some(CellDef {
                    sheet: "tiles".into(),
                    cell: [2, 3],
                }),
                tint: Color::WHITE,
            })
        );
        assert_eq!(crate_.y, 0.5);
        assert!(
            matches!(ball.kind, KindDef::Mesh(MeshDef { shape: Shape::Sphere(r), .. }) if r == 0.25)
        );
        assert_eq!(
            barrel.kind,
            KindDef::Model(ModelDef {
                model: "models/barrel.glb".into(),
                tint: Color::WHITE,
            })
        );
        assert_eq!(barrel.placement3.unwrap().scale_z, 2.0);
        assert_eq!(
            hero.kind,
            KindDef::Billboard(BillboardDef {
                sheet: "tiles".into(),
                cell: [27, 0],
                size: [-1.0, 1.5],
                tint: Color::WHITE,
            })
        );
        assert!(matches!(prop.kind, KindDef::Instance(_)));
        assert_eq!(prop.placement3.unwrap().pitch, 0.25);
    }

    #[test]
    fn files_of_props_place_their_nodes_in_3d() {
        // A file of its own, for an instance to place in a view: its meshes are placed in 3D, and
        // so is a plain node with a field only nodes in 3D have, but not one without.
        let scene = parse(
            "return { format = 2, children = {
                { name = 'box', kind = 'mesh', shape = 'cuboid', yaw = 1 },
                { name = 'turned', kind = 'node', yaw = 1, children = { { name = 'in', kind = 'node' } } },
                { name = 'group', kind = 'node', children = { { name = 'in', kind = 'node' } } },
            } }",
        )
        .unwrap();
        let [mesh, turned, group] = &scene.children[..] else {
            panic!("{:?}", scene.children);
        };
        assert_eq!(mesh.placement3.unwrap().yaw, 1.0);
        assert_eq!(turned.placement3.unwrap().yaw, 1.0);
        assert!(
            turned.children[0].placement3.is_some(),
            "inside a node in 3D"
        );
        assert_eq!(group.placement3, None);
        assert_eq!(group.children[0].placement3, None);
    }

    #[test]
    fn nodes_in_3d_are_checked() {
        let view = |children: &str, fields: &str| {
            with_children(&format!(
                "{{ name = 'view', kind = 'view3d', width = 10, height = 10, {fields} \
                 children = {{ {children} }} }}"
            ))
        };
        let camera = "camera = { position = { 0, 1, 2 }, target = { 0, 0, 0 } },";
        let cases = [
            (
                view("", ""),
                "node `view`: a view3d node has a `camera`, { position, target }",
            ),
            (
                view(
                    "",
                    "camera = { position = { 0, 1 }, target = { 0, 0, 0 } },",
                ),
                "`camera.position` must be three numbers",
            ),
            (
                view("", "camera = { position = { 0, 1, 2 } },"),
                "a camera has a `target`",
            ),
            (
                view(
                    "",
                    "camera = { position = { 0, 1, 2 }, target = { 0, 0, 0 }, fov = 1, \
                     height = 2 },",
                ),
                "a camera has a `fov`, for perspective, or a `height`, for none, not both",
            ),
            (
                view(
                    "",
                    "camera = { position = { 0, 1, 2 }, target = { 0, 0, 0 }, fov = 4 },",
                ),
                "a camera's fov must be between 0 and pi, not 4",
            ),
            (
                view(
                    "",
                    "camera = { position = { 0, 1, 2 }, target = { 0, 0, 0 }, zoom = 2 },",
                ),
                "a camera has no field `zoom`",
            ),
            (
                view("", &format!("{camera} lighting = {{ colour = 1 }},")),
                "`lighting` has no field `colour`",
            ),
            (
                view("", &format!("{camera} rotation = 1,")),
                "node `view`: a 3D view moves, but doesn't turn or stretch",
            ),
            (
                view(
                    "{ name = 'box', kind = 'mesh', rotation = 1, shape = 'cuboid' }",
                    camera,
                ),
                "node `view/box`: a node placed in 3D, in a 3D view, turns by `yaw`, `pitch` and \
                 `roll`, and has no `rotation`",
            ),
            (
                view("{ name = 'box', kind = 'node', anchor = { 0, 0 } }", camera),
                "node `view/box`: a node placed in 3D, in a 3D view, has no `anchor`",
            ),
            (
                view("{ name = 'box', kind = 'mesh' }", camera),
                r#"a mesh node has a `shape`: "cuboid", "plane" or "sphere""#,
            ),
            (
                view("{ name = 'box', kind = 'mesh', shape = 'cone' }", camera),
                r#"`shape` must be "cuboid", "plane" or "sphere", not "cone""#,
            ),
            (
                view(
                    "{ name = 'box', kind = 'mesh', shape = 'cuboid', size = { 1, 0, 1 } }",
                    camera,
                ),
                "`size` must be above 0, not 0",
            ),
            (
                view(
                    "{ name = 'box', kind = 'mesh', shape = 'sphere', size = { 1, 1, 1 } }",
                    camera,
                ),
                "a mesh node has no field `size`",
            ),
            (
                view(
                    "{ name = 'box', kind = 'mesh', shape = 'plane', cell = { 0, 0 } }",
                    camera,
                ),
                "a mesh node with a sprite has a `sheet`",
            ),
            (
                view("{ name = 'barrel', kind = 'model' }", camera),
                "a model node has a `model`, the path of a model file",
            ),
            (
                view(
                    "{ name = 'hero', kind = 'billboard', sheet = 'tiles' }",
                    camera,
                ),
                "a billboard node has a `cell`",
            ),
            (
                view(
                    "{ name = 'hero', kind = 'sprite', sheet = 'tiles', cell = { 0, 0 }, z = 1 }",
                    camera,
                ),
                "a sprite node has no field `z`",
            ),
        ];
        for (source, expected) in cases {
            let err = error(&source);
            assert!(err.contains(expected), "{source}\n{err}");
        }
    }

    #[test]
    fn overriding_nodes_in_3d_keeps_them_in_3d() {
        let scene = parse(&with_children(
            "{ name = 'view', kind = 'view3d', width = 10, height = 10, \
               camera = { position = { 0, 1, 2 }, target = { 0, 0, 0 } }, children = {
                 { name = 'box', kind = 'mesh', shape = 'cuboid', x = 1, yaw = 0.5 },
                 { name = 'group', kind = 'node' } } }",
        ))
        .unwrap();
        let view = &scene.children[0];
        let change = |field: &str, value: f64| [(field.to_owned(), Value::Number(value))].into();
        let box_ = &view.children[0];
        let moved = box_
            .overridden(&change("z", 3.0), &scene.sheets, None)
            .unwrap();
        assert_eq!(
            moved.placement3,
            Some(Placement3 {
                z: 3.0,
                yaw: 0.5,
                ..Placement3::default()
            })
        );
        assert_eq!(moved.x, 1.0);
        assert_eq!(
            box_.overridden(&BTreeMap::new(), &scene.sheets, None)
                .unwrap(),
            *box_
        );
        let err = box_
            .overridden(&change("rotation", 1.0), &scene.sheets, None)
            .unwrap_err();
        assert!(err.contains("has no `rotation`"), "{err}");
        // A plain node a file places in 2D takes a 3D field.
        let group = NodeDef::new("group", KindDef::Node);
        let turned = group
            .overridden(&change("yaw", 1.0), &scene.sheets, None)
            .unwrap();
        assert_eq!(turned.placement3.unwrap().yaw, 1.0);
    }

    #[test]
    fn clips_have_tracks_of_3d_properties() {
        let scene = parse(
            "return { format = 2, clips = { spin = { duration = 1, tracks = {
                { property = 'yaw', keys = { { 0, 0 }, { 1, 6.28 } } },
                { property = 'scale_z', keys = { { 0, 1 } } },
            } } } }",
        )
        .unwrap();
        let properties: Vec<_> = scene.clips["spin"]
            .tracks
            .iter()
            .map(|track| track.keys.property_name())
            .collect();
        assert_eq!(properties, ["yaw", "scale_z"]);
    }

    #[test]
    fn errors_name_the_node() {
        let cases = [
            (
                "{ name = 'mill', kind = 'sprite', sheet = 'tiles', cell = { 0, 0 }, colour = 1 }",
                "node `mill`: a sprite node has no field `colour`",
            ),
            (
                "{ name = 'mill', kind = 'node', children = { { name = 'sails', kind = 'node', \
                 rotation = 'fast' } } }",
                "node `mill/sails`: `rotation` must be a number, not a string",
            ),
            (
                "{ name = 'a', kind = 'node' }, { name = 'a', kind = 'node' }",
                "the scene: two of its children are named `a`",
            ),
            (
                "{ kind = 'node' }",
                "the scene's child 1: a node has a `name`",
            ),
            ("{ name = 'a/b', kind = 'node' }", "`/` separates the names"),
            (
                "{ name = 'a', kind = 'circle' }",
                "there's no kind of node `circle`",
            ),
            (
                "{ name = 'a', kind = 'sprite', sheet = 'other', cell = { 0, 0 } }",
                "there's no sheet `other`",
            ),
            (
                "{ name = 'a', kind = 'sprite', sheet = 'tiles', cell = { 0.5, 0 } }",
                "`cell` must be a whole number",
            ),
            (
                "{ name = 'a', kind = 'sprite', sheet = 'tiles', cell = { 0, 0 }, width = 4 }",
                "has a `height`",
            ),
            (
                "{ name = 'a', kind = 'rect', width = 4 }",
                "a rect node has a `height`",
            ),
            (
                "{ name = 'a', kind = 'node', x = 0/0 }",
                "x` must be a finite number",
            ),
            (
                "{ name = 'a', kind = 'text', text = 'hi', font_scale = 0 }",
                "`font_scale` must be above 0",
            ),
            (
                "{ name = 'a', kind = 'node', tags = { 'x', 3 } }",
                "`tags` must be a string",
            ),
            (
                "{ name = 'a', kind = 'node', props = { route = { [1] = 'up', [3] = 'down' } } }",
                "props.route` has numbers for keys, but not 1, 2, 3",
            ),
            (
                "{ name = 'a', kind = 'node', props = { mixed = { 1, x = 2 } } }",
                "props.mixed` has both strings and numbers for keys",
            ),
            (
                "{ name = 'a', kind = 'rect', width = 4, height = 4, color = 'red' }",
                "`color`:",
            ),
            (
                "{ name = 'a', kind = 'area', width = 4, height = 4, sheet = 'tiles' }",
                "node `a`: an area node has no field `sheet`",
            ),
            (
                "{ name = 'a', kind = 'node', bounce = 1 }",
                "node `a`: a plain node has no field `bounce`",
            ),
            (
                "{ name = 'a', kind = 'text', text = 'hi', align = 'middle' }",
                r#"`align` must be "left", "center" or "right", not "middle""#,
            ),
            (
                "{ name = 'a', kind = 'area', width = 4, height = 4, stretch = { 1 } }",
                "`stretch` must be two numbers",
            ),
            (
                "{ name = 'a', kind = 'node', anchor = 'middle' }",
                "`anchor` must be a list",
            ),
            (
                "{ name = 'a', kind = 'text', text = 'hi', stretch = { 1, 0 } }",
                "a text node has no field `stretch`",
            ),
        ];
        for (children, expected) in cases {
            let err = error(&with_children(children));
            assert!(err.contains(expected), "{children}\n{err}");
        }
    }

    #[test]
    fn tilemaps_and_their_layers_are_checked() {
        let map = |layers: &str, extra: &str| {
            with_children(&format!(
                "{{ name = 'map', kind = 'tilemap', tileset = 't.tileset.luau', columns = 3, \
                 rows = 2, {extra} children = {{ {layers} }} }}"
            ))
        };
        let layer = |fields: &str| format!("{{ name = 'ground', kind = 'tile_layer', {fields} }}");
        let cases = [
            (
                map(&layer("legend = { a = 'x' }, tiles = { 'aaa' }"), ""),
                "`tiles` has 1 rows, but its tilemap has 2",
            ),
            (
                map(&layer("legend = { a = 'x' }, tiles = { 'aaa', 'aa' }"), ""),
                "row 1 of `tiles` is 2 cells wide, but its tilemap is 3",
            ),
            (
                map(&layer("legend = { a = 'x' }, tiles = { 'aaa', 'abz' }"), ""),
                "row 1 of `tiles` has `b`, which isn't in the `legend`",
            ),
            (
                map(
                    &layer("legend = { ab = 'x' }, tiles = { '   ', '   ' }"),
                    "",
                ),
                "each of its keys is a single character",
            ),
            (
                map(
                    &layer("legend = { [' '] = 'x' }, tiles = { '   ', '   ' }"),
                    "",
                ),
                "a space is an empty cell",
            ),
            (
                map(&layer("x = 4, tiles = { '   ', '   ' }"), ""),
                "node `map/ground`: a tile layer is where its tilemap is",
            ),
            (
                map(&layer("anchor = { 0, 0 }, tiles = { '   ', '   ' }"), ""),
                "node `map/ground`: a tile layer is where its tilemap is",
            ),
            (
                map("", "rotation = 1,"),
                "node `map`: a tilemap moves, but doesn't turn or stretch",
            ),
            (
                with_children(&layer("tiles = {}")),
                "a tile layer must be the child of a tilemap",
            ),
            (
                with_children(
                    "{ name = 'map', kind = 'tilemap', tileset = 't', columns = 0, rows = 1 }",
                ),
                "`columns` must be at least 1",
            ),
            (
                with_children(
                    "{ name = 'a', kind = 'instance', scene = 'b.scene.luau', \
                     overrides = { x = { kind = 'rect' } } }",
                ),
                "a node's `kind` can't be overridden",
            ),
        ];
        for (source, expected) in cases {
            let err = error(&source);
            assert!(err.contains(expected), "{source}\n{err}");
        }
        // Spaces are empty cells, and other nodes may be among the layers.
        assert!(
            parse(&map(
                &format!(
                    "{}, {{ name = 'people', kind = 'node' }}",
                    layer("legend = { a = 'x' }, tiles = { 'a a', '   ' }")
                ),
                ""
            ))
            .is_ok()
        );
    }

    #[test]
    fn tables_holding_themselves_are_too_deep() {
        let err = error("local t = {} t.t = t return { format = 1, props = { t = t } }");
        assert!(err.contains("too deep"), "{err}");
        let err = error(
            "local node = { name = 'a', kind = 'node' } node.children = { node }
             return { format = 1, children = { node } }",
        );
        assert!(err.contains("too deep"), "{err}");
    }

    #[test]
    fn files_that_would_never_finish_being_read_are_stopped() {
        let err = error("while true do end return { format = 1 }");
        assert!(err.contains("runs too long"), "{err}");
        let err = error("local function f() return f() + 1 end return f()");
        assert!(
            err.contains("runs too long") || err.contains("stack"),
            "{err}"
        );
        // Strings still have their methods.
        let err = error("return { format = 1, props = { s = ('x'):rep(1e9) } }");
        assert!(err.contains("MiB of memory"), "{err}");
        // Each table twice in the one around it, 60 deep: read every time, it's 2^60 tables.
        let err = error(
            "local t = {} for _ = 1, 60 do t = { t, t } end
             return { format = 1, props = { t = t } }",
        );
        assert!(err.contains("too many to read"), "{err}");
        // Saves are read the same way.
        let err = evaluate_any("while true do end", Path::new("save.luau")).unwrap_err();
        assert!(err.contains("runs too long"), "{err}");
    }

    #[test]
    fn tilesets_name_their_tiles() {
        let tileset = TilesetFile::parse(
            r##"return {
                format = 1,
                tile_width = 16,
                tile_height = 16,
                sheets = { tiles = { path = "tiles.png", tile_width = 8, tile_height = 8 } },
                tiles = {
                    grass = { sheet = "tiles", cell = { 1, 0 }, tint = "#3e6b3f" },
                    ["tree.1"] = { sheet = "tiles", cell = { 0, 1 }, solid = true, props = { kind = "tree" } },
                },
            }"##,
            "world.tileset.luau",
        )
        .unwrap();
        assert_eq!([tileset.tile_width, tileset.tile_height], [16, 16]);
        assert_eq!(tileset.sheets["tiles"].tile_width, 8);
        let tree = &tileset.tiles["tree.1"];
        assert!(tree.solid);
        assert_eq!(tree.cell, [0, 1]);
        assert_eq!(tree.props["kind"], Value::String("tree".into()));
        assert_eq!(tileset.tiles["grass"].tint, Color::rgb8(0x3e, 0x6b, 0x3f));
        assert!(!tileset.tiles["grass"].solid);

        let err = TilesetFile::parse(
            "return { format = 1, tile_width = 16, tile_height = 16, sheets = {}, \
             tiles = { wall = { sheet = 'tiles', cell = { 0, 0 } } } }",
            "world.tileset.luau",
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("tileset world.tileset.luau: tile `wall`: there's no sheet `tiles`"),
            "{err}"
        );
        let err = TilesetFile::parse("return { format = 1, tile_width = 16 }", "t.tileset.luau")
            .unwrap_err()
            .to_string();
        assert!(err.contains("a tileset has a `tile_height`"), "{err}");
    }

    #[test]
    fn font_files_describe_bitmap_and_truetype_fonts() {
        let font = |source: &str| FontFile::parse(source, "title.font.luau");
        let bitmap = font(
            "return { format = 3, image = 'title.png', cell = { 10, 12 },
                characters = { 'AB C', 'é?' } }",
        )
        .unwrap();
        let FontFile::Bitmap(def) = bitmap else {
            panic!("a bitmap font: {bitmap:?}");
        };
        assert_eq!(def.image, "title.png");
        assert_eq!(def.cell, [10, 12]);
        assert_eq!(
            (def.spacing, def.space, def.line_height),
            (1, 5, 13),
            "defaults"
        );
        assert!(!def.monospace);
        assert_eq!(
            def.characters.cells(16),
            [
                ('A', [0, 0]),
                ('B', [1, 0]),
                ('C', [3, 0]),
                ('é', [0, 1]),
                ('?', [1, 1])
            ],
            "a space skips a cell"
        );

        let FontFile::Bitmap(def) = font(
            "return { format = 3, image = 'digits.png', cell = { 5, 7 }, characters = '0123',
                monospace = true, spacing = 2, line_height = 9 }",
        )
        .unwrap() else {
            panic!("a bitmap font");
        };
        assert_eq!((def.spacing, def.space, def.line_height), (2, 7, 9));
        assert_eq!(
            def.characters.cells(3)[3],
            ('3', [0, 1]),
            "on to the next row"
        );

        assert_eq!(
            font("return { format = 3, file = 'Hack.ttf', size = 13 }").unwrap(),
            FontFile::Vector(VectorFontDef {
                file: "Hack.ttf".into(),
                size: 13.0,
                smooth: true,
            })
        );
        assert_eq!(
            font("return { format = 3, file = 'pixel.ttf', size = 8, smooth = false }").unwrap(),
            FontFile::Vector(VectorFontDef {
                file: "pixel.ttf".into(),
                size: 8.0,
                smooth: false,
            })
        );
    }

    #[test]
    fn font_files_that_cannot_be_read_are_errors() {
        for (source, problem) in [
            ("return { image = 'a.png' }", "the font has no `format`"),
            (
                "return { format = 3 }",
                "an `image` of its glyphs, or a TrueType",
            ),
            (
                "return { format = 3, image = 'a.png', file = 'a.ttf' }",
                "not both",
            ),
            ("return { format = 3, image = 'a.png' }", "has a `cell`"),
            (
                "return { format = 3, image = 'a.png', cell = { 0, 8 }, characters = 'a' }",
                "`cell` must be at least 1",
            ),
            (
                "return { format = 3, image = 'a.png', cell = { 8, 8 } }",
                "has its `characters`",
            ),
            (
                "return { format = 3, image = 'a.png', cell = { 8, 8 }, characters = 7 }",
                "`characters` must be a string, or a list of them",
            ),
            (
                "return { format = 3, image = 'a.png', cell = { 8, 8 }, characters = { 'ab', 'ca' } }",
                "`characters` has `a` twice",
            ),
            (
                "return { format = 3, image = 'a.png', cell = { 8, 8 }, characters = 'a\\tb' }",
                "can't hold control characters",
            ),
            ("return { format = 3, file = 'a.ttf' }", "has a `size`"),
            (
                "return { format = 3, file = 'a.ttf', size = -1 }",
                "`size` must be above 0",
            ),
            (
                "return { format = 3, file = 'a.ttf', size = 8, cell = { 8, 8 } }",
                "a font has no field `cell`",
            ),
        ] {
            let err = FontFile::parse(source, "f.font.luau")
                .unwrap_err()
                .to_string();
            assert!(err.contains(problem), "{source}: {err}");
            assert!(err.starts_with("font f.font.luau: "), "{err}");
        }
    }
}
