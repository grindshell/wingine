//! Scenes: trees of sprites, rectangles, text, areas, tilemaps and 3D views, laid out in files that
//! people and tools can both write, for levels, screens and characters.
//!
//! A scene file is Luau that returns one table of plain data: tables, strings, numbers and
//! booleans, with no function calls, variables or expressions apart from negative numbers. That
//! way a tool, such as an editor, can read a file, change it and write it back. The engine runs
//! the file with nothing in scope, so it can't reach the engine or the game; [`SceneFile`] is what
//! it holds, and [`SceneFile::to_luau`] writes it in one canonical layout. Scene files are named
//! `*.scene.luau` by convention.
//!
//! ```lua
//! return {
//!     format = 2,
//!     sheets = {
//!         tiles = {
//!             path = "tiles.png",
//!             tile_width = 16,
//!             tile_height = 16,
//!         },
//!     },
//!     props = {
//!         music = "sounds/garden.ogg",
//!     },
//!     children = {
//!         {
//!             name = "map",
//!             kind = "tilemap",
//!             tileset = "garden.tileset.luau",
//!             columns = 4,
//!             rows = 3,
//!             children = {
//!                 {
//!                     name = "ground",
//!                     kind = "tile_layer",
//!                     legend = {
//!                         ["."] = "grass",
//!                         ["~"] = "water",
//!                     },
//!                     tiles = {
//!                         "....",
//!                         ".~~.",
//!                         "....",
//!                     },
//!                 },
//!             },
//!         },
//!         {
//!             name = "mill",
//!             kind = "sprite",
//!             x = 120,
//!             y = 80,
//!             sheet = "tiles",
//!             cell = { 3, 20 },
//!             pivot = { 8, 16 },
//!             tint = "#d8b070",
//!             children = {
//!                 {
//!                     name = "sails",
//!                     kind = "node",
//!                     y = -12,
//!                     rotation = 0.5,
//!                 },
//!             },
//!         },
//!         {
//!             name = "miller",
//!             kind = "instance",
//!             x = 136,
//!             y = 80,
//!             scene = "villager.scene.luau",
//!             overrides = {
//!                 body = {
//!                     tint = "#c8b8e8",
//!                 },
//!             },
//!         },
//!         {
//!             name = "pond",
//!             kind = "area",
//!             x = 16,
//!             y = 16,
//!             width = 32,
//!             height = 16,
//!             tags = { "solid" },
//!             props = {
//!                 text = "The pond is full of frogs.",
//!             },
//!         },
//!     },
//! }
//! ```
//!
//! At the top of the file:
//!
//! - `format`, required, is the version of the format, 2 ([`FORMAT`]), which added 3D views. A
//!   newer engine reads older formats, as this one reads format 1; this one refuses newer ones.
//! - `sheets` names the sprite sheets sprites come from: images, by their path relative to the
//!   game's folder, divided into a grid of sprites `tile_width` by `tile_height` texels.
//! - `props` is data for the game about the scene as a whole. The engine doesn't read props;
//!   they hold booleans, numbers, strings, and tables of them that are lists or have strings for
//!   keys.
//! - `children` is a list of nodes, the top of the scene's tree.
//! - `clips` names animations of its nodes, such as a character's walk. See below.
//!
//! Every node has a `name`, unique among its siblings and without `/`, so a path such as
//! `"mill/sails"` finds a node from the one above it, and a `kind`. Each of these is optional:
//!
//! - `x` and `y`, where its origin is within its parent, 0 by default. Scene coordinates are
//!   the coordinates the scene is drawn in: canvas pixels, or the world's with a camera.
//! - `anchor`, the point of its parent's rectangle that `x` and `y` are measured from instead of
//!   the parent's origin: fractions of its width and height from its top-left corner,
//!   `{ x, y }`. See below.
//! - `rotation`, radians clockwise about its origin, 0 by default.
//! - `scale_x` and `scale_y`, how much it's stretched along its own axes about its origin, 1 by
//!   default. Negative scales mirror it.
//! - `visible`, whether it and its children draw, true by default.
//! - `tags`, a list of strings, for finding nodes of a sort, such as `"solid"`.
//! - `props`, data for the game, like the scene's.
//! - `children`, the nodes inside it. Each is placed within its parent: it moves, turns and
//!   stretches with it.
//!
//! The kinds, and the fields each has:
//!
//! - `node`: nothing but a place, for grouping nodes, turning them together, or marking a spot.
//! - `sprite`: `sheet`, the name of one of the scene's sheets, and `cell`, the sprite's column and
//!   row in it, `{ column, row }`, counting from 0 at the top-left. Optionally, `width` and
//!   `height` to stretch it to (its own size by default), `stretch`, as a rect's, and `tint`, a
//!   color its colors are multiplied by.
//! - `rect`: a filled rectangle, `width` by `height`, of `color` (white by default). `stretch`,
//!   `{ 0, 0 }` by default, is the fractions of its parent's width and height added to its own,
//!   so it grows and shrinks with its parent: a rect `width = 0` with `stretch = { 1, 0 }` is as
//!   wide as its parent, and one `width = -16` is 16 narrower. It's never less than nothing.
//! - `text`: `text` in the built-in font, `font_scale` times its size (1 by default), wrapped
//!   between words to `wrap` if given, in `color` (white by default). `align`, `"left"` by
//!   default, `"center"` or `"right"`, lines its lines up along their left edges, about their
//!   middles or along their right edges, and puts that edge or middle at the node's origin, so
//!   `"right"` ends the text there. Text moves with its node, but doesn't turn or stretch.
//! - `area`: a rectangle `width` by `height` that doesn't draw, for marking out part of a scene,
//!   such as somewhere the player can't walk or where something happens, or for laying out the
//!   nodes inside it. It has a `stretch`, as a rect does.
//! - `tilemap`: a grid of `columns` by `rows` tiles from the tileset file `tileset`, with its
//!   top-left corner at the node's origin. Its layers are its children of the kind `tile_layer`,
//!   which draw in order with its other children, so a node can draw between two layers, such
//!   as characters between the ground and treetops. A tilemap moves with its node, but doesn't
//!   turn or stretch, so it has no `rotation`, `scale_x` or `scale_y`.
//! - `tile_layer`: a layer of the tilemap it's in. `tiles` is a string for each row with a
//!   character for each column, and `legend` the name of the tile each character stands for, as
//!   the tileset names it: `{ ["."] = "grass", T = "tree" }`. A space is an empty cell. A layer is
//!   where its tilemap is, so it has no `x`, `y`, `rotation`, `scale_x` or `scale_y`.
//! - `instance`: the scene file `scene`, placed at the node. The node becomes a plain node holding
//!   the scene's nodes, with the scene's props and its own (its own win), and any children of its
//!   own after the scene's. `overrides` changes nodes of the scene, by their paths from its top:
//!   `{ ["body/hat"] = { tint = "#c8b8e8" } }` sets fields as the scene file would have them, apart
//!   from `name`, `kind` and `children`, and changes `props` one by one rather than replacing
//!   them. A `sheet` set by an override is one of the sheets of the file the node is from.
//! - `view3d`: a rectangle of the scene showing a 3D world, as [`Canvas::begin_3d`] shows one,
//!   whose nodes are its children. It's laid out as an area is, `width` by `height` with a
//!   `stretch`, and moves with its node, but doesn't turn or stretch. `camera`, required, is
//!   `{ position = { x, y, z }, target = { x, y, z } }`, with `up` (`{ 0, 1, 0 }`), `fov`
//!   (`0.8`) or for an orthographic camera `height`, `near` (`0.1`) and `far` (`1000`) if they
//!   aren't the defaults; `lighting`, `{ direction, color, ambient }`, is the default lighting,
//!   field by field, where it leaves fields out.
//! - `mesh`, in a view: a built-in shape centered on its origin, `shape`, `"cuboid"` with
//!   `size = { width, height, depth }`, `"plane"`, facing up, with `size = { width, depth }`, or
//!   `"sphere"` with a `radius`, sizes 1 and the radius a half by default; optionally a sprite,
//!   `sheet` and `cell`, and a `tint`.
//! - `model`, in a view: the model file `model`, its path relative to the game's folder, and a
//!   `tint`.
//! - `billboard`, in a view: a sprite, `sheet` and `cell`, standing upright with the middle of its
//!   bottom edge at the node's origin, facing the camera across the ground, `width` by `height`
//!   world units (1 by default; a negative width mirrors it), and a `tint`. It moves with its node,
//!   but doesn't turn or stretch.
//!
//! Nodes in a view are placed in 3D, in its world, right-handed with `y` up: by `x`, `y` and `z`,
//! turned by `yaw`, `pitch` and `roll`, radians about `y`, `x` and `z`, and stretched by `scale_x`,
//! `scale_y` and `scale_z`, composed as `graphics.draw_mesh`'s options are: stretched, rolled,
//! pitched, turned, then moved. They have no `rotation` or `anchor` ([`NodeDef::placement3`]).
//! Plain nodes and instances can be in views too, so a prop can be a scene file of its own, whose
//! nodes an instance places in a view: a file reads nodes as placed in 3D if they're of the kinds
//! only views hold, inside a view or a node placed in 3D, or have fields only those nodes have.
//! Once instances have placed their scenes, a mesh, model or billboard anywhere but in a view is an
//! error, as is a node of another kind in one, naming the node.
//!
//! What a sprite, rect, text or area shows has its top-left corner at the node's origin, unless
//! it has a `pivot`: the point of it, from its top-left corner, `{ x, y }`, that's at the origin
//! instead, and that it turns and stretches about. A sprite of a person 16 texels tall standing
//! at `x`, `y` has a pivot of `{ 8, 16 }`, the middle of its feet. Text's pivot is from the top
//! of the edge or middle it's aligned by. Colors are hex strings, such as `"#f2c14e"`, or
//! `"#00000080"` with alpha. Any field a node doesn't have is an error naming the node, as is a
//! missing one it must have.
//!
//! Screens are laid out by anchors, so they fit canvases of any size. A node with an `anchor` is
//! placed from that point of its parent's rectangle: `{ 0, 0 }` is its top-left corner,
//! `{ 0.5, 0.5 }` its middle and `{ 1, 1 }` its bottom-right, so a node `anchor = { 1, 0 }` with
//! `x = -8` is 8 in from its parent's right edge, wherever that is. A parent's rectangle is what
//! it covers, pivot and all; a plain node passes its parent's on, carried along as it moves, so
//! grouping nodes changes nothing; and the nodes at the top of a scene are in the canvas's, at
//! the scene's origin. Without an anchor, a node is placed from its parent's origin, as nodes of
//! characters are.
//!
//! ```lua
//! {
//!     name = "bar",
//!     kind = "rect",
//!     anchor = { 0, 1 },
//!     width = 0,
//!     height = 12,
//!     stretch = { 1, 0 },
//!     pivot = { 0, 12 },
//!     color = "#14182c",
//!     children = {
//!         {
//!             name = "score",
//!             kind = "text",
//!             x = -4,
//!             y = 1,
//!             anchor = { 1, 0 },
//!             text = "1,250",
//!             align = "right",
//!         },
//!     },
//! }
//! ```
//!
//! That's a bar 12 tall along the bottom of the canvas, as wide as it, with a score ending 4 in
//! from its right end.
//!
//! Characters are rigs: sprites of their parts, each the child of the part it hangs from and
//! turning about its pivot, its joint, as cutouts do. Clips animate them. Each is a `duration` in
//! seconds, whether it `loop`s (false by default), and `tracks`, each changing one `property` of
//! one `node`, by its path from the node the clip poses (without a `node`, that node itself).
//! Tracks change `x`, `y`, `rotation`, `scale_x` and `scale_y`, or for nodes in 3D views, `z`,
//! `yaw`, `pitch`, `roll` and `scale_z` but not `rotation`, which move from key to key in a
//! straight line, or with `interpolation = "step"` jump at each key; and `visible` and the `cell`
//! of a sprite, billboard or mesh, which always step. `keys` are a time in seconds, from 0 to the
//! duration, and the value then, in order of time; before the first key a property has the first
//! key's value, and after the last, the last key's. A loop that should come back to where it
//! started has its first key's value again at its end.
//!
//! ```lua
//! clips = {
//!     wave = {
//!         duration = 1.2,
//!         loop = true,
//!         tracks = {
//!             {
//!                 node = "body/arm",
//!                 property = "rotation",
//!                 keys = {
//!                     { 0, -0.25 },
//!                     { 0.6, 0.5 },
//!                     { 1.2, -0.25 },
//!                 },
//!             },
//!             {
//!                 node = "body/head/face",
//!                 property = "cell",
//!                 keys = {
//!                     { 0, { 1, 0 } },
//!                     { 0.6, { 2, 0 } },
//!                 },
//!             },
//!         },
//!     },
//! },
//! ```
//!
//! A clip doesn't keep time: a game keeps how long it has played and poses a node with it then,
//! with [`Scene::pose`], which sets only the properties it has tracks for. Since paths are from
//! the node posed, one clip poses every instance of its file.
//!
//! A tileset file, named `*.tileset.luau` by convention, is plain data in the same way, holding
//! the kinds of tile tilemaps are made of, by name ([`TilesetFile`]):
//!
//! ```lua
//! return {
//!     format = 2,
//!     tile_width = 16,
//!     tile_height = 16,
//!     sheets = {
//!         tiles = {
//!             path = "tiles.png",
//!             tile_width = 16,
//!             tile_height = 16,
//!         },
//!     },
//!     tiles = {
//!         grass = {
//!             sheet = "tiles",
//!             cell = { 5, 0 },
//!             tint = "#3e6b3f",
//!         },
//!         water = {
//!             sheet = "tiles",
//!             cell = { 8, 5 },
//!             tint = "#21406a",
//!             solid = true,
//!             props = {
//!                 text = "Cold, clear water.",
//!             },
//!         },
//!     },
//! }
//! ```
//!
//! Every tile is `tile_width` by `tile_height`, which its sprite, from one of the tileset's
//! sheets, is stretched to. A tile may have a `tint`, be `solid`, so that a cell holding it on any
//! layer blocks movement, and have `props` of its own.
//!
//! Nodes draw in order, each before its children, so later ones are on top. A [`Scene`] is a
//! scene loaded to play: its nodes, which a game finds by path or tag, moves, turns, changes,
//! poses, adds and removes, drawn with [`Scene::draw`], which fits it to the canvas. A scene drawn
//! without a camera is in canvas pixels, so [`Scene::bounds`] and [`Scene::contains`] test where
//! the mouse is against its nodes, and [`Scene::pick`] finds a 3D view's nodes under it. A view
//! draws its world where it is in the tree: what's drawn after it goes over it.

