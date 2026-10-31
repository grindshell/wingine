//! Scenes: trees of sprites, rectangles, text and areas, laid out in files that people and tools
//! can both write, for levels, screens and characters.
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
//!             name = "pond",
//!             kind = "area",
//!             x = 40,
//!             y = 96,
//!             width = 48,
//!             height = 32,
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
//!
//! What a sprite, rect, text or area shows has its top-left corner at the node's origin, unless
//! it has a `pivot`: the point of it, from its top-left corner, `{ x, y }`, that's at the origin
//! instead, and that it turns and stretches about. A sprite of a person 16 texels tall standing
//! at `x`, `y` has a pivot of `{ 8, 16 }`, the middle of its feet. Colors are hex strings, such as
//! `"#f2c14e"`, or `"#00000080"` with alpha. Any field a node doesn't have is an error naming the
//! node, as is a missing one it must have.
//!
//! Nodes draw in order, each before its children, so later ones are on top. A [`Scene`] is a
//! scene loaded to play: its nodes, which a game finds by path or tag and moves, turns and
//! changes, drawn with [`Scene::draw`].

mod file;
mod writer;

use std::{collections::BTreeMap, fs, path::Path};

pub use file::{
    AreaDef, FORMAT, KindDef, NodeDef, Props, RectDef, SceneFile, SheetDef, SpriteDef, TextDef,
    Value,
};

use crate::{
    Context, Error, Result,
    graphics::{Canvas, Color, Font, Sprite, SpriteSheet, TextLayout, Texture},
    math::{Affine, Rect},
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
        let path = path.as_ref();
        fs::write(path, self.to_luau()?).map_err(|source| Error::WriteFile {
            path: path.to_owned(),
            source,
        })
    }
}

/// A scene loaded to play: a tree of nodes, which a game finds, moves, turns and changes, and
/// draws.
///
/// The root is a plain node holding the scene's props, with the file's nodes as its children.
/// Cloning a scene copies its nodes, sharing the images they show.
#[derive(Clone, Debug)]
pub struct Scene {
    /// The root first, then every other node, each after its parent.
    nodes: Vec<Node>,
}

/// Which node of a [`Scene`] is meant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(usize);

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

impl Kind {
    /// Its name in scene files, such as `"sprite"`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Sprite(_) => "sprite",
            Self::Rect(_) => "rect",
            Self::Text(_) => "text",
            Self::Area(_) => "area",
        }
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

    /// The rectangle a sprite, rect, text or area covers, in the node's own coordinates, its
    /// pivot at the origin. A plain node covers nothing.
    pub fn rect(&self) -> Option<Rect> {
        let (size, [x, y]) = match &self.kind {
            Kind::Node => return None,
            Kind::Sprite(sprite) => (sprite.size, sprite.pivot),
            Kind::Rect(rect) => (rect.size, rect.pivot),
            Kind::Text(text) => (text.layout.size(), text.pivot),
            Kind::Area(area) => (area.size, area.pivot),
        };
        Some(Rect::new(-x, -y, size[0], size[1]))
    }
}

impl Scene {
    /// The scene `file` describes, its text in `font`, loading each of its sheets' images with
    /// `texture`. Errors name the file `path`, such as a sprite outside its sheet.
    pub fn new(
        file: &SceneFile,
        path: impl AsRef<Path>,
        font: &Font,
        mut texture: impl FnMut(&SheetDef) -> Result<Texture>,
    ) -> Result<Self> {
        let path = path.as_ref();
        let mut sheets = BTreeMap::new();
        for (name, def) in &file.sheets {
            let sheet = SpriteSheet::try_new(texture(def)?, def.tile_width, def.tile_height)
                .map_err(|err| Error::Scene {
                    path: path.to_owned(),
                    message: format!("sheet `{name}`: {err}"),
                })?;
            sheets.insert(name.as_str(), sheet);
        }
        let mut root = Node::new(String::new(), Kind::Node);
        root.props = file.props.clone();
        let mut scene = Self { nodes: vec![root] };
        let build = Build { path, sheets, font };
        for child in &file.children {
            scene.add(&build, child, NodeId(0), "")?;
        }
        Ok(scene)
    }

    /// Reads the scene file at `path` in `folder`, and loads its sheets' images from `folder`.
    pub fn load(
        ctx: &mut Context,
        folder: impl AsRef<Path>,
        path: impl AsRef<Path>,
    ) -> Result<Self> {
        let folder = folder.as_ref();
        let path = folder.join(path);
        let file = SceneFile::read(&path)?;
        let font = ctx.default_font().clone();
        Self::new(&file, &path, &font, |sheet| {
            ctx.load_texture(folder.join(&sheet.path))
        })
    }

