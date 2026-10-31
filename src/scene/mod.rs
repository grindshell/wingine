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
//!
//! Every node has a `name`, unique among its siblings and without `/`, so a path such as
//! `"mill/sails"` finds a node from the one above it, and a `kind`. Each of these is optional:
//!
//! - `x` and `y`, where its origin is within its parent, 0 by default. Scene coordinates are
//!   the coordinates the scene is drawn in: canvas pixels, or the world's with a camera.
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
//!   `height` to stretch it to (its own size by default), and `tint`, a color its colors are
//!   multiplied by.
//! - `rect`: a filled rectangle, `width` by `height`, of `color` (white by default).
//! - `text`: `text` in the built-in font, `font_scale` times its size (1 by default), wrapped
//!   between words to `wrap` if given, in `color` (white by default). Text moves with its node,
//!   but doesn't turn or stretch.
//! - `area`: a rectangle `width` by `height` that doesn't draw, for marking out part of a scene,
//!   such as somewhere the player can't walk or where something happens.
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
//! at `x`, `y` has a pivot of `{ 8, 16 }`, the middle of its feet. Colors are hex strings, such as
//! `"#f2c14e"`, or `"#00000080"` with alpha. Any field a node doesn't have is an error naming the
//! node, as is a missing one it must have.
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
//! adds and removes, drawn with [`Scene::draw`].

mod file;
mod writer;

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
    rc::Rc,
};

pub use file::{
    AreaDef, FORMAT, InstanceDef, KindDef, NodeDef, Props, RectDef, SceneFile, SheetDef, SpriteDef,
    TextDef, TileDef, TileLayerDef, TilemapDef, TilesetFile, Value,
};