mod clip;
mod file;
mod writer;

use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    rc::Rc,
};

pub use clip::{Clip, Interpolation, KeyValue, Keys, Property, Track};
pub use file::{
    AreaDef, BillboardDef, CellDef, FORMAT, InstanceDef, KindDef, MeshDef, ModelDef, NodeDef,
    Placement3, Props, RectDef, SceneFile, Shape, SheetDef, SpriteDef, TextDef, TileDef,
    TileLayerDef, TilemapDef, TilesetFile, Value, View3dDef,
};
pub(crate) use file::{evaluate_any, plain};
pub(crate) use writer::write_value;

use crate::{
    Context, Error, Result,
    graphics::{
        Align, Camera3d, Canvas, Canvas3d, Color, Font, Lighting, Mesh, MeshData, Model, Ray,
        Sprite, SpriteSheet, TextLayout, Texture, View3d,
    },
    lua::check_tilemap_size,
    math::{Affine, Affine3, Bounds3, Rect, add3, scale3},
    save,
    tilemap::{Tile, TileId, Tilemap, Tileset},
};

impl SceneFile {
    /// The scene as the text of a scene file, in the canonical layout: every field of a node on
    /// a line of its own, in a fixed order, leaving out those at their defaults. Writing the
    /// same scene always gives the same text, so files change by as little as the scene does.
    ///
    /// Fails if a number isn't finite, which a scene file can't hold.
    pub fn to_luau(&self) -> Result<String> {
        writer::write(self).map_err(Error::WriteScene)
    }

    /// Writes the scene to the file at `path`, as [`SceneFile::to_luau`] gives it. The scene is
    /// written in full to a file of its own first, then takes the old one's place, so stopping
    /// partway through leaves the old one as it was.
    pub fn write(&self, path: impl AsRef<Path>) -> Result<()> {
        save::write_replacing(path.as_ref(), &self.to_luau()?)
    }
}

impl TilesetFile {
    /// The tileset as the text of a tileset file, in the canonical layout, as
    /// [`SceneFile::to_luau`] writes a scene.
    pub fn to_luau(&self) -> Result<String> {
        writer::write_tileset(self).map_err(Error::WriteScene)
    }

    /// Writes the tileset to the file at `path`, as [`TilesetFile::to_luau`] gives it, replacing
    /// the old one as [`SceneFile::write`] does.
    pub fn write(&self, path: impl AsRef<Path>) -> Result<()> {
        save::write_replacing(path.as_ref(), &self.to_luau()?)
    }
}

/// How deeply nodes may nest: how far below a scene's root a node may be, and how far below a
/// file's top its nodes may be. Files refuse to nest deeper, through their children or the
/// scenes their instances place, and [`Scene::add`] does too, so the engine never walks a tree
/// so deep it runs out of stack.
pub(crate) const MAX_NODE_DEPTH: usize = 200;

/// Where the files and images a scene uses come from as it's built: the scene files its
/// instances place, the tileset files its tilemaps use, the images of its sheets and the model
/// files of its model nodes, each by its path as the file names it, and the meshes of its mesh
/// nodes' shapes. [`FolderAssets`] reads them from a game's folder; a game can keep them loaded
/// instead.
pub trait Assets {
    fn texture(&mut self, path: &str) -> Result<Texture>;
    fn scene(&mut self, path: &str) -> Result<Rc<SceneFile>>;
    fn tileset(&mut self, path: &str) -> Result<Rc<TilesetFile>>;
    fn model(&mut self, path: &str) -> Result<Model>;
    /// A mesh of `data`'s triangles, as [`Context::create_mesh`] makes one.
    fn mesh(&mut self, data: &MeshData) -> Result<Mesh>;
}

/// [`Assets`] read from the files in a folder, each time they're asked for.
pub struct FolderAssets<'a> {
    pub ctx: &'a mut Context,
    pub folder: &'a Path,
}

impl Assets for FolderAssets<'_> {
    fn texture(&mut self, path: &str) -> Result<Texture> {
        self.ctx.load_texture(self.folder.join(path))
    }

    fn scene(&mut self, path: &str) -> Result<Rc<SceneFile>> {
        SceneFile::read(self.folder.join(path)).map(Rc::new)
    }

    fn tileset(&mut self, path: &str) -> Result<Rc<TilesetFile>> {
        TilesetFile::read(self.folder.join(path)).map(Rc::new)
    }

    fn model(&mut self, path: &str) -> Result<Model> {
        load_model(self.ctx, &self.folder.join(path))
    }

    fn mesh(&mut self, data: &MeshData) -> Result<Mesh> {
        self.ctx.create_mesh(data)
    }
}

/// The model file at `path`, as [`Context::load_model`] reads it, or without the engine's
/// `models` feature, an error saying so.
pub fn load_model(ctx: &mut Context, path: &Path) -> Result<Model> {
    #[cfg(feature = "models")]
    {
        ctx.load_model(path)
    }
    #[cfg(not(feature = "models"))]
    {
        let _ = ctx;
        Err(Error::Model {
            path: path.to_owned(),
            message: "reading models needs the engine built with its `models` feature".into(),
        })
    }
}

/// A scene loaded to play: a tree of nodes, which a game finds, moves, turns, changes, adds and
/// removes, and draws.
///
/// The root is a plain node holding the scene's props, with the file's nodes as its children.
/// Its rectangle, which the nodes at the top anchor and stretch to, is [`Scene::size`] at its
/// origin: the canvas's size, once the scene is drawn. Cloning a scene copies its nodes, sharing
/// the images they show.
#[derive(Clone, Debug)]
pub struct Scene {
    slots: Vec<Slot>,
    /// Slots whose nodes were removed, for new nodes.
    free: Vec<u32>,
    size: [f32; 2],
}

/// Where a node is kept, and how many nodes have been kept there before it, so an ID of a removed
/// node never reaches one added since.
#[derive(Clone, Debug)]
struct Slot {
    generation: u32,
    node: Option<Node>,
}

/// Which node of a [`Scene`] is meant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId {
    index: u32,
    generation: u32,
}

/// A node of a [`Scene`].
#[derive(Clone, Debug)]
pub struct Node {
    name: String,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    position: [f32; 2],
    rotation: f32,
    scale: [f32; 2],
    /// From the node's coordinates to its parent's, kept up to date with the three above, as
    /// though it had no anchor.
    local: Affine,
    anchor: Option<[f32; 2]>,
    /// The rest of where it is, for a node placed in 3D, in a 3D view.
    spatial: Option<Spatial>,
    visible: bool,
    tags: Vec<String>,
    props: Props,
    kind: Kind,
}

/// Where a node placed in 3D is, besides its position's `x` and `y` and its scale's.
#[derive(Clone, Copy, Debug)]
struct Spatial {
    placement: Placement3,
    /// From the node's coordinates to its parent's, kept up to date with its placement.
    local: Affine3,
}

