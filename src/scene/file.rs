//! What scene and tileset files hold, and reading them.
//!
//! A file runs as Luau with nothing in scope, and what it returns is turned into plain [`Value`]s
//! first, then read into a [`SceneFile`] or a [`TilesetFile`]. Instances' overrides are checked
//! by the same reading: the node they change, as plain values, with their fields put in.

use std::{collections::BTreeMap, fs, path::Path};

use super::clip::{Clip, Interpolation, Keys, Property, Track};
use crate::{
    Error, Result,
    graphics::{Align, Color},
    lua::{
        mlua::{self, chunk::ChunkMode},
        parse_hex, unpacked_color,
    },
};

/// The version of the scene and tileset formats this engine reads and writes, their `format`.
pub const FORMAT: u32 = 1;

/// How deeply tables may nest. Files written by hand or by an editor come nowhere near, but one
/// that builds a table holding itself would otherwise never finish being read.
const MAX_DEPTH: usize = 200;

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
    /// Whether it and its children draw.
    pub visible: bool,
    /// Names for finding it and nodes like it, such as `"solid"`.
    pub tags: Vec<String>,
    /// Data for the game, which the engine doesn't read.
    pub props: Props,
    /// The nodes inside it, in the order they draw, after it.
    pub children: Vec<NodeDef>,
}

impl NodeDef {
    /// A node of `kind` named `name` at its parent's origin, unturned and unstretched, visible,
    /// with no tags, props or children.
    pub fn new(name: impl Into<String>, kind: KindDef) -> Self {
        Self {
            name: name.into(),
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
        }
    }
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
}

/// Text in the built-in font.
#[derive(Clone, Debug, PartialEq)]
pub struct TextDef {
    pub text: String,
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

fn read_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| Error::ReadFile {
        path: path.to_owned(),
        source,
    })
}

/// Runs the Luau `source` with nothing in scope, and returns what it returns as plain data.
fn evaluate(source: &str, path: &Path) -> Result<Value, String> {
    let lua = mlua::Lua::new();
    let value: mlua::Value = lua
        .load(source)
        .set_name(format!("@{}", path.display()))
        .set_mode(ChunkMode::Text)
        .set_environment(lua.create_table().map_err(|err| err.to_string())?)
        .eval()
        .map_err(|err| err.to_string())?;
    if !value.is_table() {
        return Err(format!(
            "the file must return a table, not {}",
            describe(&value)
        ));
    }
    plain(value, "", 0)
}

/// `value` as plain data. `at` is where it is, such as `children[2].props`, for errors.
fn plain(value: mlua::Value, at: &str, depth: usize) -> Result<Value, String> {
    let here = || {
        if at.is_empty() {
            "the file's table".to_owned()
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
                        let value = plain(value, &field_path(at, &key), depth + 1)?;
                        strings.insert(key, value);
                    }
                    mlua::Value::Integer(_) | mlua::Value::Number(_) => {
                        let index = match key {
                            mlua::Value::Integer(index) => index as f64,
                            mlua::Value::Number(index) => index,
                            _ => unreachable!(),
                        };
                        let value = plain(value, &format!("{at}[{index}]"), depth + 1)?;
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
    match (at.is_empty(), super::is_identifier(key)) {
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
                     scale_y, visible or cell"
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

fn check_format(format: Option<Value>, what: &str) -> Result<(), String> {
    let Some(format) = format else {
        return Err(format!(
            "the {what} has no `format`; this engine reads {FORMAT}"
        ));
    };
    match whole(format, "format").map_err(|err| format!("the {what}: {err}"))? {
        FORMAT => Ok(()),
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

/// What the nodes being read are inside: the path of the node, or `""` for the scene, and the
/// columns and rows of the map if it's a tilemap, whose layers they may be.
#[derive(Clone, Copy)]
pub(super) struct Parent<'a> {
    pub path: &'a str,
    pub tilemap: Option<[u32; 2]>,
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
    if depth >= MAX_DEPTH {
        return Err(format!(
            "{owner}: nodes nest more than {MAX_DEPTH} deep, too deep to read"
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
    };
    node.children = read_children(fields.take("children"), &inside, sheets, depth + 1)?;
    fields.finish(&super::a_node_of(&kind_name)).map_err(at)?;
    Ok(node)
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
            })
        }
        "rect" => KindDef::Rect(RectDef {
            size: read_size(fields, "a rect node")?,
            stretch: read_stretch(fields)?,
            pivot: read_pivot(fields)?,
            color: optional_color(fields, "color")?,
        }),
        "text" => {
            let text = fields.take("text").ok_or("a text node has a `text`")?;
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
                font_scale,
                wrap,
                align,
                pivot: read_pivot(fields)?,
                color: optional_color(fields, "color")?,
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
        other => {
            return Err(format!(
                "there's no kind of node `{other}`: a node is a node, sprite, rect, text, area, \
                 tilemap, tile_layer or instance"
            ));
        }
    };
    Ok(kind)
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
    Ok(TileLayerDef { legend, tiles })
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
    set("rotation", number(node.rotation));
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
        }
        KindDef::Rect(rect) => {
            set("width", number(rect.size[0]));
            set("height", number(rect.size[1]));
            set("stretch", pair(rect.stretch));
            set("pivot", pair(rect.pivot));
            set("color", hex(rect.color));
        }
        KindDef::Text(text) => {
            set("text", string(&text.text));
            set("font_scale", number(text.font_scale));
            if let Some(wrap) = text.wrap {
                set("wrap", number(wrap));
            }
            set("align", string(text.align.name()));
            set("pivot", pair(text.pivot));
            set("color", hex(text.color));
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
    }
    fields
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
                        stretch = { 1, 0 }, color = "#21406a80",
                    },
                    { name = "title", kind = "text", text = "Hi", font_scale = 2, wrap = 100, align = "center" },
                    { name = "gate", kind = "area", width = 16, height = 8, props = { to = "town" } },
                    {
                        name = "map", kind = "tilemap", x = 8, tileset = "world.tileset.luau",
                        columns = 3, rows = 2,
                        children = {
                            { name = "ground", kind = "tile_layer", legend = { ["."] = "grass", T = "tree" }, tiles = { "..T", "T. " } },
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
            })
        );
        assert_eq!(
            scene.children[2].kind,
            KindDef::Text(TextDef {
                text: "Hi".into(),
                font_scale: 2.0,
                wrap: Some(100.0),
                align: Align::Center,
                pivot: [0.0, 0.0],
                color: Color::WHITE,
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
        let err = error("return { format = 2 }");
        assert!(
            err.contains("format 2, newer than this engine reads (1)"),
            "{err}"
        );
        assert!(parse("return { format = 1 }").is_ok());
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
}
