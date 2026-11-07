//! Writing scene and tileset files, in one canonical layout.
//!
//! The layout is fixed, so writing the same scene always gives the same text, and changing one
//! thing changes few lines: every field of a node, sheet or tile on a line of its own, in a fixed
//! order, and fields at their defaults left out. Lists of plain values, such as a cell or tags,
//! stay on one line, but a tile layer's rows go one to a line, so they line up as the map does.
//! Indents are four spaces, and every field ends with a comma.

use std::{collections::BTreeMap, fmt::Write};

use super::{
    clip::{Clip, Interpolation, Keys, Track},
    file::{
        FORMAT, KindDef, NodeDef, PanelLookDef, Placement3, SceneFile, Shape, SheetDef, Stack,
        StackDirection, TileDef, TilesetFile, Value,
    },
};
use crate::{
    graphics::{Align, Camera3d, Color, Lighting, Projection},
    lua::{DEFAULT_FOV, is_identifier},
};

/// `scene` as the text of a scene file, or why it can't be written: only finite numbers can.
pub(super) fn write(scene: &SceneFile) -> Result<String, String> {
    let mut writer = Writer::default();
    writer.out.push_str("return ");
    writer.open();
    writer.field("format", &FORMAT.to_string());
    writer.sheets(&scene.sheets);
    if !scene.props.is_empty() {
        writer.key("props");
        writer
            .map(&scene.props)
            .map_err(|err| format!("the scene's props: {err}"))?;
        writer.end_field();
    }
    writer.children(&scene.children, "")?;
    if !scene.clips.is_empty() {
        writer.key("clips");
        writer.open();
        for (name, clip) in &scene.clips {
            writer.key(name);
            writer
                .clip(clip)
                .map_err(|err| format!("clip `{name}`: {err}"))?;
            writer.end_field();
        }
        writer.close();
        writer.end_field();
    }
    writer.close();
    writer.out.push('\n');
    Ok(writer.out)
}

/// `tileset` as the text of a tileset file, or why it can't be written.
pub(super) fn write_tileset(tileset: &TilesetFile) -> Result<String, String> {
    let mut writer = Writer::default();
    writer.out.push_str("return ");
    writer.open();
    writer.field("format", &FORMAT.to_string());
    writer.field("tile_width", &tileset.tile_width.to_string());
    writer.field("tile_height", &tileset.tile_height.to_string());
    writer.sheets(&tileset.sheets);
    if !tileset.tiles.is_empty() {
        writer.key("tiles");
        writer.open();
        for (name, tile) in &tileset.tiles {
            writer.key(name);
            writer
                .tile(tile)
                .map_err(|err| format!("tile `{name}`: {err}"))?;
            writer.end_field();
        }
        writer.close();
        writer.end_field();
    }
    writer.close();
    writer.out.push('\n');
    Ok(writer.out)
}

/// `value` as the text of a file that returns it, as a save file holds data, or why it can't be
/// written: only finite numbers can.
pub(crate) fn write_value(value: &Value) -> Result<String, String> {
    let mut writer = Writer::default();
    writer.out.push_str("return ");
    writer.value(value)?;
    writer.out.push('\n');
    Ok(writer.out)
}

#[derive(Default)]
struct Writer {
    out: String,
    depth: usize,
}

impl Writer {
    /// Starts a table whose entries go on lines of their own.
    fn open(&mut self) {
        self.out.push_str("{\n");
        self.depth += 1;
    }

    fn close(&mut self) {
        self.depth -= 1;
        self.indent();
        self.out.push('}');
    }

    fn indent(&mut self) {
        for _ in 0..self.depth {
            self.out.push_str("    ");
        }
    }

    /// Starts a field: its key, then its value goes next.
    fn key(&mut self, key: &str) {
        self.indent();
        if is_identifier(key) {
            self.out.push_str(key);
        } else {
            self.out.push('[');
            self.out.push_str(&quoted(key));
            self.out.push(']');
        }
        self.out.push_str(" = ");
    }

    fn end_field(&mut self) {
        self.out.push_str(",\n");
    }

    /// A field whose value is `value`, written already.
    fn field(&mut self, key: &str, value: &str) {
        self.key(key);
        self.out.push_str(value);
        self.end_field();
    }

    fn number_field(&mut self, key: &str, value: f32) -> Result<(), String> {
        let value = single(value, key)?;
        self.field(key, &value);
        Ok(())
    }

    fn pair_field(&mut self, key: &str, [x, y]: [f32; 2]) -> Result<(), String> {
        let value = format!("{{ {}, {} }}", single(x, key)?, single(y, key)?);
        self.field(key, &value);
        Ok(())
    }