/// What a node is, and what it shows.
#[derive(Clone, Debug)]
pub enum Kind {
    /// Nothing but a place.
    Node,
    Sprite(SpriteNode),
    Rect(RectNode),
    Text(TextNode),
    /// A rectangle that doesn't draw.
    Area(AreaNode),
    /// A grid of tiles, whose layers are its children of the kind [`Kind::TileLayer`].
    Tilemap(TilemapNode),
    /// A layer of the tilemap that's its parent.
    TileLayer(TileLayerNode),
    /// A rectangle showing a 3D world, whose nodes are its children.
    View3d(View3dNode),
    Mesh(MeshNode),
    Model(ModelNode),
    Billboard(BillboardNode),
}

/// What a 3D view shows, and how it's laid out, as an area is.
#[derive(Clone, Debug)]
pub struct View3dNode {
    pub size: [f32; 2],
    /// As a rect's.
    pub stretch: [f32; 2],
    /// As a sprite's.
    pub pivot: [f32; 2],
    pub camera: Camera3d,
    pub lighting: Lighting,
}

/// What a mesh node shows: its shape's mesh, showing a sprite or white, times its tint.
#[derive(Clone, Debug)]
pub struct MeshNode {
    pub mesh: Mesh,
    pub sprite: Option<Sprite>,
    /// The sheet its sprite is from, whose cells it shows with [`MeshNode::set_cell`] and clips.
    pub sheet: Option<SpriteSheet>,
    pub tint: Color,
}

/// What a model node shows.
#[derive(Clone, Debug)]
pub struct ModelNode {
    pub model: Model,
    pub tint: Color,
}

/// What a billboard node shows: a sprite standing upright, the middle of its bottom edge at the
/// node's origin, turned to face the camera.
#[derive(Clone, Debug)]
pub struct BillboardNode {
    pub sprite: Sprite,
    /// The sheet whose cells it shows, with [`BillboardNode::set_cell`] and clips.
    pub sheet: Option<SpriteSheet>,
    /// Its width and height in world units. A negative width mirrors it.
    pub size: [f32; 2],
    pub tint: Color,
}

/// What a sprite node shows.
#[derive(Clone, Debug)]
pub struct SpriteNode {
    pub sprite: Sprite,
    /// The sheet whose cells it shows, with [`SpriteNode::set_cell`] and clips.
    pub sheet: Option<SpriteSheet>,
    /// The width and height it's stretched to, before `stretch`.
    pub size: [f32; 2],
    /// As a rect's.
    pub stretch: [f32; 2],
    /// The point of the sprite, from its top-left corner, at the node's origin.
    pub pivot: [f32; 2],
    pub tint: Color,
}

/// What a rect node fills.
#[derive(Clone, Debug)]
pub struct RectNode {
    pub size: [f32; 2],
    /// The fractions of its parent's rectangle's width and height added to `size`, so it grows
    /// and shrinks with its parent, though never to less than nothing.
    pub stretch: [f32; 2],
    /// As a sprite's.
    pub pivot: [f32; 2],
    pub color: Color,
}

/// What a text node shows, laid out once and again whenever the text changes.
#[derive(Clone, Debug)]
pub struct TextNode {
    layout: TextLayout,
    text: String,
    wrap: Option<f32>,
    /// The point of the text at the node's origin, from the top of the edge or middle it's
    /// aligned by.
    pub pivot: [f32; 2],
    pub color: Color,
}

/// The rectangle an area node marks out.
#[derive(Clone, Debug)]
pub struct AreaNode {
    pub size: [f32; 2],
    /// As a rect's.
    pub stretch: [f32; 2],
    /// As a sprite's.
    pub pivot: [f32; 2],
}

/// A tilemap node's map, with its tiles' names and props from the tileset file. Its layers are
/// the layers of its children of the kind [`Kind::TileLayer`], in order.
#[derive(Clone, Debug)]
pub struct TilemapNode {
    pub map: Tilemap,
    kinds: Rc<TileKinds>,
}

/// The tiles of a tileset file, by name, as a [`Tileset`] numbers them.
#[derive(Debug)]
struct TileKinds {
    names: Vec<String>,
    ids: HashMap<String, TileId>,
    props: Vec<Props>,
}

/// Which layer of its tilemap a tile layer node is.
#[derive(Clone, Debug)]
pub struct TileLayerNode {
    pub layer: usize,
    /// The size of its map, which it covers.
    size: [f32; 2],
}

impl Kind {
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
            Self::View3d(_) => "view3d",
            Self::Mesh(_) => "mesh",
            Self::Model(_) => "model",
            Self::Billboard(_) => "billboard",
        }
    }

    /// Whether it only moves with its node, rather than turning and stretching too.
    fn only_moves(&self) -> bool {
        matches!(
            self,
            Self::Text(_) | Self::Tilemap(_) | Self::TileLayer(_) | Self::View3d(_)
        )
    }

    /// The sheet of the sprite it shows, whose cells it can show instead: a sprite's, a
    /// billboard's or a mesh's, if it's from a sheet.
    fn sheet(&self) -> Option<&SpriteSheet> {
        match self {
            Self::Sprite(sprite) => sprite.sheet.as_ref(),
            Self::Billboard(billboard) => billboard.sheet.as_ref(),
            Self::Mesh(mesh) => mesh.sheet.as_ref(),
            _ => None,
        }
    }

    /// Shows the sprite in `cell` of its sheet, if it shows a sprite: see [`Kind::sheet`].
    /// `None` if it's a kind that doesn't.
    pub fn set_cell(&mut self, cell: [u32; 2]) -> Option<Result<(), String>> {
        match self {
            Self::Sprite(sprite) => Some(sprite.set_cell(cell)),
            Self::Billboard(billboard) => Some(billboard.set_cell(cell)),
            Self::Mesh(mesh) => Some(mesh.set_cell(cell)),
            _ => None,
        }
    }
}

/// The sprite in `cell` of `sheet`, its column and row. Fails if there's no sheet, or it has no
/// such cell.
fn cell_of(sheet: Option<&SpriteSheet>, [column, row]: [u32; 2]) -> Result<Sprite, String> {
    let sheet = sheet.ok_or("the sprite isn't from a sheet")?;
    sheet.try_sprite(column, row)
}

impl MeshNode {
    /// Shows the sprite in `cell` of its sheet, its column and row. Fails if it has no sheet, or
    /// the sheet has no such cell.
    pub fn set_cell(&mut self, cell: [u32; 2]) -> Result<(), String> {
        self.sprite = Some(cell_of(self.sheet.as_ref(), cell)?);
        Ok(())
    }
}

impl BillboardNode {
    /// Shows the sprite in `cell` of its sheet, its column and row. Fails if it has no sheet, or
    /// the sheet has no such cell.
    pub fn set_cell(&mut self, cell: [u32; 2]) -> Result<(), String> {
        self.sprite = cell_of(self.sheet.as_ref(), cell)?;
        Ok(())
    }
}

/// "a node of the kind `kind`", as errors say it, such as "an area node", or "a plain node" for
/// the kind `node`.
pub(crate) fn a_node_of(kind: &str) -> String {
    if kind == "node" {
        return "a plain node".into();
    }
    let article = if kind.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    };
    format!("{article} {kind} node")
}

impl SpriteNode {
    /// Shows the sprite in `cell` of its sheet, its column and row. Fails if it has no sheet, or
    /// the sheet has no such cell.
    pub fn set_cell(&mut self, cell: [u32; 2]) -> Result<(), String> {
        self.sprite = cell_of(self.sheet.as_ref(), cell)?;
        Ok(())
    }
}

