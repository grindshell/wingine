//! Scenes: trees of sprites, rectangles, text, areas, particles, tilemaps and 3D views, laid out in
//! files that people and tools can both write, for levels, screens and characters.
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
//!     format = 4,
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
//! - `format`, required, is the version of the format, 4 ([`FORMAT`]), which added particles and
//!   blend modes, as 3 added stacks and panels and 2 3D views. A newer engine reads older formats,
//!   as this one reads formats 1 to 3; this one refuses newer ones.
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
//! - `text`: `text` in `font`, the font file at that path from the game's folder (the built-in
//!   font without one; see below), `font_scale` times its size (1 by default), wrapped
//!   between words to `wrap` if given, in `color` (white by default). `align`, `"left"` by
//!   default, `"center"` or `"right"`, lines its lines up along their left edges, about their
//!   middles or along their right edges, and puts that edge or middle at the node's origin, so
//!   `"right"` ends the text there. Text moves with its node, but doesn't turn or stretch.
//! - `area`: a rectangle `width` by `height` that doesn't draw, for marking out part of a scene,
//!   such as somewhere the player can't walk or where something happens, or for laying out the
//!   nodes inside it. It has a `stretch`, as a rect does.
//! - `stack`: a rectangle that doesn't draw, laying out the nodes inside it one after another,
//!   down or across, such as a menu's items or a panel's lines. See below.
//! - `panel`: a stack that draws behind what it holds: filled with `color` (white by default),
//!   inside a `border` that many units wide (0 by default) of `border_color` (white by default);
//!   or, with a `sheet` and a `cell`, that sprite sliced as a frame is (see
//!   [`Canvas::draw_sprite_sliced`]), `slice` texels along its edges, a number for them all or
//!   `{ left, top, right, bottom }`, keeping their size while the rest stretches, times `tint`.
//! - `particles`: an emitter of particles ([`Particles`]), with the fields of
//!   [`ParticleSettings`] (`rate`, `lifetime`, `colors` and the rest), each optional, which
//!   [`Scene::advance_particles`] sends out from random points of its rectangle, laid out as an
//!   area's, `width` by `height` (0 by 0, a point, by default) with a `stretch`. With a `sheet` and
//!   a `cell`, each particle shows that sprite; without, a white square. Its particles draw where
//!   it is among the scene's nodes. Its randomness starts from its path, unless it has a `seed`.
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
//! `"#00000080"` with alpha. A sprite, rect, text, tile layer or particles node may have a
//! `blend`, how what it draws mixes with what's under it: `"normal"`, the default, `"add"`,
//! `"multiply"` or `"screen"` ([`Blend`]). Any field a node doesn't have is an error naming the node, as is a
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
//! Lists are laid out by stacks and panels, which place the nodes inside them one after another,
//! so a game changes what's in a list, and how much of it shows, without placing anything:
//!
//! - `direction` is `"column"` (the default), down from the top, or `"row"`, across from the
//!   left. Each node inside takes the space it covers that way, turned and stretched as it's
//!   drawn, after the one before and the `gap` (0 by default). A hidden node takes no space, and
//!   nor does its gap.
//! - Across, each is lined up by its `anchor`, the same fraction of it at that fraction of the
//!   stack (by whole units, as text lines up), at the start without one; one that stretches
//!   across stretches with the stack. Its pivot doesn't move it, though it still turns about it,
//!   and its `x` and `y` move it from where the stack puts it without moving the rest.
//! - A plain node, as an instance becomes, has its origin at the start of its space, and takes
//!   as much as the nodes inside it reach past its origin. They anchor and stretch across the
//!   stack, from the start of its space.
//! - `padding`, a number for every edge or `{ left, top, right, bottom }` (0 by default), is the
//!   space between the stack's edges and what it holds.
//! - `width` and `height` are its size, with a `stretch`, as an area has. Each is optional:
//!   without one, or with `false`, it fits what it holds that way, with its padding, and has no
//!   stretch that way. `false` restores fitting in an instance's override of a fixed size.
//! - Its `pivot` is from the point of it its anchor names, its top-left corner without one, so a
//!   stack anchored at its parent's bottom-right corner keeps its own there however much it
//!   holds.
//!
//! ```lua
//! {
//!     name = "menu",
//!     kind = "panel",
//!     y = 80,
//!     anchor = { 0.5, 0 },
//!     gap = 2,
//!     padding = { 6, 5, 6, 5 },
//!     color = "#14182c",
//!     border = 1,
//!     border_color = "#f2d3ab",
//!     children = {
//!         { name = "continue", kind = "text", text = "Continue" },
//!         { name = "new_game", kind = "text", text = "New Game" },
//!     },
//! }
//! ```
//!
//! That's a menu 80 down the canvas, in the middle of it, as wide as its widest line and its
//! padding, with a line of text 12 below the one before. Hiding `continue` leaves `new_game` at
//! the top, and the menu shorter.
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
//!     format = 4,
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
//! A font file, named `*.font.luau` by convention, is plain data too ([`FontFile`]), naming the
//! files it's made of by their paths from its own folder. A bitmap font's glyphs are drawn in a
//! PNG, `image`, divided into cells `cell = { width, height }` texels from its top-left corner:
//!
//! ```lua
//! return {
//!     format = 4,
//!     image = "title.png",
//!     cell = { 14, 14 },
//!     characters = { "ABCDEFGHIJKLMNOP", "QRSTUVWXYZ" },
//!     space = 5,
//! }
//! ```
//!
//! `characters` is which character each cell draws: a string for each row of cells, or one string
//! read cell by cell, on to the next row at the image's right edge. A space skips a cell. Each
//! glyph is as wide as its ink, from its cell's leftmost column with anything in it to its
//! rightmost, unless `monospace` is true, and as tall as its cell, its top at the top of the line.
//! Optionally, `spacing` is the texels after each glyph (1), `space` how far a space moves the pen
//! (half a cell, rounded up, or a whole cell and its spacing if monospace), and `line_height` from
//! one line's top to the next's (a cell's height and 1). A TrueType or OpenType font is its
//! `file`, its `size`, how many pixels its em is, and whether it's `smooth` (true by default), as
//! [`Context::load_vector_font`] takes them:
//!
//! ```lua
//! return { format = 4, file = "Hack-Regular.ttf", size = 13 }
//! ```
//!
//! Nodes draw in order, each before its children, so later ones are on top, each in its own blend
//! mode, leaving the canvas's as it was. A [`Scene`] is a
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
    sync::atomic::{AtomicU64, Ordering},
};