use crate::{
    Context, Error, Result,
    graphics::{Canvas, Color, Font, Sprite, SpriteSheet, TextLayout, Texture},
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
/// Cloning a scene copies its nodes, sharing the images they show.
#[derive(Clone, Debug)]
pub struct Scene {
    slots: Vec<Slot>,
    /// Slots whose nodes were removed, for new nodes.
    free: Vec<u32>,
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
    /// From the node's coordinates to its parent's, kept up to date with the three above.
    local: Affine,
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
    /// The width and height it's stretched to.
    pub size: [f32; 2],
    /// The point of the sprite, from its top-left corner, at the node's origin.
    pub pivot: [f32; 2],
    pub tint: Color,
}

/// What a rect node fills.
#[derive(Clone, Debug)]
pub struct RectNode {
    pub size: [f32; 2],
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
    /// The point of the text, from its top-left corner, at the node's origin.
    pub pivot: [f32; 2],
    pub color: Color,
}

/// The rectangle an area node marks out.
#[derive(Clone, Debug)]
pub struct AreaNode {
    pub size: [f32; 2],
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

impl TextNode {
    /// `text` in `font`, wrapped between words to `wrap` if given.
    pub fn new(text: impl Into<String>, font: &Font, wrap: Option<f32>) -> Self {
        let text = text.into();
        Self {
            layout: font.layout(&text, wrap),
            text,
            wrap,
            pivot: [0.0, 0.0],
            color: Color::WHITE,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Changes the text, laying it out again.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.layout = self.layout.font().layout(&self.text, self.wrap);
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

    /// Where its origin is in its parent's coordinates.
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

    /// From its coordinates to its parent's: its scale, then its rotation, then its position.
    pub fn local_transform(&self) -> Affine {
        self.local
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

    /// The rectangle a sprite, rect, text, area, tilemap or tile layer covers, in the node's own
    /// coordinates, its pivot at the origin. A plain node covers nothing.
    pub fn rect(&self) -> Option<Rect> {
        let (size, [x, y]) = match &self.kind {
            Kind::Node => return None,
            Kind::Sprite(sprite) => (sprite.size, sprite.pivot),
            Kind::Rect(rect) => (rect.size, rect.pivot),
            Kind::Text(text) => (text.layout.size(), text.pivot),
            Kind::Area(area) => (area.size, area.pivot),
            Kind::Tilemap(map) => {
                let bounds = map.map.bounds();
                ([bounds.w, bounds.h], [0.0, 0.0])
            }
            Kind::TileLayer(layer) => (layer.size, [0.0, 0.0]),
        };
        Some(Rect::new(-x, -y, size[0], size[1]))
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
    /// `folder`.
    pub fn load(
        ctx: &mut Context,
        folder: impl AsRef<Path>,
        path: impl AsRef<Path>,
    ) -> Result<Self> {
        let folder = folder.as_ref();
        let path = path.as_ref();
        let file = SceneFile::read(folder.join(path))?;
        let font = ctx.default_font().clone();
        let mut assets = FolderAssets { ctx, folder };
        Self::new(&file, &path.to_string_lossy(), &font, &mut assets)
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

    /// From the node's coordinates to the scene's: its own transform, then its parent's, and so
    /// on up to the root.
    pub fn transform(&self, id: NodeId) -> Affine {
        let mut transform = self.node(id).local;
        let mut at = id;
        while let Some(parent) = self.node(at).parent {
            transform = self.node(parent).local * transform;
            at = parent;
        }
        transform
    }

    /// Where what the node shows is placed in the scene: by [`Scene::transform`], or only moved
    /// by it, for text and tilemaps, which don't turn or stretch.
    fn placement(&self, id: NodeId) -> Affine {
        let transform = self.transform(id);
        if self.node(id).kind.only_moves() {
            Affine::from_translation(transform.translation)
        } else {
            transform
        }
    }

    /// The smallest axis-aligned rectangle in the scene's coordinates around what the node
    /// covers, if it covers anything: see [`Node::rect`].
    pub fn bounds(&self, id: NodeId) -> Option<Rect> {
        let rect = self.node(id).rect()?;
        Some(self.placement(id).bounds(rect))
    }

    /// Whether `point`, in the scene's coordinates, is inside what the node covers, turned and
    /// stretched as it's drawn. Nothing is inside a plain node, or one squashed flat.
    pub fn contains(&self, id: NodeId, point: [f32; 2]) -> bool {
        let Some(rect) = self.node(id).rect() else {
            return false;
        };
        let Some(inverse) = self.placement(id).inverse() else {
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

    /// Draws the whole scene.
    pub fn draw(&self, canvas: &mut Canvas) {
        self.draw_node(canvas, self.root());
    }

    /// Draws `id` and the nodes inside it where they are in the scene, each before its children.
    /// Nothing is drawn where it's off the canvas, or where a node, or a node it's inside, isn't
    /// visible.
    pub fn draw_node(&self, canvas: &mut Canvas, id: NodeId) {
        let mut at = id;
        while let Some(above) = self.node(at).parent {
            if !self.node(above).visible {
                return;
            }
            at = above;
        }
        let parent = match self.node(id).parent {
            Some(above) => self.transform(above),
            None => Affine::IDENTITY,
        };
        let visible = canvas.visible_area();
        self.draw_tree(canvas, id, parent, visible);
    }

    fn draw_tree(&self, canvas: &mut Canvas, id: NodeId, parent: Affine, visible: Rect) {
        let node = self.node(id);
        if !node.visible {
            return;
        }
        let transform = parent * node.local;
        if let Some(rect) = node.rect() {
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
                    // Where its tilemap is, which is its parent, and only culls by cell.
                    if let Some(Kind::Tilemap(map)) = node
                        .parent
                        .and_then(|parent| self.get(parent))
                        .map(|p| &p.kind)
                    {
                        map.map
                            .draw_layer_at(canvas, layer.layer, parent.translation);
                    }
                }
                Kind::Node | Kind::Area(_) | Kind::Tilemap(_) => {}
            }
        }
        for &child in &node.children {
            self.draw_tree(canvas, child, transform, visible);
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
        let mut fields = file::node_fields(node);
        for (field, value) in change {
            match (field.as_str(), value, &node.kind) {
                ("props", Value::Map(props), _) => {
                    if let Some(Value::Map(old)) = fields.get_mut("props") {
                        old.extend(props.clone());
                    }
                }
                ("sheet", Value::String(sheet), KindDef::Sprite(sprite)) => {
                    let (file, _) = sprite.sheet.rsplit_once('#').unwrap_or_default();
                    fields.insert(field.clone(), Value::String(sheet_key(file, sheet)));
                }
                _ => {
                    fields.insert(field.clone(), value.clone());
                }
            }
        }
        let inside = file::Parent {
            path: &parent,
            tilemap,
        };
        let mut changed = file::read_node(Value::Map(fields), &inside, 1, &self.sheets, 0)?;
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
                let image = self.sheet(&sprite.sheet, node)?.try_sprite(column, row);
                let image = image.map_err(|err| self.error(node, &err))?;
                Kind::Sprite(SpriteNode {
                    size: sprite.size.unwrap_or([image.width(), image.height()]),
                    sprite: image,
                    pivot: sprite.pivot,
                    tint: sprite.tint,
                })
            }
            KindDef::Rect(rect) => Kind::Rect(RectNode {
                size: rect.size,
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
                    ..TextNode::new(&text.text, &font, text.wrap)
                })
            }
            KindDef::Area(area) => Kind::Area(AreaNode {
                size: area.size,
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