impl TextNode {
    /// `text` in `font`, wrapped between words to `wrap` if given, its lines lined up by
    /// `align`.
    pub fn new(text: impl Into<String>, font: &Font, wrap: Option<f32>, align: Align) -> Self {
        let text = text.into();
        Self {
            layout: font.layout_aligned(&text, wrap, align),
            text,
            wrap,
            pivot: [0.0, 0.0],
            color: Color::WHITE,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Changes the text, laying it out again unless it's the same.
    pub fn set_text(&mut self, text: impl Into<String>) {
        let text = text.into();
        if text != self.text {
            self.text = text;
            self.lay_out(self.align());
        }
    }

    /// How its lines line up, and which part of them is at the node's origin: their left
    /// edges, their middles or their right edges.
    pub fn align(&self) -> Align {
        self.layout.align()
    }

    pub fn set_align(&mut self, align: Align) {
        if align != self.align() {
            self.lay_out(align);
        }
    }

    fn lay_out(&mut self, align: Align) {
        let font = self.layout.font();
        self.layout = font.layout_aligned(&self.text, self.wrap, align);
    }

    /// The text laid out, as it draws.
    pub fn layout(&self) -> &TextLayout {
        &self.layout
    }
}

impl TilemapNode {
    /// The ID of the tile the tileset names `name`, if it has one.
    pub fn tile_id(&self, name: &str) -> Option<TileId> {
        self.kinds.ids.get(name).copied()
    }

    /// The name of the tile with the ID `id`.
    ///
    /// # Panics
    ///
    /// If the tileset has no such tile.
    pub fn tile_name(&self, id: TileId) -> &str {
        &self.kinds.names[id.0 as usize]
    }

    /// The props of the tile named `name`, if the tileset has one.
    pub fn tile_props(&self, name: &str) -> Option<&Props> {
        let id = self.tile_id(name)?;
        Some(&self.kinds.props[id.0 as usize])
    }

    /// The names of the tileset's tiles, in order.
    pub fn tile_names(&self) -> &[String] {
        &self.kinds.names
    }
}

impl Node {
    fn new(name: String, kind: Kind) -> Self {
        Self {
            name,
            parent: None,
            children: Vec::new(),
            position: [0.0, 0.0],
            rotation: 0.0,
            scale: [1.0, 1.0],
            local: Affine::IDENTITY,
            anchor: None,
            spatial: None,
            visible: true,
            tags: Vec::new(),
            props: Props::new(),
            kind,
        }
    }

    /// Its name, unique among its siblings. The root's is empty.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The node it's inside, unless it's the root.
    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    /// The nodes inside it, in the order they draw.
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    /// Where its origin is in its parent's coordinates: from the parent's origin, or with an
    /// anchor, from that point of the parent's rectangle.
    pub fn position(&self) -> [f32; 2] {
        self.position
    }

    pub fn set_position(&mut self, position: [f32; 2]) {
        self.position = position;
        self.update_local();
    }

    /// Radians, clockwise, about its origin.
    pub fn rotation(&self) -> f32 {
        self.rotation
    }

    pub fn set_rotation(&mut self, rotation: f32) {
        self.rotation = rotation;
        self.update_local();
    }

    /// How much it and its children are stretched along its own axes, about its origin.
    pub fn scale(&self) -> [f32; 2] {
        self.scale
    }

    pub fn set_scale(&mut self, scale: [f32; 2]) {
        self.scale = scale;
        self.update_local();
    }

    fn update_local(&mut self) {
        self.local = Affine::from_parts(self.position, self.rotation, self.scale);
        if let Some(spatial) = &mut self.spatial {
            let Placement3 {
                z,
                yaw,
                pitch,
                roll,
                scale_z,
            } = spatial.placement;
            let [x, y] = self.position;
            let [scale_x, scale_y] = self.scale;
            spatial.local = Affine3::from_translation([x, y, z])
                * Affine3::from_rotation_y(yaw)
                * Affine3::from_rotation_x(pitch)
                * Affine3::from_rotation_z(roll)
                * Affine3::from_scale([scale_x, scale_y, scale_z]);
        }
    }

    /// For a node placed in 3D, in a 3D view: its `z`, how it's turned and how much it's
    /// stretched along `z`, with [`Node::position`] and [`Node::scale`] the rest, along the view's
    /// world's axes. `None` for a node of the 2D scene.
    pub fn placement3(&self) -> Option<Placement3> {
        self.spatial.map(|spatial| spatial.placement)
    }

    /// Fails for a node of the 2D scene, which isn't placed in 3D.
    pub fn set_placement3(&mut self, placement: Placement3) -> Result<(), String> {
        let spatial = self
            .spatial
            .as_mut()
            .ok_or("the node isn't in a 3D view, so it isn't placed in 3D")?;
        spatial.placement = placement;
        self.update_local();
        Ok(())
    }

    /// For a node placed in 3D, from its coordinates to its parent's: stretched, then rolled,
    /// pitched and turned, then moved.
    pub fn local_3d(&self) -> Option<Affine3> {
        self.spatial.map(|spatial| spatial.local)
    }

    /// One of its numbers, as a clip's track changes it.
    fn number(&self, property: Property) -> f32 {
        let placement = self.placement3().unwrap_or_default();
        match property {
            Property::X => self.position[0],
            Property::Y => self.position[1],
            Property::Rotation => self.rotation,
            Property::ScaleX => self.scale[0],
            Property::ScaleY => self.scale[1],
            Property::Z => placement.z,
            Property::Yaw => placement.yaw,
            Property::Pitch => placement.pitch,
            Property::Roll => placement.roll,
            Property::ScaleZ => placement.scale_z,
        }
    }

    fn set_number(&mut self, property: Property, value: f32) {
        let mut placement = self.placement3().unwrap_or_default();
        match property {
            Property::X => self.position[0] = value,
            Property::Y => self.position[1] = value,
            Property::Rotation => self.rotation = value,
            Property::ScaleX => self.scale[0] = value,
            Property::ScaleY => self.scale[1] = value,
            Property::Z => placement.z = value,
            Property::Yaw => placement.yaw = value,
            Property::Pitch => placement.pitch = value,
            Property::Roll => placement.roll = value,
            Property::ScaleZ => placement.scale_z = value,
        }
        if let Some(spatial) = &mut self.spatial {
            spatial.placement = placement;
        }
        self.update_local();
    }

    /// The point of its parent's rectangle its position is from, as fractions of the
    /// rectangle's width and height from its top-left corner, or `None` to place it from its
    /// parent's origin. The rectangle is what the parent covers, or for a plain node, its own
    /// parent's, carried along with it as it moves, turns and stretches; the root's is the
    /// scene's size, at its origin. The root's anchor does nothing, since it has no parent.
    pub fn anchor(&self) -> Option<[f32; 2]> {
        self.anchor
    }

    pub fn set_anchor(&mut self, anchor: Option<[f32; 2]>) {
        self.anchor = anchor;
    }

    /// Whether it and its children draw.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    /// Data for the game.
    pub fn props(&self) -> &Props {
        &self.props
    }

    pub fn props_mut(&mut self) -> &mut Props {
        &mut self.props
    }

    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    pub fn kind_mut(&mut self) -> &mut Kind {
        &mut self.kind
    }

    /// The width and height of a sprite, rect, text, area, tilemap, tile layer or 3D view, before
    /// stretching with its parent. A plain node has none, and nor do nodes in 3D views.
    pub fn size(&self) -> Option<[f32; 2]> {
        match &self.kind {
            Kind::Node | Kind::Mesh(_) | Kind::Model(_) | Kind::Billboard(_) => None,
            Kind::Sprite(sprite) => Some(sprite.size),
            Kind::Rect(rect) => Some(rect.size),
            Kind::Text(text) => Some(text.layout.size()),
            Kind::Area(area) => Some(area.size),
            Kind::Tilemap(map) => {
                let bounds = map.map.bounds();
                Some([bounds.w, bounds.h])
            }
            Kind::TileLayer(layer) => Some(layer.size),
            Kind::View3d(view) => Some(view.size),
        }
    }

    /// The rectangle the node covers in its own coordinates, its pivot at the origin, inside a
    /// parent whose rectangle is `outer`, which it may stretch with. A plain node covers nothing,
    /// and nor do nodes in 3D views.
    fn rect_in(&self, outer: Rect) -> Option<Rect> {
        let size = self.size()?;
        let (stretch, [x, y]) = match &self.kind {
            Kind::Sprite(sprite) => (sprite.stretch, sprite.pivot),
            Kind::Rect(rect) => (rect.stretch, rect.pivot),
            Kind::Area(area) => (area.stretch, area.pivot),
            Kind::View3d(view) => (view.stretch, view.pivot),
            Kind::Text(text) => {
                let [x, y] = text.pivot;
                let x = x + size[0] * text.align().fraction();
                ([0.0, 0.0], [x, y])
            }
            Kind::Node
            | Kind::Tilemap(_)
            | Kind::TileLayer(_)
            | Kind::Mesh(_)
            | Kind::Model(_)
            | Kind::Billboard(_) => ([0.0, 0.0], [0.0, 0.0]),
        };
        let [width, height] = if stretch == [0.0, 0.0] {
            size
        } else {
            [
                (size[0] + stretch[0] * outer.w).max(0.0),
                (size[1] + stretch[1] * outer.h).max(0.0),
            ]
        };
        Some(Rect::new(-x, -y, width, height))
    }

    /// From its coordinates to its parent's, inside a parent whose rectangle is `outer`: its
    /// scale, then its rotation, then its position, from its anchor.
    fn local_in(&self, outer: Rect) -> Affine {
        let mut local = self.local;
        if let Some([x, y]) = self.anchor {
            local.translation[0] += outer.x + x * outer.w;
            local.translation[1] += outer.y + y * outer.h;
        }
        local
    }
}

/// Where a node is laid out in its scene.
#[derive(Clone, Copy)]
struct Placed {
    /// From the node's coordinates to the scene's.
    transform: Affine,
    /// What it covers in its own coordinates, if anything.
    rect: Option<Rect>,
    /// The rectangle the nodes inside it anchor and stretch to, in its coordinates: what it
    /// covers, or for a plain node, its parent's, carried along with it.
    inner: Rect,
}

impl Placed {
    /// Where `node` is, as a child of the node placed here.
    fn child(&self, node: &Node) -> Self {
        let local = node.local_in(self.inner);
        let rect = node.rect_in(self.inner);
        Self {
            transform: self.transform * local,
            rect,
            inner: rect.unwrap_or(self.inner),
        }
    }
}

impl Scene {
    /// The scene `file` describes, its text in `font`, loading the scene files its instances
    /// place, the tilesets its tilemaps use and its sheets' images from `assets`. Errors name the
    /// file `path`, such as a sprite outside its sheet.
    pub fn new(file: &SceneFile, path: &str, font: &Font, assets: &mut dyn Assets) -> Result<Self> {
        let mut expand = Expand {
            assets: &mut *assets,
            stack: vec![path.to_owned()],
            sheets: BTreeMap::new(),
        };
        let nodes = expand.file(file, path)?;
        let sheets = expand.sheets;
        let mut root = Node::new(String::new(), Kind::Node);
        root.props = file.props.clone();
        let mut scene = Self {
            slots: vec![Slot {
                generation: 0,
                node: Some(root),
            }],
            free: Vec::new(),
            size: [0.0, 0.0],
        };
        let mut build = Build {
            path,
            font,
            assets,
            sheet_defs: sheets,
            sheets: HashMap::new(),
            tilesets: HashMap::new(),
            meshes: Vec::new(),
            models: HashMap::new(),
        };
        let root = scene.root();
        for node in &nodes {
            scene.build(&mut build, node, root, "", false)?;
        }
        Ok(scene)
    }

    /// Reads the scene file at `path` in `folder`, and the files and images it uses from
    /// `folder`. Its size is the canvas's.
    pub fn load(
        ctx: &mut Context,
        folder: impl AsRef<Path>,
        path: impl AsRef<Path>,
    ) -> Result<Self> {
        let folder = folder.as_ref();
        let path = path.as_ref();
        let file = SceneFile::read(folder.join(path))?;
        let font = ctx.default_font().clone();
        let size = ctx.canvas_size();
        let mut assets = FolderAssets { ctx, folder };
        let mut scene = Self::new(&file, &path.to_string_lossy(), &font, &mut assets)?;
        scene.set_size(size);
        Ok(scene)
    }

    /// The width and height of the root's rectangle, at its origin, which the nodes at the top
    /// of the scene anchor and stretch to. Drawing the scene makes it the canvas's size, and
    /// [`Scene::load`] starts it there; [`Scene::new`] starts it at 0 by 0.
    pub fn size(&self) -> [f32; 2] {
        self.size
    }

    pub fn set_size(&mut self, size: [f32; 2]) {
        self.size = size;
    }

    /// Adds the node `def` describes, and its children, at the end of `parent`'s children, which
    /// is a 3D view or in one if `in_3d`.
    fn build(
        &mut self,
        build: &mut Build,
        def: &NodeDef,
        parent: NodeId,
        path: &str,
        in_3d: bool,
    ) -> Result<()> {
        let path = if path.is_empty() {
            def.name.clone()
        } else {
            format!("{path}/{}", def.name)
        };
        check_3d(def, in_3d).map_err(|err| build.error(&path, &err))?;
        let kind = match &def.kind {
            KindDef::TileLayer(layer) => {
                let Some(Kind::Tilemap(map)) = self.get_mut(parent).map(|node| &mut node.kind)
                else {
                    return Err(build.error(&path, "a tile layer must be the child of a tilemap"));
                };
                Kind::TileLayer(fill_layer(map, layer).map_err(|err| build.error(&path, &err))?)
            }
            kind => build.kind(kind, &path)?,
        };
        let mut node = Node::new(def.name.clone(), kind);
        node.position = [def.x, def.y];
        node.anchor = def.anchor;
        node.rotation = def.rotation;
        node.scale = [def.scale_x, def.scale_y];
        // A plain node from a file of its own may be placed in a view without saying so.
        node.spatial = in_3d.then(|| Spatial {
            placement: def.placement3.unwrap_or_default(),
            local: Affine3::IDENTITY,
        });
        node.update_local();
        node.visible = def.visible;
        node.tags = def.tags.clone();
        node.props = def.props.clone();
        let inside = in_3d || matches!(def.kind, KindDef::View3d(_));
        let id = self.attach(node, parent);
        for child in &def.children {
            self.build(build, child, id, &path, inside)?;
        }
        Ok(())
    }

    /// Keeps `node` in a free slot, at the end of `parent`'s children, and returns its ID.
    fn attach(&mut self, mut node: Node, parent: NodeId) -> NodeId {
        node.parent = Some(parent);
        let id = match self.free.pop() {
            Some(index) => {
                let slot = &mut self.slots[index as usize];
                slot.node = Some(node);
                NodeId {
                    index,
                    generation: slot.generation,
                }
            }
            None => {
                let index =
                    u32::try_from(self.slots.len()).expect("a scene holds at most 2^32 nodes");
                self.slots.push(Slot {
                    generation: 0,
                    node: Some(node),
                });
                NodeId {
                    index,
                    generation: 0,
                }
            }
        };
        self.node_mut(parent).children.push(id);
        id
    }

    /// The node at the top of the tree, which holds the scene's props.
    pub fn root(&self) -> NodeId {
        NodeId {
            index: 0,
            generation: 0,
        }
    }

    /// The node `id`, if it's in the scene: it hasn't been removed, and isn't from another scene.
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        let slot = self.slots.get(id.index as usize)?;
        slot.node
            .as_ref()
            .filter(|_| slot.generation == id.generation)
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        let slot = self.slots.get_mut(id.index as usize)?;
        slot.node
            .as_mut()
            .filter(|_| slot.generation == id.generation)
    }

    /// Whether `id` is a node of the scene, which hasn't been removed.
    pub fn contains_node(&self, id: NodeId) -> bool {
        self.get(id).is_some()
    }

    /// # Panics
    ///
    /// If the node isn't in the scene: see [`Scene::get`].
    pub fn node(&self, id: NodeId) -> &Node {
        self.get(id).expect("the node is in the scene")
    }

    /// # Panics
    ///
    /// If the node isn't in the scene: see [`Scene::get`].
    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        self.get_mut(id).expect("the node is in the scene")
    }