    /// Adds the node `def` describes, and its children, at the end of `parent`'s children.
    fn add(&mut self, build: &Build, def: &NodeDef, parent: NodeId, path: &str) -> Result<()> {
        let path = if path.is_empty() {
            def.name.clone()
        } else {
            format!("{path}/{}", def.name)
        };
        let kind = build.kind(&def.kind, &path)?;
        let mut node = Node::new(def.name.clone(), kind);
        node.parent = Some(parent);
        node.position = [def.x, def.y];
        node.rotation = def.rotation;
        node.scale = [def.scale_x, def.scale_y];
        node.update_local();
        node.visible = def.visible;
        node.tags = def.tags.clone();
        node.props = def.props.clone();
        let id = NodeId(self.nodes.len());
        self.nodes.push(node);
        self.nodes[parent.0].children.push(id);
        for child in &def.children {
            self.add(build, child, id, &path)?;
        }
        Ok(())
    }

    /// The node at the top of the tree, which holds the scene's props.
    pub fn root(&self) -> NodeId {
        NodeId(0)
    }

    /// # Panics
    ///
    /// If `id` is from another scene with more nodes.
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0]
    }

    /// # Panics
    ///
    /// If `id` is from another scene with more nodes.
    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.0]
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

    /// The smallest axis-aligned rectangle in the scene's coordinates around what the node
    /// covers, if it covers anything: see [`Node::rect`].
    pub fn bounds(&self, id: NodeId) -> Option<Rect> {
        let rect = self.node(id).rect()?;
        Some(self.transform(id).bounds(rect))
    }

    /// Whether `point`, in the scene's coordinates, is inside what the node covers, turned and
    /// stretched as it's drawn. Nothing is inside a plain node, or one squashed flat.
    pub fn contains(&self, id: NodeId, point: [f32; 2]) -> bool {
        let Some(rect) = self.node(id).rect() else {
            return false;
        };
        let Some(inverse) = self.transform(id).inverse() else {
            return false;
        };
        rect.contains(inverse.apply(point))
    }

    /// Draws the whole scene.
    pub fn draw(&self, canvas: &mut Canvas) {
        self.draw_node(canvas, self.root());
    }

    /// Draws `id` and the nodes inside it where they are in the scene, each before its children.
    /// Nothing is drawn where it's off the canvas, or where a node, or a node it's inside, isn't
    /// visible.
    pub fn draw_node(&self, canvas: &mut Canvas, id: NodeId) {
        let mut parent = Affine::IDENTITY;
        let mut at = id;
        while let Some(above) = self.node(at).parent {
            if !self.node(above).visible {
                return;
            }
            at = above;
        }
        if let Some(above) = self.node(id).parent {
            parent = self.transform(above);
        }
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
                    let [x, y] = transform.apply([rect.x, rect.y]);
                    if Rect::new(x, y, rect.w, rect.h).overlaps(&visible) {
                        canvas.draw_text_layout(&text.layout, .., [x, y], text.color);
                    }
                }
                Kind::Node | Kind::Area(_) => {}
            }
        }
        for &child in &node.children {
            self.draw_tree(canvas, child, transform, visible);
        }
    }
}

/// What building a scene's nodes from a file needs: the file's path, for errors, its sheets, and
/// the font for its text.
struct Build<'a> {
    path: &'a Path,
    sheets: BTreeMap<&'a str, SpriteSheet>,
    font: &'a Font,
}

impl Build<'_> {
    /// What the node at `node` shows, as `def` describes it.
    fn kind(&self, def: &KindDef, node: &str) -> Result<Kind> {
        let error = |message: String| Error::Scene {
            path: self.path.to_owned(),
            message: format!("node `{node}`: {message}"),
        };
        let kind = match def {
            KindDef::Node => Kind::Node,
            KindDef::Sprite(sprite) => {
                let Some(sheet) = self.sheets.get(sprite.sheet.as_str()) else {
                    let message = format!("there's no sheet `{}` in the scene", sprite.sheet);
                    return Err(error(message));
                };
                let [column, row] = sprite.cell;
                let image = sheet.try_sprite(column, row).map_err(error)?;
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
                let font = self.font.try_scaled(text.font_scale).map_err(error)?;
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
        };
        Ok(kind)
    }
}
