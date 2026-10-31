//! What scene files hold, and reading them.

use std::{collections::BTreeMap, fs, path::Path};

use crate::{
    Error, Result,
    graphics::Color,
    lua::mlua::{self, chunk::ChunkMode},
};

/// The version of the scene format this engine reads and writes, a scene's `format`.
pub const FORMAT: u32 = 1;

/// How deeply nodes and props may nest. Files written by hand or by an editor come nowhere near,
/// but one that builds a table holding itself would otherwise never finish being read.
const MAX_DEPTH: usize = 200;

/// What a scene file holds: the sprite sheets its sprites come from, the scene's own props, and
/// its nodes. See the [module docs](super) for the format.
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
    /// Where its origin is in its parent's coordinates.
    pub x: f32,
    pub y: f32,
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
    /// The point of the text, from its top-left corner, at the node's origin. Text moves with
    /// its node but doesn't turn or stretch.
    pub pivot: [f32; 2],
    pub color: Color,
}

/// A rectangle that doesn't draw, for marking out part of a scene, such as somewhere the player
/// can't walk or something happens.
#[derive(Clone, Debug, PartialEq)]
pub struct AreaDef {
    pub size: [f32; 2],
    /// As a sprite's.
    pub pivot: [f32; 2],
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
        let source = fs::read_to_string(path).map_err(|source| Error::ReadFile {
            path: path.to_owned(),
            source,
        })?;
        Self::parse(&source, path)
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
        let lua = mlua::Lua::new();
        let scene: mlua::Value = lua
            .load(source)
            .set_name(format!("@{}", path.display()))
            .set_mode(ChunkMode::Text)
            .set_environment(lua.create_table()?)
            .eval()
            .map_err(|err| error(err.to_string()))?;
        read_scene(&lua, scene).map_err(error)
    }
}

fn read_scene(lua: &mlua::Lua, scene: mlua::Value) -> Result<SceneFile, String> {
    let at_scene = |err: String| format!("the scene: {err}");
    let mut fields = Fields::new(scene, "a scene").map_err(at_scene)?;
    match fields.take("format") {
        Some(format) => check_format(whole(format, "format").map_err(at_scene)?)?,
        None => {
            return Err(format!(
                "the scene has no `format`; this engine reads {FORMAT}"
            ));
        }
    }
    let sheets = match fields.take("sheets") {
        Some(sheets) => read_sheets(sheets)?,
        None => BTreeMap::new(),
    };
    let props = read_props(fields.take("props"), 0).map_err(at_scene)?;
    let children = read_children(lua, fields.take("children"), "", &sheets, 0)?;
    fields.finish("the scene").map_err(at_scene)?;
    Ok(SceneFile {
        sheets,
        props,
        children,
    })
}

fn check_format(format: u32) -> Result<(), String> {
    match format {
        FORMAT => Ok(()),
        newer if newer > FORMAT => Err(format!(
            "the scene is format {newer}, newer than this engine reads ({FORMAT})"
        )),
        other => Err(format!("there's no scene format {other}")),
    }
}