    /// The node `path` leads to from `from`, such as `"mill/sails"`, a child of a child of
    /// `from`, if there is one.
    pub fn find(&self, from: NodeId, path: &str) -> Option<NodeId> {
        path.split('/').try_fold(from, |id, name| {
            self.node(id)
                .children
                .iter()
                .copied()
                .find(|&child| self.node(child).name == name)
        })
    }

    /// `id` and every node inside it, in the order they draw.
    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut found = Vec::new();
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            found.push(id);
            stack.extend(self.node(id).children.iter().rev());
        }
        found
    }

    /// How far `id` is below the scene's root: 0 for the root itself.
    fn depth(&self, id: NodeId) -> usize {
        let mut depth = 0;
        let mut at = id;
        while let Some(parent) = self.node(at).parent {
            at = parent;
            depth += 1;
        }
        depth
    }

    /// How far the deepest node inside `id` is below it: 0 for a node with nothing inside.
    fn height(&self, id: NodeId) -> usize {
        self.node(id)
            .children
            .iter()
            .map(|&child| self.height(child) + 1)
            .max()
            .unwrap_or(0)
    }

    /// `from` and the nodes inside it with the tag `tag`, in the order they draw.
    pub fn tagged(&self, from: NodeId, tag: &str) -> Vec<NodeId> {
        let mut found = self.descendants(from);
        found.retain(|&id| self.node(id).has_tag(tag));
        found
    }

    /// The names from the root to `id`, separated by `/`, such as `"mill/sails"`, which
    /// [`Scene::find`] finds from the root. The root's is empty.
    pub fn path(&self, id: NodeId) -> String {
        let mut names = Vec::new();
        let mut at = id;
        while let Some(parent) = self.node(at).parent {
            names.push(self.node(at).name.as_str());
            at = parent;
        }
        names.reverse();
        names.join("/")
    }

    /// How errors name the node: by its path, or as the root.
    pub(crate) fn describe(&self, id: NodeId) -> String {
        if id == self.root() {
            "the scene's root".into()
        } else {
            format!("node `{}`", self.path(id))
        }
    }

    /// Poses `id` and the nodes inside it as `clip` has them `time` seconds after it started
    /// (see [`Clip::time_in`]): each track sets a property of the node its path leads to from
    /// `id` to the value its keys give then. Properties the clip has no track for keep what they
    /// have, so a game that poses with several clips poses first with one that has tracks for
    /// everything the others change.
    ///
    /// With a `weight` below 1, down to 0, numbers move only that fraction of the way from what
    /// they are to the clip's, and visibility and cells change only from a weight of a half. So
    /// posing with one clip, then with another at a weight rising from 0 to 1, crossfades from
    /// the first to the second.
    ///
    /// Fails, changing nothing, if a track's node isn't there, or a track of cells is of a node
    /// that isn't a sprite from a sheet with those cells.
    pub fn pose(&mut self, id: NodeId, clip: &Clip, time: f32, weight: f32) -> Result<(), String> {
        let mut targets = Vec::with_capacity(clip.tracks.len());
        for track in &clip.tracks {
            let target = if track.node.is_empty() {
                id
            } else {
                self.find(id, &track.node).ok_or_else(|| {
                    format!(
                        "there's no node `{}` in {} for the clip to pose",
                        track.node,
                        self.describe(id)
                    )
                })?
            };
            match &track.keys {
                Keys::Cell(keys) => self.check_cells(target, keys)?,
                Keys::Number { property, .. } => self.check_property(target, *property)?,
                Keys::Visible(_) => {}
            }
            targets.push(target);
        }
        let weight = weight.clamp(0.0, 1.0);
        if weight == 0.0 {
            return Ok(());
        }
        let time = clip.time_in(time);
        let steps = weight >= 0.5;
        for (track, target) in clip.tracks.iter().zip(targets) {
            let node = self.node_mut(target);
            match &track.keys {
                Keys::Number {
                    property,
                    interpolation,
                    keys,
                } => {
                    if let Some(value) = clip::number(keys, *interpolation, time) {
                        let value = if weight == 1.0 {
                            value
                        } else {
                            let from = node.number(*property);
                            from + (value - from) * weight
                        };
                        node.set_number(*property, value);
                    }
                }
                Keys::Visible(keys) => {
                    if steps && let Some(visible) = clip::step(keys, time) {
                        node.visible = visible;
                    }
                }
                Keys::Cell(keys) => {
                    if steps && let Some(cell) = clip::step(keys, time) {
                        node.kind.set_cell(cell).transpose()?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Fails unless `id` has `property`: `rotation` only in the 2D scene, and `z`, `yaw`, `pitch`,
    /// `roll` and `scale_z` only in 3D views.
    fn check_property(&self, id: NodeId, property: Property) -> Result<(), String> {
        let name = property.name();
        match self.node(id).spatial {
            Some(_) if !property.is_3d() => Err(format!(
                "{} is placed in 3D, where it turns by yaw, pitch and roll, so it has no {name}",
                self.describe(id)
            )),
            None if !property.is_2d() => Err(format!(
                "{} isn't in a 3D view, so it has no {name}",
                self.describe(id)
            )),
            _ => Ok(()),
        }
    }

    /// Fails unless `id` is a sprite, billboard or mesh node from a sheet with every cell of
    /// `keys`.
    fn check_cells(&self, id: NodeId, keys: &[(f32, [u32; 2])]) -> Result<(), String> {
        let kind = &self.node(id).kind;
        let sheet = match kind.sheet() {
            Some(sheet) => sheet,
            None if matches!(kind, Kind::Sprite(_) | Kind::Billboard(_) | Kind::Mesh(_)) => {
                return Err(format!(
                    "{}'s sprite isn't from a sheet, so it has no cells",
                    self.describe(id)
                ));
            }
            None => {
                return Err(format!(
                    "{} is {}, which has no cell",
                    self.describe(id),
                    a_node_of(kind.name())
                ));
            }
        };
        for &(_, [column, row]) in keys {
            sheet
                .try_sprite(column, row)
                .map_err(|err| format!("{}: {err}", self.describe(id)))?;
        }
        Ok(())
    }

    /// Where the node is laid out: from the root down, each node placed in its parent.
    fn placed(&self, id: NodeId) -> Placed {
        let node = self.node(id);
        match node.parent {
            Some(parent) => self.placed(parent).child(node),
            None => Placed {
                transform: node.local,
                rect: None,
                inner: Rect::new(0.0, 0.0, self.size[0], self.size[1]),
            },
        }
    }

    /// From the node's coordinates to the scene's: its own transform, from its anchor, then its
    /// parent's, and so on up to the root.
    pub fn transform(&self, id: NodeId) -> Affine {
        self.placed(id).transform
    }

    /// The rectangle the node covers in its own coordinates, with its pivot at the origin, as
    /// it's drawn: stretched with its parent, and for text, aligned. A plain node covers nothing.
    pub fn rect(&self, id: NodeId) -> Option<Rect> {
        self.placed(id).rect
    }

    /// Where what the node shows is placed in the scene, and what it covers: by
    /// [`Scene::transform`], or only moved by it, for text and tilemaps, which don't turn or
    /// stretch.
    fn placement(&self, id: NodeId) -> Option<(Affine, Rect)> {
        let placed = self.placed(id);
        let rect = placed.rect?;
        let transform = if self.node(id).kind.only_moves() {
            Affine::from_translation(placed.transform.translation)
        } else {
            placed.transform
        };
        Some((transform, rect))
    }

    /// The smallest axis-aligned rectangle in the scene's coordinates around what the node
    /// covers, if it covers anything: see [`Scene::rect`]. A scene drawn without a camera is in
    /// canvas pixels, so this is where the node is on the canvas.
    pub fn bounds(&self, id: NodeId) -> Option<Rect> {
        let (transform, rect) = self.placement(id)?;
        Some(transform.bounds(rect))
    }

    /// Whether `point`, in the scene's coordinates, is inside what the node covers, turned and
    /// stretched as it's drawn. Nothing is inside a plain node, or one squashed flat.
    pub fn contains(&self, id: NodeId, point: [f32; 2]) -> bool {
        let Some((transform, rect)) = self.placement(id) else {
            return false;
        };
        let Some(inverse) = transform.inverse() else {
            return false;
        };
        rect.contains(inverse.apply(point))
    }

    /// The 3D view a node placed in 3D is in, if it is.
    pub fn view_of(&self, id: NodeId) -> Option<NodeId> {
        let mut at = id;
        while self.node(at).spatial.is_some() {
            at = self.node(at).parent?;
        }
        (at != id && matches!(self.node(at).kind, Kind::View3d(_))).then_some(at)
    }

    /// For a node placed in 3D, from its coordinates to its view's world's: its own transform,
    /// then its parent's, and so on up to its view. `None` for a node of the 2D scene.
    pub fn transform_3d(&self, id: NodeId) -> Option<Affine3> {
        let node = self.node(id);
        let local = node.local_3d()?;
        let parent = node.parent?;
        match self.transform_3d(parent) {
            Some(above) => Some(above * local),
            None => Some(local),
        }
    }

    /// The box in its view's world around what a node placed in 3D shows, if it shows anything:
    /// a mesh's or a model's, moved as it's drawn, or the flat box a billboard stands in, facing
    /// its view's camera. A plain node shows nothing, and nor does a node of the 2D scene.
    pub fn bounds_3d(&self, id: NodeId) -> Option<Bounds3> {
        let transform = self.transform_3d(id)?;
        match &self.node(id).kind {
            Kind::Mesh(mesh) => Some(mesh.mesh.bounds()?.transformed(&transform)),
            Kind::Model(model) => Some(model.model.bounds()?.transformed(&transform)),
            Kind::Billboard(billboard) => {
                let Kind::View3d(view) = &self.node(self.view_of(id)?).kind else {
                    return None;
                };
                let (right, _) = view.camera.billboard_axes();
                let [width, height] = billboard.size;
                let across = scale3(right, width / 2.0);
                let foot = transform.translation;
                let head = add3(foot, [0.0, height, 0.0]);
                Bounds3::from_points([
                    add3(foot, across),
                    add3(foot, scale3(across, -1.0)),
                    add3(head, across),
                    add3(head, scale3(across, -1.0)),
                ])
            }
            _ => None,
        }
    }

    /// The ray from the camera of the 3D view `view` through `point`, in the scene's coordinates,
    /// along which everything that shows there lies, if the point is in the view. `None` if it
    /// isn't, or `view` isn't a 3D view.
    pub fn ray(&self, view: NodeId, point: [f32; 2]) -> Option<Ray> {
        let Kind::View3d(node) = &self.node(view).kind else {
            return None;
        };
        let rect = self.bounds(view)?;
        rect.contains(point).then(|| {
            let [x, y] = [point[0] - rect.x, point[1] - rect.y];
            node.camera.ray([x, y], [rect.w, rect.h])
        })
    }

    /// The nodes of the 3D view `view` under `point`, in the scene's coordinates, nearest first:
    /// those that show, whose boxes (see [`Scene::bounds_3d`]) the ray through the point enters.
    /// Nothing is under a point outside the view, or if `view` isn't a view that shows.
    pub fn pick(&self, view: NodeId, point: [f32; 2]) -> Vec<NodeId> {
        if !self.is_shown(view) {
            return Vec::new();
        }
        let Some(ray) = self.ray(view, point) else {
            return Vec::new();
        };
        let mut hits = Vec::new();
        let mut stack: Vec<NodeId> = self.node(view).children.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            let node = self.node(id);
            if !node.visible {
                continue;
            }
            if let Some(distance) = self.bounds_3d(id).and_then(|bounds| ray.hit_box(&bounds)) {
                hits.push((distance, id));
            }
            stack.extend(node.children.iter().rev());
        }
        // Stable, so nodes as near as each other are in the order they draw.
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        hits.into_iter().map(|(_, id)| id).collect()
    }

    /// Whether `id` and every node it's inside are visible.
    fn is_shown(&self, id: NodeId) -> bool {
        let mut at = Some(id);
        while let Some(id) = at {
            let node = self.node(id);
            if !node.visible {
                return false;
            }
            at = node.parent;
        }
        true
    }

    /// The tilemap a tilemap node holds, or the one a tile layer node is a layer of, with the
    /// tilemap node's ID.
    pub fn tilemap(&self, id: NodeId) -> Option<(NodeId, &TilemapNode)> {
        let id = match self.node(id).kind {
            Kind::Tilemap(_) => id,
            Kind::TileLayer(_) => self.node(id).parent?,
            _ => return None,
        };
        match &self.node(id).kind {
            Kind::Tilemap(map) => Some((id, map)),
            _ => None,
        }
    }

    /// The name of the tile in cell `cell` of the tile layer node `layer`, if it holds one.
    ///
    /// # Panics
    ///
    /// If `layer` isn't a tile layer node.
    pub fn tile(&self, layer: NodeId, cell: [i32; 2]) -> Option<&str> {
        let Kind::TileLayer(TileLayerNode { layer: index, .. }) = self.node(layer).kind else {
            panic!("the node is a tile layer");
        };
        let (_, map) = self.tilemap(layer)?;
        let id = map.map.get(index, cell)?;
        Some(map.tile_name(id))
    }

    /// Puts the tile named `name` in cell `cell` of the tile layer node `layer`, or with `None`,
    /// empties the cell. Fails if the cell is outside the map or the tileset has no such tile.
    ///
    /// # Panics
    ///
    /// If `layer` isn't a tile layer node.
    pub fn set_tile(
        &mut self,
        layer: NodeId,
        cell: [i32; 2],
        name: Option<&str>,
    ) -> Result<(), String> {
        let Kind::TileLayer(TileLayerNode { layer: index, .. }) = self.node(layer).kind else {
            panic!("the node is a tile layer");
        };
        let Some(parent) = self.node(layer).parent else {
            return Err("the layer isn't in a tilemap".into());
        };
        let Kind::Tilemap(map) = &mut self.node_mut(parent).kind else {
            return Err("the layer isn't in a tilemap".into());
        };
        let id = match name {
            Some(name) => Some(
                map.tile_id(name)
                    .ok_or_else(|| format!("there's no tile `{name}` in the tilemap's tileset"))?,
            ),
            None => None,
        };
        if !map.map.contains(cell) {
            return Err(format!(
                "tile {cell:?} is outside a {}x{} map",
                map.map.columns(),
                map.map.rows()
            ));
        }
        map.map.set(index, cell, id);
        Ok(())
    }

    /// Adds a copy of the node `from` of `source`, and everything inside it, at the end of
    /// `parent`'s children, named `name`, or the name it has, and returns the copy's ID. Copying a
    /// scene's root needs a name, since the root has none.
    ///
    /// Fails if the name isn't one a node can have or one of `parent`'s children has it already,
    /// if `from` is a tile layer, which only its tilemap can hold, if `from` is placed in 3D but
    /// `parent` isn't a 3D view or in one, or the other way round, or if the copy would place a
    /// node deeper than scenes hold, 200 levels below their root.
    pub fn add(
        &mut self,
        parent: NodeId,
        source: &Scene,
        from: NodeId,
        name: Option<&str>,
    ) -> Result<NodeId, String> {
        let copied = source.node(from);
        if let Kind::TileLayer(_) = copied.kind {
            return Err("a tile layer can only be copied with its tilemap".into());
        }
        let into_3d = self.holds_3d(parent);
        if into_3d != copied.spatial.is_some() {
            let what = a_node_of(copied.kind.name());
            return Err(if into_3d {
                format!("{what} of the 2D scene can't be copied into a 3D view")
            } else {
                format!("{what} placed in 3D can only be copied into a 3D view")
            });
        }
        let name = name.unwrap_or(&copied.name);
        if name.is_empty() {
            return Err("copying a scene's root takes a name for the copy".into());
        }
        if name.contains('/') {
            return Err(format!(
                "`{name}` can't be a node's name: `/` separates the names in a path"
            ));
        }
        if self.find(parent, name).is_some() {
            return Err(format!(
                "there's a node named `{name}` there already, but siblings' names are unique"
            ));
        }
        if self.depth(parent) + 1 + source.height(from) > MAX_NODE_DEPTH {
            return Err(format!(
                "nodes would nest more than {MAX_NODE_DEPTH} deep, too deep to add"
            ));
        }
        let id = self.copy(parent, source, from);
        self.node_mut(id).name = name.to_owned();
        Ok(id)
    }

    /// Whether the nodes inside `id` are placed in 3D: it's a 3D view, or it's in one.
    fn holds_3d(&self, id: NodeId) -> bool {
        let node = self.node(id);
        node.spatial.is_some() || matches!(node.kind, Kind::View3d(_))
    }

    /// Adds a copy of `from` of `source`, and everything inside it, at the end of `parent`'s
    /// children.
    fn copy(&mut self, parent: NodeId, source: &Scene, from: NodeId) -> NodeId {
        let original = source.node(from);
        let node = Node {
            children: Vec::new(),
            ..original.clone()
        };
        let id = self.attach(node, parent);
        for &child in &original.children {
            self.copy(id, source, child);
        }
        id
    }

    /// A scene holding only a copy of `id` and everything inside it, under a new root, for
    /// adding a copy of part of a scene to itself.
    pub fn copy_of(&self, id: NodeId) -> (Scene, NodeId) {
        let mut scene = Scene {
            slots: vec![Slot {
                generation: 0,
                node: Some(Node::new(String::new(), Kind::Node)),
            }],
            free: Vec::new(),
            size: self.size,
        };
        let root = scene.root();
        let copy = scene.copy(root, self, id);
        (scene, copy)
    }

    /// Takes `id`, and everything inside it, out of the scene. Their IDs then lead nowhere: see
    /// [`Scene::get`]. Fails for the root, which can't be removed.
    pub fn remove(&mut self, id: NodeId) -> Result<(), String> {
        let Some(parent) = self.node(id).parent else {
            return Err("a scene's root can't be removed".into());
        };
        self.node_mut(parent).children.retain(|&child| child != id);
        for removed in self.descendants(id) {
            let slot = &mut self.slots[removed.index as usize];
            slot.node = None;
            slot.generation = slot.generation.wrapping_add(1);
            self.free.push(removed.index);
        }
        Ok(())
    }

    /// Draws the whole scene, first making its size the canvas's.
    pub fn draw(&mut self, canvas: &mut Canvas) {
        self.draw_node(canvas, self.root());
    }

    /// Draws `id` and the nodes inside it where they are in the scene, each before its children,
    /// first making the scene's size the canvas's. Nothing is drawn where it's off the canvas, or
    /// where a node, or a node it's inside, isn't visible.
    pub fn draw_node(&mut self, canvas: &mut Canvas, id: NodeId) {
        self.size = [canvas.width(), canvas.height()];
        self.draw_from(canvas, id);
    }

    /// Draws the whole scene laid out as it would be on a canvas `size` in size, whatever the
    /// size of `canvas`, making that its size: as a tool shows a screen made for the game's canvas
    /// in a view of another size.
    pub fn draw_with_size(&mut self, canvas: &mut Canvas, size: [f32; 2]) {
        self.size = size;
        self.draw_from(canvas, self.root());
    }

    /// Draws `id` and the nodes inside it, laid out at the scene's size.
    fn draw_from(&mut self, canvas: &mut Canvas, id: NodeId) {
        let mut at = id;
        while let Some(above) = self.node(at).parent {
            if !self.node(above).visible {
                return;
            }
            at = above;
        }
        let visible = canvas.visible_area();
        self.draw_tree(canvas, id, &self.placed(id), visible);
    }

    /// Draws `id`, placed at `placed`, and the nodes inside it.
    fn draw_tree(&self, canvas: &mut Canvas, id: NodeId, placed: &Placed, visible: Rect) {
        let node = self.node(id);
        if !node.visible {
            return;
        }
        let transform = placed.transform;
        if let Some(rect) = placed.rect {
            match &node.kind {
                Kind::Sprite(sprite) => {
                    if transform.bounds(rect).overlaps(&visible) {
                        canvas.draw_sprite_transformed(
                            &sprite.sprite,
                            rect,
                            transform,
                            sprite.tint,
                        );
                    }
                }
                Kind::Rect(fill) => {
                    if transform.bounds(rect).overlaps(&visible) {
                        canvas.fill_rect_transformed(rect, transform, fill.color);
                    }
                }
                Kind::Text(text) => {
                    // Text moves with its node, but doesn't turn or stretch.
                    let [x, y] = transform.translation;
                    let [x, y] = [x + rect.x, y + rect.y];
                    if Rect::new(x, y, rect.w, rect.h).overlaps(&visible) {
                        canvas.draw_text_layout(&text.layout, .., [x, y], text.color);
                    }
                }
                Kind::TileLayer(layer) => {
                    // Its tilemap is its parent, and only culls by cell.
                    if let Some(Kind::Tilemap(map)) = node
                        .parent
                        .and_then(|parent| self.get(parent))
                        .map(|p| &p.kind)
                    {
                        map.map
                            .draw_layer_at(canvas, layer.layer, transform.translation);
                    }
                }
                Kind::View3d(view) => {
                    // A view moves with its node, but doesn't turn or stretch, and its children
                    // are its world's.
                    let [x, y] = transform.translation;
                    let shown = Rect::new(x + rect.x, y + rect.y, rect.w, rect.h);
                    if shown.overlaps(&visible) {
                        let mut world = canvas.begin_3d(View3d {
                            camera: view.camera,
                            rect: shown,
                            lighting: view.lighting,
                        });
                        for &child in &node.children {
                            self.draw_3d(&mut world, child, Affine3::IDENTITY);
                        }
                    }
                    return;
                }
                Kind::Node
                | Kind::Area(_)
                | Kind::Tilemap(_)
                | Kind::Mesh(_)
                | Kind::Model(_)
                | Kind::Billboard(_) => {}
            }
        }
        for &child in &node.children {
            let child_placed = placed.child(self.node(child));
            self.draw_tree(canvas, child, &child_placed, visible);
        }
    }

    /// Draws `id`, a node placed in 3D in a view, and the nodes inside it, inside a node placed
    /// by `parent`.
    fn draw_3d(&self, world: &mut Canvas3d, id: NodeId, parent: Affine3) {
        let node = self.node(id);
        if !node.visible {
            return;
        }
        let transform = parent * node.local_3d().unwrap_or(Affine3::IDENTITY);
        match &node.kind {
            Kind::Mesh(mesh) => {
                world.draw_mesh(&mesh.mesh, transform, mesh.sprite.as_ref(), mesh.tint);
            }
            Kind::Model(model) => world.draw_model(&model.model, transform, model.tint),
            Kind::Billboard(billboard) => world.draw_billboard(
                &billboard.sprite,
                transform.translation,
                billboard.size,
                billboard.tint,
            ),
            _ => {}
        }
        for &child in &node.children {
            self.draw_3d(world, child, transform);
        }
    }
}

/// Fails unless the node `def` can be where it is, in a 3D view if `in_3d`, as its instances have
/// put it: only views hold meshes, models and billboards, and views hold only them, plain nodes
/// and the scenes instances place, which are placed in 3D, without a `rotation` or an `anchor`.
fn check_3d(def: &NodeDef, in_3d: bool) -> Result<(), String> {
    let kind = def.kind.name();
    if in_3d {
        if !file::can_be_3d(kind) {
            return Err(format!(
                "{} can't be in a 3D view, which holds only mesh, model, billboard and plain \
                 nodes, and instances of scenes of them",
                a_node_of(kind)
            ));
        }
        if def.placement3.is_none() && (def.rotation != 0.0 || def.anchor.is_some()) {
            return Err(
                "a node in a 3D view turns by `yaw`, `pitch` and `roll`, and has no \
                        `rotation` or `anchor`"
                    .into(),
            );
        }
    } else if def.kind.is_3d() {
        return Err(format!(
            "{} is only in a 3D view: put it inside a view3d node",
            a_node_of(kind)
        ));
    } else if def.placement3.is_some() {
        return Err(
            "only nodes in 3D views are placed in 3D, with `z`, `yaw`, `pitch`, `roll` \
                    and `scale_z`"
                .into(),
        );
    }
    Ok(())
}

/// Fills a new layer of `map` with the tiles `layer` describes, and returns the layer node.
fn fill_layer(map: &mut TilemapNode, layer: &TileLayerDef) -> Result<TileLayerNode, String> {
    let tiles = &mut map.map;
    check_tilemap_size(tiles.columns(), tiles.rows(), tiles.layer_count() + 1)
        .map_err(|err| err.to_string())?;
    let mut ids = HashMap::new();
    for (symbol, name) in &layer.legend {
        let id = map
            .kinds
            .ids
            .get(name)
            .ok_or_else(|| format!("there's no tile `{name}` in the tilemap's tileset"))?;
        ids.insert(*symbol, *id);
    }
    let index = tiles.add_layer();
    for (row, line) in layer.tiles.iter().enumerate() {
        for (column, symbol) in line.chars().enumerate() {
            if let Some(&id) = ids.get(&symbol) {
                let cell = [column as i32, row as i32];
                if tiles.contains(cell) {
                    tiles.set(index, cell, Some(id));
                }
            }
        }
    }
    let bounds = tiles.bounds();
    Ok(TileLayerNode {
        layer: index,
        size: [bounds.w, bounds.h],
    })
}

/// Instances, expanded: the nodes of the scene files they place in their stead, with their
/// overrides made. Sprites then name their sheets by `file#sheet`, since nodes from different
/// files are together.
struct Expand<'a> {
    assets: &'a mut dyn Assets,
    /// The scene files being expanded, innermost last, to catch one that places itself.
    stack: Vec<String>,
    /// Every sheet of every file, by `file#sheet`.
    sheets: BTreeMap<String, SheetDef>,
}

/// Work left to expand a file. Children finish before their node, and an instance's placed
/// file finishes before its overrides and its own children are joined to it. Keeping this on
/// the heap leaves room on the call stack to parse files even at the deepest instance.
enum ExpandStep {
    Node {
        node: NodeDef,
        file: String,
        path: String,
        depth: usize,
        children: usize,
    },
    Instance {
        node: NodeDef,
        file: String,
        path: String,
        children: usize,
        props: Props,
    },
    Error(Error),
}

fn sheet_key(file: &str, sheet: &str) -> String {
    format!("{file}#{sheet}")
}

impl Expand<'_> {
    /// The nodes of `file`, at `path`, expanded without recursing through nodes or instances.
    fn file(&mut self, file: &SceneFile, path: &str) -> Result<Vec<NodeDef>> {
        let mut steps = Vec::new();
        self.queue_file(file, path, 0, &mut steps);
        let mut nodes = Vec::new();
        while let Some(step) = steps.pop() {
            match step {
                ExpandStep::Error(error) => return Err(error),
                ExpandStep::Node {
                    mut node,
                    file,
                    path,
                    depth,
                    children,
                } => {
                    node.children = nodes.split_off(nodes.len() - children);
                    if let KindDef::Instance(instance) = &node.kind {
                        let scene = instance.scene.clone();
                        if self.stack.contains(&scene) {
                            return Err(Error::Scene {
                                path: file.into(),
                                message: format!(
                                    "node `{path}`: `{scene}` places itself, through its instances"
                                ),
                            });
                        }
                        let placed = self.assets.scene(&scene)?;
                        self.stack.push(scene.clone());
                        steps.push(ExpandStep::Instance {
                            node,
                            file,
                            path,
                            children: placed.children.len(),
                            props: placed.props.clone(),
                        });
                        self.queue_file(&placed, &scene, depth + 1, &mut steps);
                    } else {
                        nodes.push(node);
                    }
                }
                ExpandStep::Instance {
                    mut node,
                    file,
                    path,
                    children,
                    mut props,
                } => {
                    self.stack.pop();
                    let mut placed = nodes.split_off(nodes.len() - children);
                    let KindDef::Instance(instance) = &node.kind else {
                        unreachable!("only instances place files");
                    };
                    let error = |message: String| Error::Scene {
                        path: file.clone().into(),
                        message: format!("node `{path}`: {message}"),
                    };
                    for (target, change) in &instance.overrides {
                        self.change(&mut placed, target, change, &instance.scene)
                            .map_err(|err| error(format!("overriding `{target}`: {err}")))?;
                    }
                    for child in node.children {
                        if placed.iter().any(|node| node.name == child.name) {
                            return Err(error(format!(
                                "`{}` has a node named `{}` already, but siblings' names are unique",
                                instance.scene, child.name
                            )));
                        }
                        placed.push(child);
                    }
                    node.children = placed;
                    node.kind = KindDef::Node;
                    props.extend(node.props);
                    node.props = props;
                    nodes.push(node);
                }
            }
        }
        Ok(nodes)
    }

    /// Queues a file's nodes in reverse drawing order, parents before children, so popping
    /// steps finishes children before their parents. Only each node's own fields are cloned;
    /// walking or cloning a whole subtree recursively here would use the stack we're saving.
    fn queue_file(
        &mut self,
        file: &SceneFile,
        path: &str,
        depth: usize,
        steps: &mut Vec<ExpandStep>,
    ) {
        for (name, sheet) in &file.sheets {
            self.sheets.insert(sheet_key(path, name), sheet.clone());
        }
        let mut pending = file
            .children
            .iter()
            .map(|node| (node, String::new(), depth))
            .collect::<Vec<_>>();
        while let Some((def, parent, depth)) = pending.pop() {
            let at = if parent.is_empty() {
                def.name.clone()
            } else {
                format!("{parent}/{}", def.name)
            };
            if depth >= MAX_NODE_DEPTH {
                steps.push(ExpandStep::Error(Error::Scene {
                    path: path.into(),
                    message: format!(
                        "node `{at}`: nodes nest more than {MAX_NODE_DEPTH} deep, too deep to place"
                    ),
                }));
                continue;
            }
            let mut node = NodeDef {
                name: def.name.clone(),
                kind: def.kind.clone(),
                tags: def.tags.clone(),
                props: def.props.clone(),
                children: Vec::new(),
                ..*def
            };
            match &mut node.kind {
                KindDef::Sprite(SpriteDef { sheet, .. })
                | KindDef::Billboard(BillboardDef { sheet, .. })
                | KindDef::Mesh(MeshDef {
                    sprite: Some(CellDef { sheet, .. }),
                    ..
                }) => *sheet = sheet_key(path, sheet),
                _ => {}
            }
            steps.push(ExpandStep::Node {
                node,
                file: path.to_owned(),
                path: at.clone(),
                depth,
                children: def.children.len(),
            });
            for child in &def.children {
                pending.push((child, at.clone(), depth + 1));
            }
        }
    }

    /// Changes the node at `target` among `nodes`, the nodes of the scene file `file` with any
    /// of its instances expanded, by the fields `change` sets.
    fn change(
        &self,
        nodes: &mut [NodeDef],
        target: &str,
        change: &BTreeMap<String, Value>,
        file: &str,
    ) -> Result<(), String> {
        let missing = || format!("there's no node `{target}` to override");
        let mut names = target.split('/').peekable();
        let mut list = nodes;
        let mut tilemap = None;
        let mut in_3d = false;
        let mut parent = String::new();
        let node = loop {
            let name = names.next().ok_or_else(missing)?;
            let node = list
                .iter_mut()
                .find(|node| node.name == name)
                .ok_or_else(missing)?;
            if names.peek().is_none() {
                break node;
            }
            tilemap = match &node.kind {
                KindDef::Tilemap(map) => Some([map.columns, map.rows]),
                _ => None,
            };
            in_3d = matches!(node.kind, KindDef::View3d(_)) || node.placement3.is_some();
            parent = if parent.is_empty() {
                node.name.clone()
            } else {
                format!("{parent}/{}", node.name)
            };
            list = &mut node.children;
        };
        // Sprites name their sheets with their files' paths here: the file of the sheet the
        // node shows, or a mesh showing none, the file the instance places.
        let mut change = change.clone();
        if let Some(Value::String(sheet)) = change.get_mut("sheet") {
            let shown = match &node.kind {
                KindDef::Sprite(sprite) => Some(&sprite.sheet),
                KindDef::Billboard(billboard) => Some(&billboard.sheet),
                KindDef::Mesh(mesh) => mesh.sprite.as_ref().map(|sprite| &sprite.sheet),
                _ => None,
            };
            let from = shown
                .and_then(|key| key.rsplit_once('#'))
                .map_or(file, |(file, _)| file);
            *sheet = sheet_key(from, sheet);
        }
        let inside = file::Parent {
            path: &parent,
            tilemap,
            in_3d,
        };
        let mut changed = file::override_node(node, &change, &self.sheets, &inside)?;
        changed.children = std::mem::take(&mut node.children);
        *node = changed;
        Ok(())
    }
}

/// What building a scene's nodes needs: the scene file's path, for errors, the font for its text,
/// where its tilesets and images come from, and the sheets and tilesets loaded so far.
struct Build<'a> {
    path: &'a str,
    font: &'a Font,
    assets: &'a mut dyn Assets,
    /// Every sheet of every file, by `file#sheet`, from [`Expand`].
    sheet_defs: BTreeMap<String, SheetDef>,
    sheets: HashMap<String, SpriteSheet>,
    tilesets: HashMap<String, (Tileset, Rc<TileKinds>)>,
    /// The meshes of the shapes made so far, so mesh nodes of the same shape share one.
    meshes: Vec<(Shape, Mesh)>,
    models: HashMap<String, Model>,
}