    fn triple_field(&mut self, key: &str, [x, y, z]: [f32; 3]) -> Result<(), String> {
        let value = format!(
            "{{ {}, {}, {} }}",
            single(x, key)?,
            single(y, key)?,
            single(z, key)?
        );
        self.field(key, &value);
        Ok(())
    }

    /// A view's `camera`, leaving out what's at its defaults.
    fn camera(&mut self, camera: &Camera3d) -> Result<(), String> {
        let at = |err: String| format!("camera: {err}");
        self.key("camera");
        self.open();
        self.triple_field("position", camera.position).map_err(at)?;
        self.triple_field("target", camera.target).map_err(at)?;
        if camera.up != [0.0, 1.0, 0.0] {
            self.triple_field("up", camera.up).map_err(at)?;
        }
        match camera.projection {
            Projection::Perspective { fov_y } if fov_y != DEFAULT_FOV => {
                self.number_field("fov", fov_y).map_err(at)?;
            }
            Projection::Perspective { .. } => {}
            Projection::Orthographic { height } => {
                self.number_field("height", height).map_err(at)?
            }
        }
        let default = Camera3d::perspective([0.0; 3], [0.0; 3], DEFAULT_FOV);
        if camera.near != default.near {
            self.number_field("near", camera.near).map_err(at)?;
        }
        if camera.far != default.far {
            self.number_field("far", camera.far).map_err(at)?;
        }
        self.close();
        self.end_field();
        Ok(())
    }

    /// A view's `lighting`, unless it's the default, leaving out what's at its defaults.
    fn lighting(&mut self, lighting: &Lighting) -> Result<(), String> {
        let default = Lighting::default();
        let same_color = |a: Color, b: Color| a.to_rgba8() == b.to_rgba8();
        let direction = lighting.direction != default.direction;
        let color = !same_color(lighting.color, default.color);
        let ambient = !same_color(lighting.ambient, default.ambient);
        if !(direction || color || ambient) {
            return Ok(());
        }
        self.key("lighting");
        self.open();
        if direction {
            self.triple_field("direction", lighting.direction)
                .map_err(|err| format!("lighting: {err}"))?;
        }
        if color {
            self.any_color("color", lighting.color);
        }
        if ambient {
            self.any_color("ambient", lighting.ambient);
        }
        self.close();
        self.end_field();
        Ok(())
    }

    /// A placed node's 3D fields, leaving out those at their defaults.
    fn placement3(&mut self, placement: &Placement3) -> Result<(), String> {
        let default = Placement3::default();
        let numbers = [
            ("yaw", placement.yaw, default.yaw),
            ("pitch", placement.pitch, default.pitch),
            ("roll", placement.roll, default.roll),
        ];
        for (key, value, default) in numbers {
            if value != default {
                self.number_field(key, value)?;
            }
        }
        Ok(())
    }

    /// The `sheets` field, if there are any.
    fn sheets(&mut self, sheets: &BTreeMap<String, SheetDef>) {
        if sheets.is_empty() {
            return;
        }
        self.key("sheets");
        self.open();
        for (name, sheet) in sheets {
            self.key(name);
            self.open();
            self.field("path", &quoted(&sheet.path));
            self.field("tile_width", &sheet.tile_width.to_string());
            self.field("tile_height", &sheet.tile_height.to_string());
            self.close();
            self.end_field();
        }
        self.close();
        self.end_field();
    }

    fn tile(&mut self, tile: &TileDef) -> Result<(), String> {
        self.open();
        self.field("sheet", &quoted(&tile.sheet));
        let [column, row] = tile.cell;
        self.field("cell", &format!("{{ {column}, {row} }}"));
        self.color("tint", tile.tint);
        if tile.solid {
            self.field("solid", "true");
        }
        if !tile.props.is_empty() {
            self.key("props");
            self.map(&tile.props)
                .map_err(|err| format!("props: {err}"))?;
            self.end_field();
        }
        self.close();
        Ok(())
    }

    /// The `children` field of the node at `parent`, or of the scene for `""`, if it has any.
    fn children(&mut self, children: &[NodeDef], parent: &str) -> Result<(), String> {
        if children.is_empty() {
            return Ok(());
        }
        self.key("children");
        self.open();
        for child in children {
            let path = if parent.is_empty() {
                child.name.clone()
            } else {
                format!("{parent}/{}", child.name)
            };
            self.indent();
            self.node(child, &path)?;
            self.end_field();
        }
        self.close();
        self.end_field();
        Ok(())
    }