pub use clip::{Clip, Interpolation, KeyValue, Keys, Property, Track};
pub use file::{
    AreaDef, BillboardDef, BitmapFontDef, CellDef, FORMAT, FontCharacters, FontFile, InstanceDef,
    KindDef, MeshDef, ModelDef, NodeDef, PanelDef, PanelFill, PanelLookDef, ParticlesDef,
    Placement3, Props, RectDef, SceneFile, Shape, SheetDef, SpriteDef, Stack, StackDirection,
    TextDef, TileDef, TileLayerDef, TilemapDef, TilesetFile, Value, VectorFontDef, View3dDef,
};
pub(crate) use file::{evaluate_any, particle_settings, particle_settings_value, plain};
pub(crate) use writer::write_value;

use crate::{
    Context, Error, Result,
    graphics::{
        Align, Blend, Camera3d, Canvas, Canvas3d, Color, Font, Lighting, Mesh, Model, Ray, Sprite,
        SpriteSheet, TextLayout, Texture, View3d,
    },
    lua::check_tilemap_size,
    math::{Affine, Affine3, Bounds3, Rect, add3, scale3},
    particles::{ParticleSettings, Particles, seed_for},
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
/// instances place, the tileset files its tilemaps use, the images of its sheets, the font files
/// of its text and the model files of its model nodes, each by its path as the file names it,
/// and the meshes of its mesh nodes' shapes. [`FolderAssets`] reads them from a game's folder; a
/// game can keep them loaded instead.
pub trait Assets {
    fn texture(&mut self, path: &str) -> Result<Texture>;
    fn scene(&mut self, path: &str) -> Result<Rc<SceneFile>>;
    fn tileset(&mut self, path: &str) -> Result<Rc<TilesetFile>>;
    /// The font file at `path`, as [`Context::load_font`] loads one.
    fn font(&mut self, path: &str) -> Result<Font>;
    fn model(&mut self, path: &str) -> Result<Model>;
    /// The mesh of `shape`, as [`Context::create_mesh`] makes one of [`Shape::mesh_data`].
    fn mesh(&mut self, shape: &Shape) -> Result<Mesh>;
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

    fn font(&mut self, path: &str) -> Result<Font> {
        self.ctx.load_font(self.folder.join(path))
    }

    fn model(&mut self, path: &str) -> Result<Model> {
        load_model(self.ctx, &self.folder.join(path))
    }

    fn mesh(&mut self, shape: &Shape) -> Result<Mesh> {
        self.ctx.create_mesh(&shape.mesh_data())
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
    /// Which scene its nodes' IDs are of: one of its own, or the one it was cloned from.
    id: u64,
    /// Which version of its nodes it has: see [`Scene::revision`].
    revision: u64,
    slots: Vec<Slot>,
    /// Slots whose nodes were removed, for new nodes.
    free: Vec<u32>,
    size: [f32; 2],
}

/// Where a node is kept, and how many nodes have been kept there before it, so an ID of a removed
/// node never reaches one added since. A slot whose count would start again is never used again.
#[derive(Clone, Debug)]
struct Slot {
    generation: u32,
    node: Option<Node>,
}

/// Which node of a [`Scene`] is meant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId {
    scene: u64,
    index: u32,
    generation: u32,
}

/// An ID for a new scene's nodes, which no other scene's have.
fn new_scene_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// A scene's next revision, which no scene has had, so no two scenes have the same one unless one
/// is a copy of the other.
fn next_revision() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed) + 1
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
    /// A rectangle that doesn't draw, laying out its children one after another.
    Stack(Stack),
    /// A stack that draws behind its children.
    Panel(PanelNode),
    /// An emitter of particles, which draw where it is among the scene's nodes.
    Particles(ParticlesNode),
}

/// An emitter of particles, sending them out from a rectangle laid out as an area's.
#[derive(Clone, Debug)]
pub struct ParticlesNode {
    pub particles: Particles,
    /// The sheet its particles' sprite is from, whose cells they show with
    /// [`ParticlesNode::set_cell`] and clips.
    pub sheet: Option<SpriteSheet>,
    /// The rectangle particles start in.
    pub size: [f32; 2],
    /// As a rect's.
    pub stretch: [f32; 2],
    /// As a sprite's.
    pub pivot: [f32; 2],
}

impl ParticlesNode {
    /// Shows the sprite in `cell` of its sheet, its column and row. Fails if it has no sheet, or
    /// the sheet has no such cell.
    pub fn set_cell(&mut self, cell: [u32; 2]) -> Result<(), String> {
        let sprite = cell_of(self.sheet.as_ref(), cell)?;
        self.particles.set_sprite(Some(sprite));
        Ok(())
    }
}

/// A stack that draws behind its children.
#[derive(Clone, Debug)]
pub struct PanelNode {
    pub stack: Stack,
    pub look: PanelLook,
}