impl Build<'_> {
    fn error(&self, node: &str, message: &str) -> Error {
        Error::Scene {
            path: self.path.into(),
            message: format!("node `{node}`: {message}"),
        }
    }

    /// The sprite sheet `key`, `file#sheet`, loading it if it hasn't been.
    fn sheet(&mut self, key: &str, node: &str) -> Result<&SpriteSheet> {
        if !self.sheets.contains_key(key) {
            let Some(def) = self.sheet_defs.get(key) else {
                let (_, name) = key.rsplit_once('#').unwrap_or(("", key));
                return Err(self.error(node, &format!("there's no sheet `{name}` in the scene")));
            };
            let texture = self.assets.texture(&def.path)?;
            let sheet = SpriteSheet::try_new(texture, def.tile_width, def.tile_height)
                .map_err(|err| self.error(node, &err))?;
            self.sheets.insert(key.to_owned(), sheet);
        }
        Ok(&self.sheets[key])
    }

    /// The tileset file at `path`, built, loading it if it hasn't been.
    fn tileset(&mut self, path: &str) -> Result<(Tileset, Rc<TileKinds>)> {
        if let Some((tileset, kinds)) = self.tilesets.get(path) {
            return Ok((tileset.clone(), Rc::clone(kinds)));
        }
        let file = self.assets.tileset(path)?;
        let error = |message: String| Error::Tileset {
            path: path.into(),
            message,
        };
        let mut tileset = Tileset::try_new(file.tile_width, file.tile_height).map_err(error)?;
        let mut kinds = TileKinds {
            names: Vec::new(),
            ids: HashMap::new(),
            props: Vec::new(),
        };
        let mut sheets = HashMap::new();
        for (name, tile) in &file.tiles {
            if !sheets.contains_key(&tile.sheet) {
                let def = file.sheets.get(&tile.sheet).ok_or_else(|| {
                    error(format!("tile `{name}`: there's no sheet `{}`", tile.sheet))
                })?;
                let texture = self.assets.texture(&def.path)?;
                let sheet = SpriteSheet::try_new(texture, def.tile_width, def.tile_height)
                    .map_err(|err| error(format!("sheet `{}`: {err}", tile.sheet)))?;
                sheets.insert(tile.sheet.clone(), sheet);
            }
            let [column, row] = tile.cell;
            let sprite = sheets[&tile.sheet]
                .try_sprite(column, row)
                .map_err(|err| error(format!("tile `{name}`: {err}")))?;
            let id = tileset.add(Tile {
                sprite,
                tint: tile.tint,
                solid: tile.solid,
            });
            kinds.names.push(name.clone());
            kinds.ids.insert(name.clone(), id);
            kinds.props.push(tile.props.clone());
        }
        let kinds = Rc::new(kinds);
        self.tilesets
            .insert(path.to_owned(), (tileset.clone(), Rc::clone(&kinds)));
        Ok((tileset, kinds))
    }

    /// What the node at `node` shows, as `def` describes it.
    fn kind(&mut self, def: &KindDef, node: &str) -> Result<Kind> {
        let kind = match def {
            KindDef::Node | KindDef::Instance(_) => Kind::Node,
            KindDef::Sprite(sprite) => {
                let [column, row] = sprite.cell;
                let sheet = self.sheet(&sprite.sheet, node)?.clone();
                let image = sheet.try_sprite(column, row);
                let image = image.map_err(|err| self.error(node, &err))?;
                Kind::Sprite(SpriteNode {
                    size: sprite.size.unwrap_or([image.width(), image.height()]),
                    sprite: image,
                    sheet: Some(sheet),
                    stretch: sprite.stretch,
                    pivot: sprite.pivot,
                    tint: sprite.tint,
                })
            }
            KindDef::Rect(rect) => Kind::Rect(RectNode {
                size: rect.size,
                stretch: rect.stretch,
                pivot: rect.pivot,
                color: rect.color,
            }),
            KindDef::Text(text) => {
                let font = self
                    .font
                    .try_scaled(text.font_scale)
                    .map_err(|err| self.error(node, &err))?;
                Kind::Text(TextNode {
                    pivot: text.pivot,
                    color: text.color,
                    ..TextNode::new(&text.text, &font, text.wrap, text.align)
                })
            }
            KindDef::Area(area) => Kind::Area(AreaNode {
                size: area.size,
                stretch: area.stretch,
                pivot: area.pivot,
            }),
            KindDef::Tilemap(map) => {
                let (tileset, kinds) = self.tileset(&map.tileset)?;
                Kind::Tilemap(TilemapNode {
                    map: Tilemap::new(tileset, map.columns, map.rows),
                    kinds,
                })
            }
            KindDef::TileLayer(_) => {
                return Err(self.error(node, "a tile layer must be the child of a tilemap"));
            }
            KindDef::View3d(view) => Kind::View3d(View3dNode {
                size: view.size,
                stretch: view.stretch,
                pivot: view.pivot,
                camera: view.camera,
                lighting: view.lighting,
            }),
            KindDef::Mesh(mesh) => {
                let (sprite, sheet) = match &mesh.sprite {
                    Some(sprite) => {
                        let (image, sheet) = self.sprite(&sprite.sheet, sprite.cell, node)?;
                        (Some(image), Some(sheet))
                    }
                    None => (None, None),
                };
                Kind::Mesh(MeshNode {
                    mesh: self.mesh(&mesh.shape)?,
                    sprite,
                    sheet,
                    tint: mesh.tint,
                })
            }
            KindDef::Model(model) => Kind::Model(ModelNode {
                model: self.model(&model.model)?,
                tint: model.tint,
            }),
            KindDef::Billboard(billboard) => {
                let (sprite, sheet) = self.sprite(&billboard.sheet, billboard.cell, node)?;
                Kind::Billboard(BillboardNode {
                    sprite,
                    sheet: Some(sheet),
                    size: billboard.size,
                    tint: billboard.tint,
                })
            }
        };
        Ok(kind)
    }

    /// The sprite in `cell` of the sheet `key`, `file#sheet`, and the sheet.
    fn sprite(&mut self, key: &str, cell: [u32; 2], node: &str) -> Result<(Sprite, SpriteSheet)> {
        let [column, row] = cell;
        let sheet = self.sheet(key, node)?.clone();
        let sprite = sheet
            .try_sprite(column, row)
            .map_err(|err| self.error(node, &err))?;
        Ok((sprite, sheet))
    }

    /// The mesh of `shape`, making it if it hasn't been.
    fn mesh(&mut self, shape: &Shape) -> Result<Mesh> {
        if let Some((_, mesh)) = self.meshes.iter().find(|(made, _)| made == shape) {
            return Ok(mesh.clone());
        }
        let mesh = self.assets.mesh(&shape.mesh_data())?;
        self.meshes.push((*shape, mesh.clone()));
        Ok(mesh)
    }

    /// The model file at `path`, loading it if it hasn't been.
    fn model(&mut self, path: &str) -> Result<Model> {
        if let Some(model) = self.models.get(path) {
            return Ok(model.clone());
        }
        let model = self.assets.model(path)?;
        self.models.insert(path.to_owned(), model.clone());
        Ok(model)
    }
}