    fn node(&mut self, node: &NodeDef, path: &str) -> Result<(), String> {
        let at = |err: String| format!("node `{path}`: {err}");
        self.open();
        self.field("name", &quoted(&node.name));
        self.field("kind", &quoted(node.kind.name()));
        let z = node.placement3.map(|placement| placement.z);
        for (key, value) in [("x", Some(node.x)), ("y", Some(node.y)), ("z", z)] {
            if let Some(value) = value
                && value != 0.0
            {
                self.number_field(key, value).map_err(at)?;
            }
        }
        if let Some(anchor) = node.anchor {
            self.pair_field("anchor", anchor).map_err(at)?;
        }
        if node.rotation != 0.0 {
            self.number_field("rotation", node.rotation).map_err(at)?;
        }
        if let Some(placement) = &node.placement3 {
            self.placement3(placement).map_err(at)?;
        }
        let scale_z = node.placement3.map(|placement| placement.scale_z);
        let scales = [
            ("scale_x", Some(node.scale_x)),
            ("scale_y", Some(node.scale_y)),
            ("scale_z", scale_z),
        ];
        for (key, value) in scales {
            if let Some(value) = value
                && value != 1.0
            {
                self.number_field(key, value).map_err(at)?;
            }
        }
        if !node.visible {
            self.field("visible", "false");
        }
        self.kind(&node.kind).map_err(at)?;
        if !node.tags.is_empty() {
            let tags: Vec<String> = node.tags.iter().map(|tag| quoted(tag)).collect();
            self.field("tags", &format!("{{ {} }}", tags.join(", ")));
        }
        if !node.props.is_empty() {
            self.key("props");
            self.map(&node.props)
                .map_err(|err| at(format!("props: {err}")))?;
            self.end_field();
        }
        self.children(&node.children, path)?;
        self.close();
        Ok(())
    }

    /// The fields of what a node holds, in a fixed order: what it shows, its size and how it's
    /// laid out, its pivot, then its color.
    fn kind(&mut self, kind: &KindDef) -> Result<(), String> {
        match kind {
            KindDef::Node => {}
            KindDef::Sprite(sprite) => {
                self.field("sheet", &quoted(&sprite.sheet));
                let [column, row] = sprite.cell;
                self.field("cell", &format!("{{ {column}, {row} }}"));
                if let Some([width, height]) = sprite.size {
                    self.number_field("width", width)?;
                    self.number_field("height", height)?;
                }
                self.stretch(sprite.stretch)?;
                self.pivot(sprite.pivot)?;
                self.color("tint", sprite.tint);
            }
            KindDef::Rect(rect) => {
                self.number_field("width", rect.size[0])?;
                self.number_field("height", rect.size[1])?;
                self.stretch(rect.stretch)?;
                self.pivot(rect.pivot)?;
                self.color("color", rect.color);
            }
            KindDef::Text(text) => {
                self.field("text", &quoted(&text.text));
                if text.font_scale != 1.0 {
                    self.number_field("font_scale", text.font_scale)?;
                }
                if let Some(wrap) = text.wrap {
                    self.number_field("wrap", wrap)?;
                }
                if text.align != Align::Left {
                    self.field("align", &quoted(text.align.name()));
                }
                self.pivot(text.pivot)?;
                self.color("color", text.color);
            }
            KindDef::Area(area) => {
                self.number_field("width", area.size[0])?;
                self.number_field("height", area.size[1])?;
                self.stretch(area.stretch)?;
                self.pivot(area.pivot)?;
            }
            KindDef::Tilemap(map) => {
                self.field("tileset", &quoted(&map.tileset));
                self.field("columns", &map.columns.to_string());
                self.field("rows", &map.rows.to_string());
            }
            KindDef::TileLayer(layer) => {
                if !layer.legend.is_empty() {
                    self.key("legend");
                    self.open();
                    for (symbol, tile) in &layer.legend {
                        self.field(&symbol.to_string(), &quoted(tile));
                    }
                    self.close();
                    self.end_field();
                }
                self.key("tiles");
                if layer.tiles.is_empty() {
                    self.out.push_str("{}");
                } else {
                    self.open();
                    for row in &layer.tiles {
                        self.indent();
                        self.out.push_str(&quoted(row));
                        self.end_field();
                    }
                    self.close();
                }
                self.end_field();
            }
            KindDef::Instance(instance) => {
                self.field("scene", &quoted(&instance.scene));
                if !instance.overrides.is_empty() {
                    self.key("overrides");
                    self.open();
                    for (path, change) in &instance.overrides {
                        self.key(path);
                        self.map(change)
                            .map_err(|err| format!("overrides of `{path}`: {err}"))?;
                        self.end_field();
                    }
                    self.close();
                    self.end_field();
                }
            }
            KindDef::View3d(view) => {
                self.number_field("width", view.size[0])?;
                self.number_field("height", view.size[1])?;
                self.stretch(view.stretch)?;
                self.pivot(view.pivot)?;
                self.camera(&view.camera)?;
                self.lighting(&view.lighting)?;
            }
            KindDef::Mesh(mesh) => {
                self.field("shape", &quoted(mesh.shape.name()));
                match mesh.shape {
                    Shape::Cuboid(size) if size != [1.0; 3] => self.triple_field("size", size)?,
                    Shape::Plane(size) if size != [1.0; 2] => self.pair_field("size", size)?,
                    Shape::Sphere(radius) if radius != 0.5 => {
                        self.number_field("radius", radius)?;
                    }
                    _ => {}
                }
                if let Some(sprite) = &mesh.sprite {
                    self.field("sheet", &quoted(&sprite.sheet));
                    let [column, row] = sprite.cell;
                    self.field("cell", &format!("{{ {column}, {row} }}"));
                }
                self.color("tint", mesh.tint);
            }
            KindDef::Model(model) => {
                self.field("model", &quoted(&model.model));
                self.color("tint", model.tint);
            }
            KindDef::Billboard(billboard) => {
                self.field("sheet", &quoted(&billboard.sheet));
                let [column, row] = billboard.cell;
                self.field("cell", &format!("{{ {column}, {row} }}"));
                let [width, height] = billboard.size;
                if width != 1.0 {
                    self.number_field("width", width)?;
                }
                if height != 1.0 {
                    self.number_field("height", height)?;
                }
                self.color("tint", billboard.tint);
            }
            KindDef::Stack(stack) => self.stack(stack)?,
            KindDef::Panel(panel) => match &panel.look {
                PanelLookDef::Fill(fill) => {
                    self.stack(&panel.stack)?;
                    self.color("color", fill.color);
                    if fill.border != 0.0 {
                        self.number_field("border", fill.border)?;
                    }
                    self.color("border_color", fill.border_color);
                }
                PanelLookDef::Sprite {
                    sprite,
                    slice,
                    tint,
                } => {
                    self.field("sheet", &quoted(&sprite.sheet));
                    let [column, row] = sprite.cell;
                    self.field("cell", &format!("{{ {column}, {row} }}"));
                    self.edges("slice", *slice)?;
                    self.stack(&panel.stack)?;
                    self.color("tint", *tint);
                }
            },
        }
        Ok(())
    }