/// What a panel draws.
#[derive(Clone, Debug)]
pub enum PanelLook {
    Fill(PanelFill),
    /// A sprite, sliced: see [`Canvas::draw_sprite_sliced`].
    Sprite {
        sprite: Sprite,
        /// The texels along its left, top, right and bottom edges that keep their size.
        slice: [f32; 4],
        tint: Color,
    },
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
    /// How it mixes with what's under it.
    pub blend: Blend,
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
    /// As a sprite's.
    pub blend: Blend,
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
    /// As a sprite's.
    pub blend: Blend,
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
    /// How its tiles mix with what's under them.
    pub blend: Blend,
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
            Self::Stack(_) => "stack",
            Self::Panel(_) => "panel",
            Self::Particles(_) => "particles",
        }
    }

    /// How what it draws mixes with what's under it, if it draws in a mode of its own: a
    /// sprite, rect, text, tile layer or particles.
    pub fn blend(&self) -> Option<Blend> {
        match self {
            Self::Sprite(sprite) => Some(sprite.blend),
            Self::Rect(rect) => Some(rect.blend),
            Self::Text(text) => Some(text.blend),
            Self::TileLayer(layer) => Some(layer.blend),
            Self::Particles(particles) => Some(particles.particles.settings().blend),
            _ => None,
        }
    }

    /// Changes how what it draws mixes with what's under it. `None` if it's a kind that has no
    /// blend mode: see [`Kind::blend`].
    pub fn set_blend(&mut self, blend: Blend) -> Option<()> {
        match self {
            Self::Sprite(sprite) => sprite.blend = blend,
            Self::Rect(rect) => rect.blend = blend,
            Self::Text(text) => text.blend = blend,
            Self::TileLayer(layer) => layer.blend = blend,
            Self::Particles(node) => {
                let settings = ParticleSettings {
                    blend,
                    ..node.particles.settings().clone()
                };
                node.particles
                    .set_settings(settings)
                    .expect("only the blend mode changed");
            }
            _ => return None,
        }
        Some(())
    }

    /// How a stack or a panel is laid out and lays out its children.
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
            Self::Particles(particles) => particles.sheet.as_ref(),
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
            Self::Particles(particles) => Some(particles.set_cell(cell)),
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
            blend: Blend::Normal,
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
        // With the same sines on every platform, so what physics places by nodes plays out the
        // same everywhere.
        self.local = Affine::from_parts_exact(self.position, self.rotation, self.scale);
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

    /// The width and height of a sprite, rect, text, area, tilemap, tile layer, 3D view or the
    /// rectangle particles start in, before stretching with its parent. A plain node has none,
    /// and nor do nodes in 3D views. A stack's or a panel's depends on its children, so it's
    /// [`Scene::size_of`].
    pub fn size(&self) -> Option<[f32; 2]> {
        match &self.kind {
            Kind::Node
            | Kind::Mesh(_)
            | Kind::Model(_)
            | Kind::Billboard(_)
            | Kind::Stack(_)
            | Kind::Panel(_) => None,
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
            Kind::Particles(particles) => Some(particles.size),
        }
    }

    /// The rectangle the node covers in its own coordinates, its pivot at the origin, inside a
    /// parent whose rectangle is `outer`, which it may stretch with. A plain node covers nothing,
    /// and nor do nodes in 3D views. Nor does a stack or a panel here: what it covers depends on
    /// its children, so [`Scene::rect_in`] measures it.
    fn rect_in(&self, outer: Rect) -> Option<Rect> {
        let size = self.size()?;
        let (stretch, [x, y]) = match &self.kind {
            Kind::Sprite(sprite) => (sprite.stretch, sprite.pivot),
            Kind::Rect(rect) => (rect.stretch, rect.pivot),
            Kind::Area(area) => (area.stretch, area.pivot),
            Kind::View3d(view) => (view.stretch, view.pivot),
            Kind::Particles(particles) => (particles.stretch, particles.pivot),
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
            | Kind::Billboard(_)
            | Kind::Stack(_)
            | Kind::Panel(_) => ([0.0, 0.0], [0.0, 0.0]),
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
    /// covers, inside its padding for a stack or panel, or for a plain node, its parent's,
    /// carried along with it, or in a stack, the space the stack gives it.
    inner: Rect,
}

impl Placed {
    /// Where a node is that's placed here by `local`, within this one.
    fn within(&self, local: Placed) -> Self {
        Self {
            transform: self.transform * local.transform,
            ..local
        }
    }
}

/// The rectangle inside `rect` its padding leaves, never less than nothing.
fn padded(stack: &Stack, rect: Rect) -> Rect {
    let [left, top, right, bottom] = stack.padding;
    Rect::new(
        rect.x + left,
        rect.y + top,
        (rect.w - left - right).max(0.0),
        (rect.h - top - bottom).max(0.0),
    )
}

/// The smallest rectangle holding both.
fn union(a: Rect, b: Rect) -> Rect {
    let [left, top] = [a.x.min(b.x), a.y.min(b.y)];
    let [right, bottom] = [a.right().max(b.right()), a.bottom().max(b.bottom())];
    Rect::new(left, top, right - left, bottom - top)
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
        let mut scene = Self::empty([0.0, 0.0]);
        scene.node_mut(scene.root()).props = file.props.clone();
        let mut build = Build {
            path,
            font,
            assets,
            fonts: HashMap::new(),
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

    /// A scene of only its root, `size` in size.
    fn empty(size: [f32; 2]) -> Self {
        Self {
            id: new_scene_id(),
            revision: next_revision(),
            slots: vec![Slot {
                generation: 0,
                node: Some(Node::new(String::new(), Kind::Node)),
            }],
            free: Vec::new(),
            size,
        }
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
        if size != self.size {
            self.size = size;
            self.revision = next_revision();
        }
    }

    /// Which version of its nodes the scene has, which changes whenever one of them may have:
    /// with each [`Scene::get_mut`] or [`Scene::node_mut`], whatever's done with the node then,
    /// as nodes are added or removed, and as the scene's size changes. Particles moving on, or
    /// sent out or ended, don't change it. It's never the same as another scene's unless one's a
    /// copy of the other, so what's worked out from where nodes are, such as a physics world's
    /// colliders, need be worked out again only when this changes.
    pub fn revision(&self) -> u64 {
        self.revision
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
                    scene: self.id,
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
                    scene: self.id,
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
            scene: self.id,
            index: 0,
            generation: 0,
        }
    }

    /// The node `id`, if it's in the scene: it hasn't been removed, and isn't from another scene.
    /// A clone of a scene has its nodes' IDs, as a copy of the same nodes, until either adds one.
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        let slot = self.slots.get(id.index as usize)?;
        slot.node
            .as_ref()
            .filter(|_| id.scene == self.id && slot.generation == id.generation)
    }

    /// The node `id`, to change, if it's in the scene, as [`Scene::get`] finds it. This changes
    /// the scene's [`Scene::revision`], whether the node is changed or not.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.revision = next_revision();
        self.get_mut_unrevised(id)
    }

    /// [`Scene::get_mut`], keeping the scene's revision, for what changes nothing it stands for,
    /// such as particles moving on.
    fn get_mut_unrevised(&mut self, id: NodeId) -> Option<&mut Node> {
        let slot = self.slots.get_mut(id.index as usize)?;
        slot.node
            .as_mut()
            .filter(|_| id.scene == self.id && slot.generation == id.generation)
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

    /// Fails unless `id` is a sprite, billboard, mesh or particles node from a sheet with every
    /// cell of `keys`.
    fn check_cells(&self, id: NodeId, keys: &[(f32, [u32; 2])]) -> Result<(), String> {
        let kind = &self.node(id).kind;
        let sheet = match kind.sheet() {
            Some(sheet) => sheet,
            None if matches!(
                kind,
                Kind::Sprite(_) | Kind::Billboard(_) | Kind::Mesh(_) | Kind::Particles(_)
            ) =>
            {
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
        let Some(parent) = node.parent else {
            return Placed {
                transform: node.local,
                rect: None,
                inner: Rect::new(0.0, 0.0, self.size[0], self.size[1]),
            };
        };
        let above = self.placed(parent);
        self.placed_under(&above, parent, id)
    }

    /// Where `id`, a child of `parent`, is laid out, with `parent` laid out at `above`.
    fn placed_under(&self, above: &Placed, parent: NodeId, id: NodeId) -> Placed {
        if let Some(stack) = self.node(parent).kind.stack() {
            let siblings = &self.node(parent).children;
            let index = siblings.iter().position(|&child| child == id);
            let (slots, _) = self.lay_out_stack(parent, stack, above.inner);
            return above.within(slots[index.expect("a node is among its parent's children")]);
        }
        self.child_placed(above, id)
    }

    /// [`Scene::transform`] of the node's parent, if it has one, and of the node, and
    /// [`Scene::rect`] of the node, worked out in one walk up the tree rather than three.
    #[cfg_attr(not(feature = "physics"), expect(dead_code))]
    pub(crate) fn transforms_and_rect(&self, id: NodeId) -> (Option<Affine>, Affine, Option<Rect>) {
        let Some(parent) = self.node(id).parent else {
            let placed = self.placed(id);
            return (None, placed.transform, placed.rect);
        };
        let above = self.placed(parent);
        let placed = self.placed_under(&above, parent, id);
        (Some(above.transform), placed.transform, placed.rect)
    }

    /// Where `id` is, as a child of a node placed at `parent` that isn't a stack.
    fn child_placed(&self, parent: &Placed, id: NodeId) -> Placed {
        let node = self.node(id);
        let rect = self.rect_in(id, parent.inner);
        Placed {
            transform: parent.transform * node.local_in(parent.inner),
            rect,
            inner: self.inner_of(id, rect, parent.inner),
        }
    }

    /// The rectangle the children of `id`, which covers `rect`, anchor and stretch to: what it
    /// covers, inside its padding for a stack or a panel, or for a plain node, `passed`, its
    /// parent's.
    fn inner_of(&self, id: NodeId, rect: Option<Rect>, passed: Rect) -> Rect {
        match (self.node(id).kind.stack(), rect) {
            (Some(stack), Some(rect)) => padded(stack, rect),
            (_, rect) => rect.unwrap_or(passed),
        }
    }

    /// The rectangle `id` covers in its own coordinates, its pivot at the origin, inside a parent
    /// whose rectangle is `outer`, which it may stretch with: see [`Node::rect_in`]. A stack or a
    /// panel covers what it holds along the axes it fits it, with its padding.
    fn rect_in(&self, id: NodeId, outer: Rect) -> Option<Rect> {
        let node = self.node(id);
        let Some(stack) = node.kind.stack() else {
            return node.rect_in(outer);
        };
        let outer = [outer.w, outer.h];
        let mut size = [0.0; 2];
        for i in 0..2 {
            if let Some(own) = stack.size[i] {
                size[i] = if stack.stretch[i] == 0.0 {
                    own
                } else {
                    (own + stack.stretch[i] * outer[i]).max(0.0)
                };
            }
        }
        if stack.size.contains(&None) {
            // What it holds, laid out as wide or as tall as it is along the axes it has a size,
            // and in no space at all along those it fits it, where the children stretching with
            // it count as their own size.
            let fixed = [0, 1].map(|i| {
                if stack.size[i].is_some() {
                    size[i]
                } else {
                    0.0
                }
            });
            let content = padded(stack, Rect::new(0.0, 0.0, fixed[0], fixed[1]));
            let (_, held) = self.lay_out_stack(id, stack, content);
            let [left, top, right, bottom] = stack.padding;
            let padding = [left + right, top + bottom];
            for i in 0..2 {
                if stack.size[i].is_none() {
                    size[i] = (held[i] + padding[i]).max(0.0);
                }
            }
        }
        // From the point its anchor names, by whole units, as text lines up.
        let anchor = node.anchor.unwrap_or([0.0, 0.0]);
        let [x, y] = [0, 1].map(|i| (anchor[i] * size[i]).floor() + stack.pivot[i]);
        Some(Rect::new(-x, -y, size[0], size[1]))
    }

    /// The width and height of `id` before stretching with its parent, if it has any: see
    /// [`Node::size`]. A stack or a panel has its own along the axes it has one, and what it
    /// holds, with its padding, along those it fits it.
    pub fn size_of(&self, id: NodeId) -> Option<[f32; 2]> {
        let node = self.node(id);
        let Some(stack) = node.kind.stack() else {
            return node.size();
        };
        let measured = self.rect_in(id, Rect::default())?;
        let measured = [measured.w, measured.h];
        Some([0, 1].map(|i| stack.size[i].unwrap_or(measured[i])))
    }

    /// Where the children of the stack or panel `id`, laid out by `stack`, are in its
    /// coordinates, laid out in `content`, the rectangle inside its padding, and how much space
    /// they take: along the stack, from the start of the first to the end of the last, and
    /// across, the widest.
    ///
    /// Each child that's shown and covers something takes the space it covers along the stack,
    /// turned and stretched as it's drawn, after the one before and the gap. Across, it's lined
    /// up by its anchor: the same fraction of it at that fraction of `content`, at the start
    /// without an anchor. A plain node's origin is at the start of its space, which its children
    /// anchor in, and it takes as much as they reach past its origin. Either way, its `x` and `y`
    /// move it from there without moving the rest. A hidden child takes no space, and nor does
    /// its gap.
    fn lay_out_stack(&self, id: NodeId, stack: &Stack, content: Rect) -> (Vec<Placed>, [f32; 2]) {
        let main = stack.direction.axis();
        let cross = 1 - main;
        let start = [content.x, content.y];
        let length = [content.w, content.h];
        let children = &self.node(id).children;
        let mut slots = Vec::with_capacity(children.len());
        let mut along = 0.0;
        let mut widest: f32 = 0.0;
        let mut first = true;
        for &child in children {
            let node = self.node(child);
            let rect = self.rect_in(child, content);
            // A plain node's children anchor in the space it's given: as long as `content` is
            // across, from its origin, which is at the start of that space.
            let mut inner = [0.0; 2];
            inner[cross] = length[cross];
            let inner = match rect {
                Some(rect) => self.inner_of(child, Some(rect), content),
                None => Rect::new(0.0, 0.0, inner[0], inner[1]),
            };
            let turned = if node.kind.only_moves() {
                Affine::IDENTITY
            } else {
                Affine::from_parts_exact([0.0, 0.0], node.rotation, node.scale)
            };
            // Where what it covers starts, from its origin, and how far it reaches: for a plain
            // node, whose origin is at the start of its space, as far as its children reach past
            // it, so what's hidden in it never moves the rest.
            let covered = match rect {
                Some(rect) => {
                    let covered = turned.bounds(rect);
                    Some(([covered.x, covered.y], [covered.w, covered.h]))
                }
                None => self
                    .span(child, inner, turned)
                    .map(|reach| ([0.0; 2], [reach.right().max(0.0), reach.bottom().max(0.0)])),
            };
            let mut origin = start;
            origin[main] += along;
            if let Some((min, size)) = covered.filter(|_| node.visible) {
                if !first {
                    along += stack.gap;
                    origin[main] += stack.gap;
                }
                first = false;
                origin[main] -= min[main];
                if rect.is_some() {
                    let fraction = node.anchor.map_or(0.0, |anchor| anchor[cross]);
                    origin[cross] +=
                        (fraction * (length[cross] - size[cross])).floor() - min[cross];
                }
                along += size[main];
                widest = widest.max(size[cross]);
            }
            let [x, y] = node.position;
            let local =
                Affine::from_parts_exact([origin[0] + x, origin[1] + y], node.rotation, node.scale);
            slots.push(Placed {
                transform: local,
                rect,
                inner,
            });
        }
        let mut held = [0.0; 2];
        held[main] = along;
        held[cross] = widest;
        (slots, held)
    }

    /// What the children of the plain node `id` cover, placed in `inner`, then by `transform`:
    /// composed down to each child before finding its bounds, since text only moves.
    fn span(&self, id: NodeId, inner: Rect, transform: Affine) -> Option<Rect> {
        self.node(id)
            .children
            .iter()
            .filter_map(|&child| self.covered(child, inner, transform))
            .reduce(union)
    }

    /// The smallest axis-aligned rectangle around what `id` covers, if it's shown and covers
    /// anything, inside `outer`, then placed by `parent`: what it covers itself, or for a plain
    /// node, what its children do.
    fn covered(&self, id: NodeId, outer: Rect, parent: Affine) -> Option<Rect> {
        let node = self.node(id);
        if !node.visible {
            return None;
        }
        let local = parent * node.local_in(outer);
        match self.rect_in(id, outer) {
            Some(rect) if node.kind.only_moves() => {
                Some(Affine::from_translation(local.translation).bounds(rect))
            }
            Some(rect) => Some(local.bounds(rect)),
            None => self.span(id, outer, local),
        }
    }

    /// Where each child of `id`, placed at `placed`, is, in order.
    fn children_placed(&self, id: NodeId, placed: &Placed) -> Vec<Placed> {
        let node = self.node(id);
        match node.kind.stack() {
            Some(stack) => {
                let (slots, _) = self.lay_out_stack(id, stack, placed.inner);
                slots.into_iter().map(|slot| placed.within(slot)).collect()
            }
            None => node
                .children
                .iter()
                .map(|&child| self.child_placed(placed, child))
                .collect(),
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

    /// Where what the node shows is placed in the scene, and the rectangle it covers in its own
    /// coordinates (see [`Scene::rect`]): by [`Scene::transform`], or only moved by it, for text,
    /// tilemaps and 3D views, which don't turn or stretch. `None` for a node that covers nothing.
    pub fn placement(&self, id: NodeId) -> Option<(Affine, Rect)> {
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

    /// The smallest axis-aligned rectangle in the scene's coordinates around what the node and
    /// the nodes inside it that are visible cover, if they cover anything, as [`Scene::bounds`]
    /// finds for each: so for a plain node, what's in it. A 3D view's nodes count only as the
    /// view does. Menus find their items with it.
    pub fn extent(&self, id: NodeId) -> Option<Rect> {
        self.extent_placed(id, &self.placed(id))
    }

    fn extent_placed(&self, id: NodeId, placed: &Placed) -> Option<Rect> {
        let node = self.node(id);
        let own = placed.rect.map(|rect| {
            if node.kind.only_moves() {
                Affine::from_translation(placed.transform.translation).bounds(rect)
            } else {
                placed.transform.bounds(rect)
            }
        });
        if matches!(node.kind, Kind::View3d(_)) {
            return own;
        }
        let children = self.children_placed(id, placed);
        node.children
            .iter()
            .zip(&children)
            .filter(|&(&child, _)| self.node(child).visible)
            .filter_map(|(&child, placed)| self.extent_placed(child, placed))
            .fold(own, |extent, rect| {
                Some(extent.map_or(rect, |extent| union(extent, rect)))
            })
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

    /// The 3D view `id` as it's drawn: its camera and lighting, and where it is in the scene's
    /// coordinates. `None` if `id` isn't a 3D view.
    pub fn view(&self, id: NodeId) -> Option<View3d> {
        let Kind::View3d(view) = &self.node(id).kind else {
            return None;
        };
        Some(View3d {
            camera: view.camera,
            rect: self.bounds(id)?,
            lighting: view.lighting,
        })
    }

    /// The ray from the camera of the 3D view `view` through `point`, in the scene's coordinates,
    /// along which everything that shows there lies, if the point is in the view. `None` if it
    /// isn't, or `view` isn't a 3D view.
    pub fn ray(&self, view: NodeId, point: [f32; 2]) -> Option<Ray> {
        let view = self.view(view)?;
        view.rect.contains(point).then(|| view.ray(point))
    }

    /// Where `point` in the world of the 3D view `view` shows in the scene's coordinates. `None`
    /// if it's nearer than the camera's near plane or farther than its far one, or `view` isn't a
    /// 3D view.
    pub fn project(&self, view: NodeId, point: [f32; 3]) -> Option<[f32; 2]> {
        self.view(view)?.project(point)
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

    /// Whether `id` and every node it's inside are visible, so it's drawn.
    pub fn is_shown(&self, id: NodeId) -> bool {
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
        // Copied emitters start over, their randomness from their own paths, so copies of a torch
        // don't flicker in step, or carry their source's particles.
        for node in self.descendants(id) {
            let path = self.path(node);
            if let Kind::Particles(particles) = &mut self.node_mut(node).kind {
                particles.particles.restart(seed_for(&path));
            }
        }
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
        let mut scene = Scene::empty(self.size);
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
        // Changes the revision.
        self.node_mut(parent).children.retain(|&child| child != id);
        for removed in self.descendants(id) {
            let slot = &mut self.slots[removed.index as usize];
            slot.node = None;
            // A slot whose generations have run out is left empty, so no ID leads to it again.
            if let Some(generation) = slot.generation.checked_add(1) {
                slot.generation = generation;
                self.free.push(removed.index);
            }
        }
        Ok(())
    }

    /// Moves the particles of `id`, and of every particles node inside it, on by `dt` seconds,
    /// sending new ones out from where each node is now laid out, as [`Particles::advance`] does.
    /// Particles advance whether their nodes are visible or not. Scenes keep no time of their
    /// own, so particles move only when a game advances them.
    pub fn advance_particles(&mut self, id: NodeId, dt: f32) {
        for node in self.descendants(id) {
            if let Some(particles) = self.placed_particles(node) {
                particles.advance(dt);
            }
        }
    }

    /// The particles of `id`, if it's a particles node, sending them out from where it's laid
    /// out now.
    fn placed_particles(&mut self, id: NodeId) -> Option<&mut Particles> {
        if !matches!(self.get(id)?.kind, Kind::Particles(_)) {
            return None;
        }
        let placed = self.placed(id);
        let Kind::Particles(node) = &mut self.get_mut_unrevised(id)?.kind else {
            return None;
        };
        let area = placed.rect.unwrap_or_default();
        node.particles.set_origin(placed.transform, area);
        Some(&mut node.particles)
    }

    /// Sends `count` particles out at once from `id`, a particles node, where it's laid out now,
    /// as [`Particles::emit`] does. `None` if it isn't a particles node.
    pub fn emit(&mut self, id: NodeId, count: u32) -> Option<()> {
        self.placed_particles(id)?.emit(count);
        Some(())
    }

    /// How many particles `id`, and every particles node inside it, have alive.
    pub fn particle_count(&self, id: NodeId) -> usize {
        self.descendants(id)
            .into_iter()
            .map(|node| match &self.node(node).kind {
                Kind::Particles(particles) => particles.particles.count(),
                _ => 0,
            })
            .sum()
    }

    /// Ends the lives of the particles of `id`, and of every particles node inside it, at once.
    pub fn clear_particles(&mut self, id: NodeId) {
        for node in self.descendants(id) {
            if let Some(Kind::Particles(particles)) =
                self.get_mut_unrevised(node).map(|node| &mut node.kind)
            {
                particles.particles.clear();
            }
        }
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
        // Nodes draw in their own blend modes, and leave the canvas's as it was.
        let blend = canvas.blend();
        self.draw_tree(canvas, id, &self.placed(id), visible);
        canvas.set_blend(blend);
    }

    /// Draws `id`, placed at `placed`, and the nodes inside it.
    fn draw_tree(&self, canvas: &mut Canvas, id: NodeId, placed: &Placed, visible: Rect) {
        let node = self.node(id);
        if !node.visible {
            return;
        }
        let transform = placed.transform;
        if let Some(blend) = node.kind.blend() {
            canvas.set_blend(blend);
        }
        if let Some(rect) = placed.rect {
            match &node.kind {
                Kind::Particles(particles) => {
                    // Particles fly out of their rectangle, so each is culled on its own.
                    canvas.draw_particles_from(&particles.particles, transform);
                }
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
                Kind::Panel(panel) => {
                    // Panels have no blend mode of their own, so draw normally whatever drew
                    // before them.
                    canvas.set_blend(Blend::Normal);
                    if transform.bounds(rect).overlaps(&visible) {
                        draw_panel(canvas, &panel.look, rect, transform);
                    }
                }
                Kind::Node
                | Kind::Area(_)
                | Kind::Stack(_)
                | Kind::Tilemap(_)
                | Kind::Mesh(_)
                | Kind::Model(_)
                | Kind::Billboard(_) => {}
            }
        }
        if node.kind.stack().is_some() {
            let places = self.children_placed(id, placed);
            for (&child, child_placed) in node.children.iter().zip(&places) {
                self.draw_tree(canvas, child, child_placed, visible);
            }
        } else {
            for &child in &node.children {
                let child_placed = self.child_placed(placed, child);
                self.draw_tree(canvas, child, &child_placed, visible);
            }
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

/// Draws a panel's look over `rect`, placed by `transform`: its fill inside its border, then the
/// border, its sides not overlapping, so a translucent one is even all round; or its sprite,
/// sliced.
fn draw_panel(canvas: &mut Canvas, look: &PanelLook, rect: Rect, transform: Affine) {
    match look {
        PanelLook::Fill(fill) => {
            let border = fill.border.min(rect.w / 2.0).min(rect.h / 2.0).max(0.0);
            if border == 0.0 {
                canvas.fill_rect_transformed(rect, transform, fill.color);
                return;
            }
            let [left, top, right, bottom] = [rect.x, rect.y, rect.right(), rect.bottom()];
            let inside = Rect::new(
                left + border,
                top + border,
                rect.w - border * 2.0,
                rect.h - border * 2.0,
            );
            if inside.w > 0.0 && inside.h > 0.0 {
                canvas.fill_rect_transformed(inside, transform, fill.color);
            }
            for side in [
                Rect::new(left, top, rect.w, border),
                Rect::new(left, top + border, border, inside.h),
                Rect::new(right - border, top + border, border, inside.h),
                Rect::new(left, bottom - border, rect.w, border),
            ] {
                if side.w > 0.0 && side.h > 0.0 {
                    canvas.fill_rect_transformed(side, transform, fill.border_color);
                }
            }
        }
        PanelLook::Sprite {
            sprite,
            slice,
            tint,
        } => canvas.draw_sprite_sliced_transformed(sprite, *slice, rect, transform, *tint),
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
        blend: layer.blend,
    })
}

/// Instances, expanded: the nodes of the scene files they place in their stead, with their
/// overrides made. Sprites then name their sheets by their files too ([`sheet_key`]), since
/// nodes from different files are together.
struct Expand<'a> {
    assets: &'a mut dyn Assets,
    /// The scene files being expanded, innermost last, to catch one that places itself.
    stack: Vec<String>,
    /// Every sheet of every file, by [`sheet_key`].
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

/// The key of the sheet `sheet` of the file at `file` among the sheets of every file: the length
/// of the file's path, then the path and the name, so either can hold any character and the key
/// still splits as it was made ([`split_sheet_key`]).
fn sheet_key(file: &str, sheet: &str) -> String {
    format!("{}:{file}{sheet}", file.len())
}

/// The file and the name of the sheet whose key is `key`, if it's one [`sheet_key`] made.
fn split_sheet_key(key: &str) -> Option<(&str, &str)> {
    let (length, rest) = key.split_once(':')?;
    rest.split_at_checked(length.parse().ok()?)
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
                })
                | KindDef::Panel(PanelDef {
                    look:
                        PanelLookDef::Sprite {
                            sprite: CellDef { sheet, .. },
                            ..
                        },
                    ..
                })
                | KindDef::Particles(ParticlesDef {
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
        &mut self,
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
        // node shows, or for a mesh showing none, the file the node is from.
        let mut change = change.clone();
        if let Some(Value::String(sheet)) = change.get_mut("sheet") {
            let shown = match &node.kind {
                KindDef::Sprite(sprite) => Some(&sprite.sheet),
                KindDef::Billboard(billboard) => Some(&billboard.sheet),
                KindDef::Mesh(mesh) => mesh.sprite.as_ref().map(|sprite| &sprite.sheet),
                KindDef::Particles(particles) => {
                    particles.sprite.as_ref().map(|sprite| &sprite.sheet)
                }
                KindDef::Panel(PanelDef {
                    look: PanelLookDef::Sprite { sprite, .. },
                    ..
                }) => Some(&sprite.sheet),
                _ => None,
            };
            let from = match shown.and_then(|key| split_sheet_key(key)) {
                Some((from, _)) => from.to_owned(),
                None => self.file_of(file, target),
            };
            let key = sheet_key(&from, sheet);
            if !self.sheets.contains_key(&key) {
                return Err(format!("there's no sheet `{sheet}` in `{from}`'s `sheets`"));
            }
            *sheet = key;
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

    /// The scene file the node at `target`, a path of names among the nodes of the scene file
    /// `file` with its instances expanded, is from: `file`, or a file one of its instances
    /// places, through their instances. `file` if it can't tell.
    fn file_of(&mut self, file: &str, target: &str) -> String {
        let mut from = file.to_owned();
        let Ok(mut scene) = self.assets.scene(&from) else {
            return from;
        };
        // The indices of the nodes the path leads through, in `scene`.
        let mut at: Vec<usize> = Vec::new();
        for name in target.split('/') {
            let children = children_at(&scene.children, &at);
            if let Some(i) = children.iter().position(|node| node.name == name) {
                at.push(i);
                continue;
            }
            // Not among the node's own children, so among those of the scene file it places.
            let placed = match at.split_last() {
                Some((&last, above)) => match &children_at(&scene.children, above)[last].kind {
                    KindDef::Instance(instance) => instance.scene.clone(),
                    _ => return file.to_owned(),
                },
                None => return file.to_owned(),
            };
            let Ok(placed_scene) = self.assets.scene(&placed) else {
                return file.to_owned();
            };
            let Some(i) = placed_scene.children.iter().position(|n| n.name == name) else {
                return file.to_owned();
            };
            from = placed;
            scene = placed_scene;
            at = vec![i];
        }
        from
    }
}

/// The children of the node the indices `at` lead to among `nodes`, or `nodes` for none.
fn children_at<'a>(nodes: &'a [NodeDef], at: &[usize]) -> &'a [NodeDef] {
    at.iter().fold(nodes, |nodes, &i| &nodes[i].children)
}

/// What building a scene's nodes needs: the scene file's path, for errors, the font for text that
/// names none, where its tilesets, images and fonts come from, and those loaded so far.
struct Build<'a> {
    path: &'a str,
    font: &'a Font,
    assets: &'a mut dyn Assets,
    fonts: HashMap<String, Font>,
    /// Every sheet of every file, by [`sheet_key`], from [`Expand`].
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

    /// The sprite sheet whose [`sheet_key`] is `key`, loading it if it hasn't been.
    fn sheet(&mut self, key: &str, node: &str) -> Result<&SpriteSheet> {
        if !self.sheets.contains_key(key) {
            let Some(def) = self.sheet_defs.get(key) else {
                let (_, name) = split_sheet_key(key).unwrap_or(("", key));
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

    /// The font file at `path`, loading it if it hasn't been.
    fn font_file(&mut self, path: &str) -> Result<Font> {
        if let Some(font) = self.fonts.get(path) {
            return Ok(font.clone());
        }
        let font = self.assets.font(path)?;
        self.fonts.insert(path.to_owned(), font.clone());
        Ok(font)
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
                    blend: sprite.blend,
                })
            }
            KindDef::Rect(rect) => Kind::Rect(RectNode {
                size: rect.size,
                stretch: rect.stretch,
                pivot: rect.pivot,
                color: rect.color,
                blend: rect.blend,
            }),
            KindDef::Text(text) => {
                let font = match &text.font {
                    Some(path) => self.font_file(path)?,
                    None => self.font.clone(),
                };
                let font = font
                    .try_scaled(text.font_scale)
                    .map_err(|err| self.error(node, &err))?;
                Kind::Text(TextNode {
                    pivot: text.pivot,
                    color: text.color,
                    blend: text.blend,
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
            KindDef::Stack(stack) => Kind::Stack(*stack),
            KindDef::Panel(panel) => {
                let look = match &panel.look {
                    PanelLookDef::Fill(fill) => PanelLook::Fill(*fill),
                    PanelLookDef::Sprite {
                        sprite,
                        slice,
                        tint,
                    } => PanelLook::Sprite {
                        sprite: self.sprite(&sprite.sheet, sprite.cell, node)?.0,
                        slice: *slice,
                        tint: *tint,
                    },
                };
                Kind::Panel(PanelNode {
                    stack: panel.stack,
                    look,
                })
            }
            KindDef::Particles(def) => {
                let (sprite, sheet) = match &def.sprite {
                    Some(sprite) => {
                        let (image, sheet) = self.sprite(&sprite.sheet, sprite.cell, node)?;
                        (Some(image), Some(sheet))
                    }
                    None => (None, None),
                };
                // Seeded by the node's path, so emitters alike differ, but each plays the same
                // every time.
                let particles = Particles::try_new(def.settings.clone(), sprite, seed_for(node))
                    .map_err(|err| self.error(node, &err))?;
                Kind::Particles(ParticlesNode {
                    particles,
                    sheet,
                    size: def.size,
                    stretch: def.stretch,
                    pivot: def.pivot,
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
        let mesh = self.assets.mesh(shape)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_are_left_empty_before_their_generations_start_again() {
        let mut scene = Scene::empty([0.0, 0.0]);
        let root = scene.root();
        let first = scene.attach(Node::new("a".into(), Kind::Node), root);
        scene.slots[first.index as usize].generation = u32::MAX;
        let last = NodeId {
            generation: u32::MAX,
            ..first
        };
        scene.remove(last).unwrap();
        let next = scene.attach(Node::new("b".into(), Kind::Node), root);
        assert_ne!(next.index, first.index, "a slot of its own");
        assert!(!scene.contains_node(first) && !scene.contains_node(last));
    }

    #[test]
    fn revisions_change_with_nodes_but_not_particles() {
        let mut scene = Scene::empty([10.0, 10.0]);
        let root = scene.root();
        let mut revision = scene.revision();
        let mut changed = |scene: &Scene| {
            let now = scene.revision();
            let changed = now != revision;
            revision = now;
            changed
        };
        let node = scene.attach(Node::new("a".into(), Kind::Node), root);
        assert!(changed(&scene), "a node added");
        scene.node_mut(node).set_position([1.0, 2.0]);
        assert!(changed(&scene), "a node moved");
        scene.set_size([10.0, 10.0]);
        assert!(!changed(&scene), "the same size");
        scene.set_size([20.0, 10.0]);
        assert!(changed(&scene), "another size");
        let _ = scene.transform(node);
        assert!(!changed(&scene), "only read");
        scene.advance_particles(root, 1.0);
        scene.clear_particles(root);
        assert!(!changed(&scene), "particles");
        scene.remove(node).unwrap();
        assert!(changed(&scene), "a node removed");
        assert_ne!(Scene::empty([0.0; 2]).revision(), scene.revision());
    }

    #[test]
    fn ids_lead_only_to_their_own_scenes_nodes() {
        let [mut a, mut b] = [(); 2].map(|_| Scene::empty([0.0, 0.0]));
        let in_a = a.attach(Node::new("a".into(), Kind::Node), a.root());
        let in_b = b.attach(Node::new("b".into(), Kind::Node), b.root());
        assert!(a.get(in_b).is_none() && b.get(in_a).is_none());
        assert!(a.get(b.root()).is_none());
        // A clone is a copy of the same nodes.
        assert_eq!(a.clone().node(in_a).name(), "a");
    }
}
