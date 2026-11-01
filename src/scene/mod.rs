//! Scenes: trees of sprites, rectangles, text, areas and tilemaps, laid out in files that people
//! and tools can both write, for levels, screens and characters.
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
//!     format = 1,
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
//! - `format`, required, is the version of the format, 1 ([`FORMAT`]). A newer engine reads older
//!   formats; this one refuses newer ones.
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
//! Tracks change `x`, `y`, `rotation`, `scale_x` and `scale_y`, which move from key to key in a
//! straight line, or with `interpolation = "step"` jump at each key; and `visible` and a sprite's
//! `cell`, which always step. `keys` are a time in seconds, from 0 to the duration, and the value
//! then, in order of time; before the first key a property has the first key's value, and after
//! the last, the last key's. A loop that should come back to where it started has its first
//! key's value again at its end.
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
//!     format = 1,
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
//! the mouse is against its nodes.

mod clip;
mod file;
mod writer;

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
    rc::Rc,
};

pub use clip::{Clip, Interpolation, KeyValue, Keys, Property, Track};
pub use file::{
    AreaDef, FORMAT, InstanceDef, KindDef, NodeDef, Props, RectDef, SceneFile, SheetDef, SpriteDef,
    TextDef, TileDef, TileLayerDef, TilemapDef, TilesetFile, Value,
};

use crate::{
    Context, Error, Result,
    graphics::{Align, Canvas, Color, Font, Sprite, SpriteSheet, TextLayout, Texture},
    lua::check_tilemap_size,
    math::{Affine, Rect},
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

    /// Writes the scene to the file at `path`, as [`SceneFile::to_luau`] gives it.
    pub fn write(&self, path: impl AsRef<Path>) -> Result<()> {
        write_file(path.as_ref(), &self.to_luau()?)
    }
}

impl TilesetFile {
    /// The tileset as the text of a tileset file, in the canonical layout, as
    /// [`SceneFile::to_luau`] writes a scene.
    pub fn to_luau(&self) -> Result<String> {
        writer::write_tileset(self).map_err(Error::WriteScene)
    }

    /// Writes the tileset to the file at `path`, as [`TilesetFile::to_luau`] gives it.
    pub fn write(&self, path: impl AsRef<Path>) -> Result<()> {
        write_file(path.as_ref(), &self.to_luau()?)
    }
}

fn write_file(path: &Path, text: &str) -> Result<()> {
    fs::write(path, text).map_err(|source| Error::WriteFile {
        path: path.to_owned(),
        source,
    })
}

/// Whether `key` can be written bare, as `key = value`, rather than as `["key"] = value`.
fn is_identifier(key: &str) -> bool {
    const KEYWORDS: [&str; 22] = [
        "and", "break", "continue", "do", "else", "elseif", "end", "false", "for", "function",
        "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until",
        "while",
    ];
    let mut chars = key.chars();
    let starts = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    starts && chars.all(|c| c.is_ascii_alphanumeric() || c == '_') && !KEYWORDS.contains(&key)
}