    /// A stack's or panel's size and layout, leaving out what's at its defaults.
    fn stack(&mut self, stack: &Stack) -> Result<(), String> {
        for (i, key) in ["width", "height"].into_iter().enumerate() {
            if let Some(length) = stack.size[i] {
                self.number_field(key, length)?;
            }
        }
        self.stretch(stack.stretch)?;
        if stack.direction != StackDirection::Column {
            self.field("direction", &quoted(stack.direction.name()));
        }
        if stack.gap != 0.0 {
            self.number_field("gap", stack.gap)?;
        }
        self.edges("padding", stack.padding)?;
        self.pivot(stack.pivot)
    }

    /// Numbers for the left, top, right and bottom edges, unless they're all 0: one, if they're
    /// all the same.
    fn edges(&mut self, key: &str, edges: [f32; 4]) -> Result<(), String> {
        let [left, top, right, bottom] = edges;
        if edges == [0.0; 4] {
            return Ok(());
        }
        if edges.iter().all(|&edge| edge == left) {
            return self.number_field(key, left);
        }
        let value = format!(
            "{{ {}, {}, {}, {} }}",
            single(left, key)?,
            single(top, key)?,
            single(right, key)?,
            single(bottom, key)?
        );
        self.field(key, &value);
        Ok(())
    }

    fn clip(&mut self, clip: &Clip) -> Result<(), String> {
        self.open();
        self.number_field("duration", clip.duration)?;
        if clip.looping {
            self.field("loop", "true");
        }
        if !clip.tracks.is_empty() {
            self.key("tracks");
            self.open();
            for (i, track) in clip.tracks.iter().enumerate() {
                self.indent();
                self.track(track)
                    .map_err(|err| format!("track {}: {err}", i + 1))?;
                self.end_field();
            }
            self.close();
            self.end_field();
        }
        self.close();
        Ok(())
    }