fn read_sheets(sheets: mlua::Value) -> Result<BTreeMap<String, SheetDef>, String> {
    let fields = Fields::new(sheets, "`sheets`").map_err(|err| format!("the scene: {err}"))?;
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

/// The nodes of a list, the children of the node at `parent`, or of the scene for `""`.
fn read_children(
    lua: &mlua::Lua,
    children: Option<mlua::Value>,
    parent: &str,
    sheets: &BTreeMap<String, SheetDef>,
    depth: usize,
) -> Result<Vec<NodeDef>, String> {
    let Some(children) = children else {
        return Ok(Vec::new());
    };
    let owner = if parent.is_empty() {
        "the scene".to_owned()
    } else {
        format!("node `{parent}`")
    };
    if depth >= MAX_DEPTH {
        return Err(format!(
            "{owner}: nodes nest more than {MAX_DEPTH} deep, too deep to read"
        ));
    }
    let list = list(children, "children").map_err(|err| format!("{owner}: {err}"))?;
    let mut nodes: Vec<NodeDef> = Vec::with_capacity(list.len());
    for (i, child) in list.into_iter().enumerate() {
        let node = read_node(lua, child, parent, i + 1, sheets, depth)?;
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

fn read_node(
    lua: &mlua::Lua,
    node: mlua::Value,
    parent: &str,
    number: usize,
    sheets: &BTreeMap<String, SheetDef>,
    depth: usize,
) -> Result<NodeDef, String> {
    let unnamed = |err: String| {
        if parent.is_empty() {
            format!("the scene's child {number}: {err}")
        } else {
            format!("node `{parent}`'s child {number}: {err}")
        }
    };
    let mut fields = Fields::new(node, "a node").map_err(unnamed)?;
    let name = fields
        .take("name")
        .ok_or_else(|| unnamed("a node has a `name`".into()))?;
    let name = string(name, "name").map_err(unnamed)?;
    check_name(&name).map_err(unnamed)?;
    let path = if parent.is_empty() {
        name.clone()
    } else {
        format!("{parent}/{name}")
    };
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
    if let Some(visible) = fields.take("visible") {
        node.visible = boolean(visible, "visible").map_err(at)?;
    }
    if let Some(tags) = fields.take("tags") {
        node.tags = list(tags, "tags")
            .and_then(|tags| tags.into_iter().map(|tag| string(tag, "tags")).collect())
            .map_err(at)?;
    }
    node.props = read_props(fields.take("props"), 0).map_err(at)?;
    node.children = read_children(lua, fields.take("children"), &path, sheets, depth + 1)?;
    node.kind = read_kind(lua, &kind_name, &mut fields, sheets).map_err(at)?;
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

/// What a node of the kind `kind` holds, from the fields it has left.
fn read_kind(
    lua: &mlua::Lua,
    kind: &str,
    fields: &mut Fields,
    sheets: &BTreeMap<String, SheetDef>,
) -> Result<KindDef, String> {
    let kind = match kind {
        "node" => KindDef::Node,
        "sprite" => {
            let sheet = fields.take("sheet").ok_or("a sprite node has a `sheet`")?;
            let sheet = string(sheet, "sheet")?;
            if !sheets.contains_key(&sheet) {
                return Err(format!(
                    "there's no sheet `{sheet}` in the scene's `sheets`"
                ));
            }
            let cell = fields.take("cell").ok_or("a sprite node has a `cell`")?;
            let cell = list(cell, "cell")?;
            let cell = match <[mlua::Value; 2]>::try_from(cell) {
                Ok([column, row]) => [whole(column, "cell")?, whole(row, "cell")?],
                Err(_) => return Err("`cell` must be a column and a row, { column, row }".into()),
            };
            let size = match (fields.take("width"), fields.take("height")) {
                (Some(width), Some(height)) => {
                    Some([finite(width, "width")?, finite(height, "height")?])
                }
                (None, None) => None,
                _ => return Err("a sprite with a `width` has a `height`, and the other way".into()),
            };
            KindDef::Sprite(SpriteDef {
                sheet,
                cell,
                size,
                pivot: read_pivot(fields)?,
                tint: optional_color(lua, fields, "tint")?,
            })
        }
        "rect" => KindDef::Rect(RectDef {
            size: read_size(fields, "rect")?,
            pivot: read_pivot(fields)?,
            color: optional_color(lua, fields, "color")?,
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
            KindDef::Text(TextDef {
                text: string(text, "text")?,
                font_scale,
                wrap,
                pivot: read_pivot(fields)?,
                color: optional_color(lua, fields, "color")?,
            })
        }
        "area" => KindDef::Area(AreaDef {
            size: read_size(fields, "area")?,
            pivot: read_pivot(fields)?,
        }),
        other => {
            return Err(format!(
                "there's no kind of node `{other}`: a node is a node, sprite, rect, text or area"
            ));
        }
    };
    Ok(kind)
}

/// A rect's or an area's `width` and `height`, which they must have.
fn read_size(fields: &mut Fields, kind: &str) -> Result<[f32; 2], String> {
    let mut take = |field: &str| match fields.take(field) {
        Some(value) => finite(value, field),
        None => Err(format!("a {kind} node has a `{field}`")),
    };
    Ok([take("width")?, take("height")?])
}

fn read_pivot(fields: &mut Fields) -> Result<[f32; 2], String> {
    match fields.take("pivot") {
        Some(pivot) => pair(pivot, "pivot"),
        None => Ok([0.0, 0.0]),
    }
}

/// The color in `field`, or white without one.
fn optional_color(lua: &mlua::Lua, fields: &mut Fields, field: &str) -> Result<Color, String> {
    let Some(value) = fields.take(field) else {
        return Ok(Color::WHITE);
    };
    let color: Color = lua
        .unpack(value)
        .map_err(|err| format!("`{field}`: {err}"))?;
    // Colors are kept as bytes, as they're written, so writing one and reading it back gives
    // the same color.
    let [r, g, b, a] = color.to_rgba8();
    Ok(Color::rgba8(r, g, b, a))
}

fn read_props(props: Option<mlua::Value>, depth: usize) -> Result<Props, String> {
    let Some(props) = props else {
        return Ok(Props::new());
    };
    let fields = Fields::new(props, "`props`")?;
    fields
        .fields
        .into_iter()
        .map(|(name, value)| {
            let value = read_value(value, &format!("props.{name}"), depth + 1)?;
            Ok((name, value))
        })
        .collect()
}

/// A value in props, at `at`, such as `props.route[2]`, for errors.
fn read_value(value: mlua::Value, at: &str, depth: usize) -> Result<Value, String> {
    if depth >= MAX_DEPTH {
        return Err(format!(
            "`{at}`: props nest more than {MAX_DEPTH} deep, too deep to read"
        ));
    }
    match value {
        mlua::Value::Boolean(value) => Ok(Value::Bool(value)),
        mlua::Value::Integer(_) | mlua::Value::Number(_) => {
            let number = number(value, at)?;
            if !number.is_finite() {
                return Err(format!("`{at}` must be a finite number, not {number}"));
            }
            Ok(Value::Number(number))
        }
        mlua::Value::String(_) => Ok(Value::String(string(value, at)?)),
        mlua::Value::Table(table) => {
            let mut strings = BTreeMap::new();
            let mut numbers = BTreeMap::new();
            for pair in table.pairs::<mlua::Value, mlua::Value>() {
                let (key, value) = pair.map_err(|err| err.to_string())?;
                match key {
                    mlua::Value::String(key) => {
                        let key = key.to_str().map_err(|err| err.to_string())?.to_owned();
                        let value = read_value(value, &format!("{at}.{key}"), depth + 1)?;
                        strings.insert(key, value);
                    }
                    mlua::Value::Integer(_) | mlua::Value::Number(_) => {
                        let index = number(key, at)?;
                        let value = read_value(value, &format!("{at}[{index}]"), depth + 1)?;
                        numbers.insert(index.to_bits(), (index, value));
                    }
                    key => {
                        let key = describe(&key);
                        return Err(format!("`{at}` has a key that's {key}"));
                    }
                }
            }
            match (strings.is_empty(), numbers.is_empty()) {
                (_, true) => {
                    if strings.is_empty() {
                        Ok(Value::List(Vec::new()))
                    } else {
                        Ok(Value::Map(strings))
                    }
                }
                (true, false) => {
                    let mut items: Vec<(f64, Value)> = numbers.into_values().collect();
                    items.sort_by(|a, b| a.0.total_cmp(&b.0));
                    let is_list = items
                        .iter()
                        .enumerate()
                        .all(|(i, (index, _))| *index == (i + 1) as f64);
                    if !is_list {
                        return Err(format!(
                            "`{at}` has numbers for keys, but not 1, 2, 3 and so on: a table in \
                             props is a list or has strings for keys"
                        ));
                    }
                    Ok(Value::List(
                        items.into_iter().map(|(_, value)| value).collect(),
                    ))
                }
                (false, false) => Err(format!(
                    "`{at}` has both strings and numbers for keys: a table in props is a list or \
                     has strings for keys"
                )),
            }
        }
        value => Err(format!(
            "`{at}` is {}, but props hold only booleans, numbers, strings and tables",
            describe(&value)
        )),
    }
}

/// A table's fields by name, taken one by one as they're read, so whatever's left over is a
/// field the table shouldn't have.
struct Fields {
    fields: BTreeMap<String, mlua::Value>,
}

impl Fields {
    /// The fields of `value`, which must be a table with strings for keys, such as `"a node"`.
    fn new(value: mlua::Value, what: &str) -> Result<Self, String> {
        let mlua::Value::Table(table) = value else {
            return Err(format!("{what} must be a table, not {}", describe(&value)));
        };
        let mut fields = BTreeMap::new();
        for pair in table.pairs::<mlua::Value, mlua::Value>() {
            let (key, value) = pair.map_err(|err| err.to_string())?;
            let mlua::Value::String(key) = &key else {
                return Err(format!(
                    "{what} has fields named by strings, not {}",
                    describe(&key)
                ));
            };
            let key = key.to_str().map_err(|err| err.to_string())?.to_owned();
            fields.insert(key, value);
        }
        Ok(Self { fields })
    }

    fn take(&mut self, name: &str) -> Option<mlua::Value> {
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

/// A table's values in order, if it's a list.
fn list(value: mlua::Value, field: &str) -> Result<Vec<mlua::Value>, String> {
    let mlua::Value::Table(table) = &value else {
        return Err(format!(
            "`{field}` must be a list, not {}",
            describe(&value)
        ));
    };
    let items: Vec<mlua::Value> = table
        .clone()
        .sequence_values()
        .collect::<mlua::Result<_>>()
        .map_err(|err| err.to_string())?;
    let count = table.pairs::<mlua::Value, mlua::Value>().count();
    if count != items.len() {
        return Err(format!(
            "`{field}` must be a list, with nothing in it but values counting from 1"
        ));
    }
    Ok(items)
}

fn number(value: mlua::Value, field: &str) -> Result<f64, String> {
    match value {
        mlua::Value::Integer(number) => Ok(number as f64),
        mlua::Value::Number(number) => Ok(number),
        value => Err(format!(
            "`{field}` must be a number, not {}",
            describe(&value)
        )),
    }
}

/// A finite number, as `f32`, for positions and sizes.
fn finite(value: mlua::Value, field: &str) -> Result<f32, String> {
    let number = number(value, field)?;
    let single = number as f32;
    if !single.is_finite() {
        return Err(format!("`{field}` must be a finite number, not {number}"));
    }
    Ok(single)
}

/// A whole number from 0 up.
fn whole(value: mlua::Value, field: &str) -> Result<u32, String> {
    let number = number(value, field)?;
    if number.fract() != 0.0 || !(0.0..=f64::from(u32::MAX)).contains(&number) {
        return Err(format!(
            "`{field}` must be a whole number from 0 up, not {number}"
        ));
    }
    Ok(number as u32)
}

fn positive_whole(value: mlua::Value, field: &str) -> Result<u32, String> {
    match whole(value, field)? {
        0 => Err(format!("`{field}` must be at least 1")),
        number => Ok(number),
    }
}

/// A pair of finite numbers, `{ x, y }`.
fn pair(value: mlua::Value, field: &str) -> Result<[f32; 2], String> {
    match <[mlua::Value; 2]>::try_from(list(value, field)?) {
        Ok([x, y]) => Ok([finite(x, field)?, finite(y, field)?]),
        Err(_) => Err(format!("`{field}` must be two numbers, {{ x, y }}")),
    }
}

fn string(value: mlua::Value, field: &str) -> Result<String, String> {
    match &value {
        mlua::Value::String(string) => match string.to_str() {
            Ok(string) => Ok(string.to_owned()),
            Err(_) => Err(format!("`{field}` must be text in UTF-8")),
        },
        _ => Err(format!(
            "`{field}` must be a string, not {}",
            describe(&value)
        )),
    }
}

fn boolean(value: mlua::Value, field: &str) -> Result<bool, String> {
    match value {
        mlua::Value::Boolean(value) => Ok(value),
        value => Err(format!(
            "`{field}` must be true or false, not {}",
            describe(&value)
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
                    { name = "pond", kind = "rect", width = 48, height = 32, color = "#21406a80" },
                    { name = "title", kind = "text", text = "Hi", font_scale = 2, wrap = 100 },
                    { name = "gate", kind = "area", width = 16, height = 8, props = { to = "town" } },
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
        assert_eq!(
            mill.kind,
            KindDef::Sprite(SpriteDef {
                sheet: "tiles".into(),
                cell: [3, 20],
                size: None,
                pivot: [8.0, 16.0],
                tint: Color::rgb8(0xd8, 0xb0, 0x70),
            })
        );
        let sails = &mill.children[0];
        assert_eq!(sails.kind, KindDef::Node);
        assert!(!sails.visible);

        assert_eq!(
            scene.children[1].kind,
            KindDef::Rect(RectDef {
                size: [48.0, 32.0],
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
                pivot: [0.0, 0.0],
                color: Color::WHITE,
            })
        );
        let gate = &scene.children[3];
        assert_eq!(
            gate.kind,
            KindDef::Area(AreaDef {
                size: [16.0, 8.0],
                pivot: [0.0, 0.0],
            })
        );
        assert_eq!(gate.props["to"], Value::String("town".into()));
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
                "`x` must be a finite number",
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
                "`props.route` has numbers for keys, but not 1, 2, 3",
            ),
            (
                "{ name = 'a', kind = 'node', props = { mixed = { 1, x = 2 } } }",
                "`props.mixed` has both strings and numbers for keys",
            ),
            (
                "{ name = 'a', kind = 'rect', width = 4, height = 4, color = 'red' }",
                "`color`:",
            ),
        ];
        for (children, expected) in cases {
            let err = error(&with_children(children));
            assert!(err.contains(expected), "{children}\n{err}");
        }
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
}