/// Where the files and images a scene uses come from as it's built: the scene files its
/// instances place, the tileset files its tilemaps use, and the images of its sheets, each by
/// its path as the file names it. [`FolderAssets`] reads them from a game's folder; a game can
/// keep them loaded instead.
pub trait Assets {
    fn texture(&mut self, path: &str) -> Result<Texture>;
    fn scene(&mut self, path: &str) -> Result<Rc<SceneFile>>;
    fn tileset(&mut self, path: &str) -> Result<Rc<TilesetFile>>;
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
    visible: bool,
    tags: Vec<String>,
    props: Props,
    kind: Kind,
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
        }
    }

    /// Whether it only moves with its node, rather than turning and stretching too.
    fn only_moves(&self) -> bool {
        matches!(self, Self::Text(_) | Self::Tilemap(_) | Self::TileLayer(_))
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
    pub fn set_cell(&mut self, [column, row]: [u32; 2]) -> Result<(), String> {
        let sheet = self.sheet.as_ref().ok_or("the sprite isn't from a sheet")?;
        self.sprite = sheet.try_sprite(column, row)?;
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
    }

    /// One of its numbers, as a clip's track changes it.
    fn number(&self, property: Property) -> f32 {
        match property {
            Property::X => self.position[0],
            Property::Y => self.position[1],
            Property::Rotation => self.rotation,
            Property::ScaleX => self.scale[0],
            Property::ScaleY => self.scale[1],
        }
    }

    fn set_number(&mut self, property: Property, value: f32) {
        match property {
            Property::X => self.position[0] = value,
            Property::Y => self.position[1] = value,
            Property::Rotation => self.rotation = value,
            Property::ScaleX => self.scale[0] = value,
            Property::ScaleY => self.scale[1] = value,
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

    /// The width and height of a sprite, rect, text, area, tilemap or tile layer, before
    /// stretching with its parent. A plain node has none.
    pub fn size(&self) -> Option<[f32; 2]> {
        match &self.kind {
            Kind::Node => None,
            Kind::Sprite(sprite) => Some(sprite.size),
            Kind::Rect(rect) => Some(rect.size),
            Kind::Text(text) => Some(text.layout.size()),
            Kind::Area(area) => Some(area.size),
            Kind::Tilemap(map) => {
                let bounds = map.map.bounds();
                Some([bounds.w, bounds.h])
            }
            Kind::TileLayer(layer) => Some(layer.size),
        }
    }

    /// The rectangle the node covers in its own coordinates, its pivot at the origin, inside a
    /// parent whose rectangle is `outer`, which it may stretch with. A plain node covers nothing.
    fn rect_in(&self, outer: Rect) -> Option<Rect> {
        let size = self.size()?;
        let (stretch, [x, y]) = match &self.kind {
            Kind::Sprite(sprite) => (sprite.stretch, sprite.pivot),
            Kind::Rect(rect) => (rect.stretch, rect.pivot),
            Kind::Area(area) => (area.stretch, area.pivot),
            Kind::Text(text) => {
                let [x, y] = text.pivot;
                let x = x + size[0] * text.align().fraction();
                ([0.0, 0.0], [x, y])
            }
            Kind::Node | Kind::Tilemap(_) | Kind::TileLayer(_) => ([0.0, 0.0], [0.0, 0.0]),
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
        };
        let root = scene.root();
        for node in &nodes {
            scene.build(&mut build, node, root, "")?;
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

    /// Adds the node `def` describes, and its children, at the end of `parent`'s children.
    fn build(
        &mut self,
        build: &mut Build,
        def: &NodeDef,
        parent: NodeId,
        path: &str,
    ) -> Result<()> {
        let path = if path.is_empty() {
            def.name.clone()
        } else {
            format!("{path}/{}", def.name)
        };
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
        node.update_local();
        node.visible = def.visible;
        node.tags = def.tags.clone();
        node.props = def.props.clone();
        let id = self.attach(node, parent);
        for child in &def.children {
            self.build(build, child, id, &path)?;
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
            if let Keys::Cell(keys) = &track.keys {
                self.check_cells(target, keys)?;
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
                    if steps
                        && let Some(cell) = clip::step(keys, time)
                        && let Kind::Sprite(sprite) = &mut node.kind
                    {
                        sprite.set_cell(cell)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Fails unless `id` is a sprite node from a sheet with every cell of `keys`.
    fn check_cells(&self, id: NodeId, keys: &[(f32, [u32; 2])]) -> Result<(), String> {
        let sheet = match &self.node(id).kind {
            Kind::Sprite(SpriteNode {
                sheet: Some(sheet), ..
            }) => sheet,
            Kind::Sprite(_) => {
                return Err(format!(
                    "{}'s sprite isn't from a sheet, so it has no cells",
                    self.describe(id)
                ));
            }
            kind => {
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
    /// or if `from` is a tile layer, which only its tilemap can hold.
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
        let id = self.copy(parent, source, from);
        self.node_mut(id).name = name.to_owned();
        Ok(id)
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
                Kind::Node | Kind::Area(_) | Kind::Tilemap(_) => {}
            }
        }
        for &child in &node.children {
            let child_placed = placed.child(self.node(child));
            self.draw_tree(canvas, child, &child_placed, visible);
        }
    }
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

fn sheet_key(file: &str, sheet: &str) -> String {
    format!("{file}#{sheet}")
}

impl Expand<'_> {
    /// The nodes of `file`, at `path`, expanded.
    fn file(&mut self, file: &SceneFile, path: &str) -> Result<Vec<NodeDef>> {
        for (name, sheet) in &file.sheets {
            self.sheets.insert(sheet_key(path, name), sheet.clone());
        }
        file.children
            .iter()
            .map(|node| self.node(node, path, ""))
            .collect()
    }

    /// `def`, a node of the file `file` inside the node at `parent`, expanded.
    fn node(&mut self, def: &NodeDef, file: &str, parent: &str) -> Result<NodeDef> {
        let path = if parent.is_empty() {
            def.name.clone()
        } else {
            format!("{parent}/{}", def.name)
        };
        let error = |message: String| Error::Scene {
            path: file.into(),
            message: format!("node `{path}`: {message}"),
        };
        let mut node = NodeDef {
            name: def.name.clone(),
            kind: def.kind.clone(),
            tags: def.tags.clone(),
            props: def.props.clone(),
            children: Vec::new(),
            ..*def
        };
        if let KindDef::Sprite(sprite) = &mut node.kind {
            sprite.sheet = sheet_key(file, &sprite.sheet);
        }
        let mut children = def
            .children
            .iter()
            .map(|child| self.node(child, file, &path))
            .collect::<Result<Vec<_>>>()?;
        if let KindDef::Instance(instance) = &def.kind {
            let scene = &instance.scene;
            if self.stack.contains(scene) {
                return Err(error(format!(
                    "`{scene}` places itself, through its instances"
                )));
            }
            let placed = self.assets.scene(scene)?;
            self.stack.push(scene.clone());
            let mut nodes = self.file(&placed, scene)?;
            self.stack.pop();
            for (target, change) in &instance.overrides {
                self.change(&mut nodes, target, change)
                    .map_err(|err| error(format!("overriding `{target}`: {err}")))?;
            }
            for child in children {
                if nodes.iter().any(|node| node.name == child.name) {
                    return Err(error(format!(
                        "`{scene}` has a node named `{}` already, but siblings' names are unique",
                        child.name
                    )));
                }
                nodes.push(child);
            }
            children = nodes;
            node.kind = KindDef::Node;
            let mut props = placed.props.clone();
            props.extend(def.props.clone());
            node.props = props;
        }
        node.children = children;
        Ok(node)
    }

    /// Changes the node at `target` among `nodes` by the fields `change` sets.
    fn change(
        &self,
        nodes: &mut [NodeDef],
        target: &str,
        change: &BTreeMap<String, Value>,
    ) -> Result<(), String> {
        let missing = || format!("there's no node `{target}` to override");
        let mut names = target.split('/').peekable();
        let mut list = nodes;
        let mut tilemap = None;
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
            parent = if parent.is_empty() {
                node.name.clone()
            } else {
                format!("{parent}/{}", node.name)
            };
            list = &mut node.children;
        };
        // Sprites name their sheets with their files' paths here.
        let mut change = change.clone();
        if let (Some(Value::String(sheet)), KindDef::Sprite(sprite)) =
            (change.get_mut("sheet"), &node.kind)
        {
            let (file, _) = sprite.sheet.rsplit_once('#').unwrap_or_default();
            *sheet = sheet_key(file, sheet);
        }
        let inside = file::Parent {
            path: &parent,
            tilemap,
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
        };
        Ok(kind)
    }
}