    /// A track, with a key to a line.
    fn track(&mut self, track: &Track) -> Result<(), String> {
        self.open();
        if !track.node.is_empty() {
            self.field("node", &quoted(&track.node));
        }
        self.field("property", &quoted(track.keys.property_name()));
        if let Keys::Number {
            interpolation: Interpolation::Step,
            ..
        } = track.keys
        {
            self.field("interpolation", &quoted(Interpolation::Step.name()));
        }
        let keys: Vec<(f32, String)> = match &track.keys {
            Keys::Number { keys, .. } => keys
                .iter()
                .map(|&(time, value)| Ok((time, single(value, "keys")?)))
                .collect::<Result<_, String>>()?,
            Keys::Visible(keys) => keys
                .iter()
                .map(|&(time, visible)| (time, visible.to_string()))
                .collect(),
            Keys::Cell(keys) => keys
                .iter()
                .map(|&(time, [column, row])| (time, format!("{{ {column}, {row} }}")))
                .collect(),
        };
        self.key("keys");
        if keys.is_empty() {
            self.out.push_str("{}");
        } else {
            self.open();
            for (time, value) in keys {
                self.indent();
                write!(self.out, "{{ {}, {value} }}", single(time, "keys")?).unwrap();
                self.end_field();
            }
            self.close();
        }
        self.end_field();
        self.close();
        Ok(())
    }

    fn pivot(&mut self, pivot: [f32; 2]) -> Result<(), String> {
        if pivot != [0.0, 0.0] {
            self.pair_field("pivot", pivot)?;
        }
        Ok(())
    }

    fn stretch(&mut self, stretch: [f32; 2]) -> Result<(), String> {
        if stretch != [0.0, 0.0] {
            self.pair_field("stretch", stretch)?;
        }
        Ok(())
    }

    /// A color field, as a hex string, unless it's white, the default.
    fn color(&mut self, key: &str, color: Color) {
        if color.to_rgba8() != Color::WHITE.to_rgba8() {
            self.any_color(key, color);
        }
    }

    /// A color field, as a hex string, whatever it is.
    fn any_color(&mut self, key: &str, color: Color) {
        let [r, g, b, a] = color.to_rgba8();
        let mut hex = format!("#{r:02x}{g:02x}{b:02x}");
        if a != 255 {
            write!(hex, "{a:02x}").unwrap();
        }
        self.field(key, &quoted(&hex));
    }

    /// A table with strings for keys, in the order of its keys.
    fn map<'a>(
        &mut self,
        map: impl IntoIterator<Item = (&'a String, &'a Value)>,
    ) -> Result<(), String> {
        let mut entries = map.into_iter().peekable();
        if entries.peek().is_none() {
            self.out.push_str("{}");
            return Ok(());
        }
        self.open();
        for (key, value) in entries {
            self.key(key);
            self.value(value).map_err(|err| format!("`{key}`: {err}"))?;
            self.end_field();
        }
        self.close();
        Ok(())
    }

    fn value(&mut self, value: &Value) -> Result<(), String> {
        match value {
            Value::Bool(value) => self.out.push_str(if *value { "true" } else { "false" }),
            Value::Number(number) => {
                if !number.is_finite() {
                    return Err(format!(
                        "{number} can't be written: files hold only finite numbers"
                    ));
                }
                write!(self.out, "{number}").unwrap();
            }
            Value::String(string) => self.out.push_str(&quoted(string)),
            Value::List(items) if items.is_empty() => self.out.push_str("{}"),
            Value::List(items) => {
                let plain = items
                    .iter()
                    .all(|item| !matches!(item, Value::List(_) | Value::Map(_)));
                if plain {
                    self.out.push_str("{ ");
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            self.out.push_str(", ");
                        }
                        self.value(item)
                            .map_err(|err| format!("item {}: {err}", i + 1))?;
                    }
                    self.out.push_str(" }");
                } else {
                    self.open();
                    for (i, item) in items.iter().enumerate() {
                        self.indent();
                        self.value(item)
                            .map_err(|err| format!("item {}: {err}", i + 1))?;
                        self.end_field();
                    }
                    self.close();
                }
            }
            Value::Map(map) => self.map(map)?,
        }
        Ok(())
    }
}

/// `value` written so Luau reads it back as the same `f32`, if it's finite.
fn single(value: f32, field: &str) -> Result<String, String> {
    if !value.is_finite() {
        return Err(format!(
            "`{field}` is {value}, which can't be written: scene files hold only finite numbers"
        ));
    }
    // The shortest decimal that reads back as `value`. Luau reads it as an `f64`, and rounding
    // that to an `f32` gives `value` again: an `f64` has more than twice the precision.
    Ok(format!("{value}"))
}

/// `text` as a Luau string literal, in double quotes.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_ascii_control() => write!(out, "\\x{:02x}", c as u32).unwrap(),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Property, Props, RectDef, SpriteDef};

    /// A scene in its canonical layout.
    const CANONICAL: &str = r##"return {
    format = 3,
    sheets = {
        tiles = {
            path = "tiles.png",
            tile_width = 16,
            tile_height = 16,
        },
    },
    props = {
        music = "garden.ogg",
    },
    children = {
        {
            name = "mill",
            kind = "sprite",
            x = 120,
            y = 80.5,
            rotation = -0.1,
            sheet = "tiles",
            cell = { 3, 20 },
            pivot = { 8, 16 },
            tint = "#d8b070",
            tags = { "building", "solid" },
            children = {
                {
                    name = "sails",
                    kind = "node",
                    y = -12,
                    visible = false,
                },
            },
        },
        {
            name = "sign",
            kind = "area",
            width = 16,
            height = 8,
            props = {
                route = { "left", 2, true },
                text = "“Mind the gap,” it says.\n\tSigned, \"the miller\"",
                where = {
                    ["end"] = 3,
                    ["the mill"] = {
                        { 1, 2 },
                        {},
                    },
                },
            },
        },
    },
}
"##;

    fn parse(source: &str) -> SceneFile {
        SceneFile::parse(source, "test.scene.luau").unwrap()
    }

    #[test]
    fn writing_a_canonical_file_gives_it_back() {
        let scene = parse(CANONICAL);
        assert_eq!(scene.to_luau().unwrap(), CANONICAL);
    }

    #[test]
    fn reading_what_is_written_gives_the_same_scene() {
        let scene = parse(
            "return { format = 1, children = {
                { name = 'a', kind = 'rect', width = 0.1, height = 1e-7, x = 123456.789,
                  rotation = 3.14159, color = { r = 0.5, g = 0.25, b = 1, a = 0.3 } },
                { name = 'b', kind = 'text', text = '\\0\\1 bell\\a', wrap = 64, font_scale = 3 },
            } }",
        );
        let written = scene.to_luau().unwrap();
        let again = parse(&written);
        assert_eq!(again, scene);
        assert_eq!(again.to_luau().unwrap(), written, "the same bytes");
    }

    #[test]
    fn scenes_built_in_rust_write_and_read_back() {
        let mut mill = NodeDef::new(
            "mill",
            KindDef::Sprite(SpriteDef {
                sheet: "tiles".into(),
                cell: [1, 2],
                size: Some([32.0, 32.0]),
                stretch: [0.0, 0.5],
                pivot: [0.5, 0.25],
                tint: Color::rgba8(1, 2, 3, 4),
            }),
        );
        mill.scale_x = 2.0;
        mill.tags = vec!["while".into()];
        mill.children.push(NodeDef::new(
            "floor",
            KindDef::Rect(RectDef {
                size: [8.0, 8.0],
                stretch: [0.0, 0.0],
                pivot: [0.0, 0.0],
                color: Color::BLACK,
            }),
        ));
        mill.children[0].anchor = Some([0.0, 0.0]);
        let mut props = Props::new();
        props.insert("for".into(), Value::Number(-0.5));
        let turn = Clip {
            duration: 0.25,
            looping: false,
            tracks: vec![
                Track {
                    node: "mill".into(),
                    keys: Keys::Number {
                        property: Property::ScaleY,
                        interpolation: Interpolation::Step,
                        keys: vec![(0.0, 1.0), (0.1, 0.3333)],
                    },
                },
                Track {
                    node: "mill/floor".into(),
                    keys: Keys::Visible(vec![(0.25, false)]),
                },
            ],
        };
        let scene = SceneFile {
            sheets: [(
                "tiles".to_owned(),
                SheetDef {
                    path: "tiles.png".into(),
                    tile_width: 8,
                    tile_height: 8,
                },
            )]
            .into(),
            props,
            children: vec![mill],
            clips: [("turn".to_owned(), turn)].into(),
        };
        let written = scene.to_luau().unwrap();
        assert!(written.contains("[\"for\"] = -0.5,"), "{written}");
        assert!(written.contains("{ 0.1, 0.3333 },"), "{written}");
        assert_eq!(parse(&written), scene);
    }

    #[test]
    fn numbers_that_are_not_finite_cannot_be_written() {
        let mut node = NodeDef::new("spinning", KindDef::Node);
        node.rotation = f32::INFINITY;
        let scene = SceneFile {
            children: vec![node],
            ..SceneFile::default()
        };
        let err = scene.to_luau().unwrap_err().to_string();
        assert!(err.contains("node `spinning`: `rotation` is inf"), "{err}");
    }

    #[test]
    fn keys_are_bare_when_they_can_be() {
        assert!(is_identifier("tile_width"));
        assert!(is_identifier("_x1"));
        assert!(!is_identifier("1x"));
        assert!(!is_identifier("the mill"));
        assert!(!is_identifier("end"));
        assert!(!is_identifier(""));
    }

    /// A level: a tilemap with two layers and people between them, and an instance.
    const LEVEL: &str = r##"return {
    format = 3,
    children = {
        {
            name = "map",
            kind = "tilemap",
            x = 16,
            tileset = "world.tileset.luau",
            columns = 4,
            rows = 2,
            children = {
                {
                    name = "ground",
                    kind = "tile_layer",
                    legend = {
                        ["."] = "grass",
                        T = "tree",
                        ["~"] = "water",
                    },
                    tiles = {
                        "..T~",
                        "T. ~",
                    },
                },
                {
                    name = "people",
                    kind = "node",
                },
                {
                    name = "roofs",
                    kind = "tile_layer",
                    tiles = {
                        "    ",
                        "    ",
                    },
                },
            },
        },
        {
            name = "elder",
            kind = "instance",
            x = 32,
            scene = "villager.scene.luau",
            overrides = {
                ["body/hat"] = {
                    props = {
                        brim = 2,
                    },
                    tint = "#c8b8e8",
                },
            },
            tags = { "talks" },
        },
    },
}
"##;

    #[test]
    fn levels_write_and_read_back() {
        let scene = parse(LEVEL);
        assert_eq!(scene.to_luau().unwrap(), LEVEL);
    }

    /// A screen: a bar along the bottom of the canvas, with a label at its right end, and a
    /// panel in the middle.
    const SCREEN: &str = r##"return {
    format = 3,
    children = {
        {
            name = "bar",
            kind = "rect",
            anchor = { 0, 1 },
            width = -8,
            height = 12,
            stretch = { 1, 0 },
            pivot = { -4, 16 },
            color = "#14182c",
            children = {
                {
                    name = "score",
                    kind = "text",
                    x = -4,
                    y = 2,
                    anchor = { 1, 0 },
                    text = "0",
                    align = "right",
                },
            },
        },
        {
            name = "panel",
            kind = "area",
            anchor = { 0.5, 0.5 },
            width = 120,
            height = 80,
            pivot = { 60, 40 },
        },
    },
}
"##;

    #[test]
    fn screens_write_and_read_back() {
        let scene = parse(SCREEN);
        assert_eq!(scene.to_luau().unwrap(), SCREEN);
    }

    /// A rig: an arm with a hand, and clips that wave it and make it blink.
    const RIG: &str = r##"return {
    format = 3,
    sheets = {
        hands = {
            path = "hands.png",
            tile_width = 8,
            tile_height = 8,
        },
    },
    children = {
        {
            name = "arm",
            kind = "rect",
            width = 2,
            height = 10,
            pivot = { 1, 0 },
            children = {
                {
                    name = "hand",
                    kind = "sprite",
                    y = 10,
                    sheet = "hands",
                    cell = { 0, 0 },
                    pivot = { 4, 0 },
                },
            },
        },
    },
    clips = {
        blink = {
            duration = 3,
            loop = true,
            tracks = {
                {
                    node = "arm/hand",
                    property = "cell",
                    keys = {
                        { 0, { 0, 0 } },
                        { 2.9, { 1, 0 } },
                    },
                },
            },
        },
        wave = {
            duration = 1.2,
            loop = true,
            tracks = {
                {
                    node = "arm",
                    property = "rotation",
                    keys = {
                        { 0, -0.25 },
                        { 0.6, 0.5 },
                        { 1.2, -0.25 },
                    },
                },
                {
                    property = "y",
                    interpolation = "step",
                    keys = {
                        { 0, 0 },
                        { 0.6, -1 },
                    },
                },
                {
                    node = "arm/hand",
                    property = "visible",
                    keys = {
                        { 1, false },
                    },
                },
            },
        },
    },
}
"##;

    #[test]
    fn rigs_write_and_read_back() {
        let scene = parse(RIG);
        assert_eq!(scene.to_luau().unwrap(), RIG);
    }

    /// A menu: a panel fitting a column of items, a sliced frame, and a row stretched across.
    const MENU: &str = r##"return {
    format = 3,
    sheets = {
        ui = {
            path = "ui.png",
            tile_width = 5,
            tile_height = 5,
        },
    },
    children = {
        {
            name = "menu",
            kind = "panel",
            anchor = { 0.5, 0 },
            gap = 2,
            padding = { 6, 5, 6, 5 },
            color = "#14182c",
            border = 1,
            border_color = "#f2d3ab",
            children = {
                {
                    name = "play",
                    kind = "text",
                    text = "Play",
                },
            },
        },
        {
            name = "frame",
            kind = "panel",
            sheet = "ui",
            cell = { 0, 0 },
            slice = 2,
            width = 40,
            padding = 4,
            pivot = { 1, 2 },
            tint = "#c8b8e8",
        },
        {
            name = "bar",
            kind = "stack",
            y = -12,
            anchor = { 0, 1 },
            width = 0,
            height = 12,
            stretch = { 1, 0 },
            direction = "row",
            padding = { 1, 0, 1, 0 },
        },
    },
}
"##;

    #[test]
    fn menus_write_and_read_back() {
        let scene = parse(MENU);
        assert_eq!(scene.to_luau().unwrap(), MENU);
        let KindDef::Panel(menu) = &scene.children[0].kind else {
            panic!("a panel");
        };
        assert_eq!(menu.stack.size, [None, None], "fitting what it holds");
        let bar = scene.children[2].kind.stack().unwrap();
        assert_eq!(
            (bar.size, bar.direction),
            ([Some(0.0), Some(12.0)], StackDirection::Row)
        );
    }

    /// A tileset in its canonical layout.
    const TILESET: &str = r##"return {
    format = 3,
    tile_width = 16,
    tile_height = 16,
    sheets = {
        tiles = {
            path = "tiles.png",
            tile_width = 16,
            tile_height = 16,
        },
    },
    tiles = {
        grass = {
            sheet = "tiles",
            cell = { 5, 0 },
            tint = "#3e6b3f",
        },
        ["tree.1"] = {
            sheet = "tiles",
            cell = { 0, 1 },
            solid = true,
            props = {
                text = "An old oak.",
            },
        },
    },
}
"##;

    #[test]
    fn tilesets_write_and_read_back() {
        let tileset = TilesetFile::parse(TILESET, "world.tileset.luau").unwrap();
        assert_eq!(tileset.to_luau().unwrap(), TILESET);
    }

    /// A 3D level: a view with a camera and lighting holding meshes, a model, a billboard and a
    /// node placed in 3D, and a view from above through an orthographic camera.
    const WORLD: &str = r##"return {
    format = 3,
    sheets = {
        tiles = {
            path = "tiles.png",
            tile_width = 16,
            tile_height = 16,
        },
    },
    children = {
        {
            name = "view",
            kind = "view3d",
            anchor = { 0.5, 0.5 },
            width = 160,
            height = 90,
            pivot = { 80, 45 },
            camera = {
                position = { 0, 5, 4 },
                target = { 0, 0, 0 },
                fov = 0.6,
                far = 50,
            },
            lighting = {
                direction = { 0, -1, 0 },
                ambient = "#202020",
            },
            children = {
                {
                    name = "floor",
                    kind = "mesh",
                    shape = "plane",
                    size = { 8, 6 },
                    tint = "#404040",
                },
                {
                    name = "stack",
                    kind = "node",
                    x = 1,
                    z = -2,
                    yaw = 0.5,
                    scale_z = 2,
                    children = {
                        {
                            name = "crate",
                            kind = "mesh",
                            y = 0.5,
                            shape = "cuboid",
                            sheet = "tiles",
                            cell = { 2, 3 },
                        },
                        {
                            name = "ball",
                            kind = "mesh",
                            y = 1.25,
                            pitch = -0.25,
                            roll = 1,
                            shape = "sphere",
                            radius = 0.25,
                        },
                    },
                },
                {
                    name = "barrel",
                    kind = "model",
                    x = -2,
                    model = "models/barrel.glb",
                    tint = "#c0a080",
                },
                {
                    name = "hero",
                    kind = "billboard",
                    sheet = "tiles",
                    cell = { 27, 0 },
                    width = -1,
                    height = 1.5,
                    tags = { "hero" },
                },
            },
        },
        {
            name = "map",
            kind = "view3d",
            width = 64,
            height = 64,
            camera = {
                position = { 0, 10, 0 },
                target = { 0, 0, 0 },
                up = { 0, 0, -1 },
                height = 12,
                near = 1,
            },
        },
    },
    clips = {
        spin = {
            duration = 2,
            loop = true,
            tracks = {
                {
                    node = "view/stack",
                    property = "yaw",
                    keys = {
                        { 0, 0 },
                        { 2, 6.2831855 },
                    },
                },
            },
        },
    },
}
"##;

    #[test]
    fn worlds_write_and_read_back() {
        let scene = parse(WORLD);
        assert_eq!(scene.to_luau().unwrap(), WORLD);
        let KindDef::View3d(view) = &scene.children[0].kind else {
            panic!("{:?}", scene.children[0].kind);
        };
        assert_eq!(view.lighting.color, Lighting::default().color);
        // Written and read back from Rust, every field the same.
        let written = scene.to_luau().unwrap();
        assert_eq!(parse(&written), scene);
    }
}
