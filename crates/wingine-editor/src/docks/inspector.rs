//! The Inspector: every field of the selected node, or of the scene itself, to change.
//!
//! It edits a copy of what's selected, and puts the copy into the file only if something
//! changed, so the file's history holds changes and nothing else.

use std::{collections::BTreeMap, path::Path};

use wingine::{
    Context,
    graphics::{Align, Color},
    gui::egui,
    scene::{KindDef, NodeDef, Props, SceneFile, SheetDef, Value},
};

use crate::{
    document::Document,
    edit::{self, NodePath},
    project::AssetCache,
    theme,
};

/// What the Inspector needs besides the document: the engine, to show sheets, and the files a
/// field can name.
pub struct Services<'a> {
    pub ctx: &'a mut Context,
    pub cache: &'a mut AssetCache,
    pub folder: &'a Path,
    pub images: &'a [String],
    pub tilesets: &'a [String],
    pub scenes: &'a [String],
}

#[derive(Default)]
pub struct Inspector {
    /// Names being typed for new tags, props and sheets.
    new_tag: String,
    new_key: String,
    new_sheet: String,
    /// Whether the sheet of the selected sprite is shown to pick a cell from.
    picking_cell: bool,
}

impl Inspector {
    pub fn ui(&mut self, ui: &mut egui::Ui, document: &mut Document, services: &mut Services) {
        let Some(path) = document.selected.clone() else {
            ui.weak("Select a node in the Scene dock or the viewport to see its fields.");
            return;
        };
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                if path.is_empty() {
                    self.scene(ui, document, services);
                } else {
                    self.node(ui, document, &path, services);
                }
            });
    }

    fn scene(&mut self, ui: &mut egui::Ui, document: &mut Document, services: &mut Services) {
        let file = document.file();
        let mut sheets = file.sheets.clone();
        let mut props = file.props.clone();
        header(ui, "scene", document.name(), "scene");

        section(ui, "Sheets", |ui| {
            let mut removed = None;
            for (name, sheet) in &mut sheets {
                ui.horizontal(|ui| {
                    ui.strong(name);
                    if ui
                        .small_button("🗑")
                        .on_hover_text("Remove the sheet")
                        .clicked()
                    {
                        removed = Some(name.clone());
                    }
                });
                grid(ui, ("sheet", name.as_str()), |ui| {
                    ui.label("path");
                    file_choice(
                        ui,
                        ("sheet path", name.as_str()),
                        &mut sheet.path,
                        services.images,
                    );
                    ui.end_row();
                    ui.label("tile size");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut sheet.tile_width).range(1..=4096));
                        ui.add(egui::DragValue::new(&mut sheet.tile_height).range(1..=4096));
                    });
                    ui.end_row();
                });
            }
            if let Some(name) = removed {
                sheets.remove(&name);
            }
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_sheet)
                        .hint_text("New sheet's name")
                        .desired_width(120.0),
                );
                let name = self.new_sheet.trim().to_owned();
                let can_add = !name.is_empty() && !sheets.contains_key(&name);
                if ui
                    .add_enabled(can_add, egui::Button::new("Add sheet"))
                    .clicked()
                {
                    let path = services.images.first().cloned().unwrap_or_default();
                    sheets.insert(
                        name,
                        SheetDef {
                            path,
                            tile_width: 16,
                            tile_height: 16,
                        },
                    );
                    self.new_sheet.clear();
                }
            });
        });
        section(ui, "Props", |ui| {
            props_editor(ui, "scene props", &mut props, &mut self.new_key);
        });
        if file.clips.is_empty() {
            ui.weak("No clips.");
        } else {
            section(ui, "Clips", |ui| {
                for (name, clip) in &file.clips {
                    ui.label(format!(
                        "{name}: {} s, {} tracks{}",
                        clip.duration,
                        clip.tracks.len(),
                        if clip.looping { ", looping" } else { "" }
                    ));
                }
            });
        }

        if sheets != file.sheets || props != file.props {
            let file = document.edit();
            file.sheets = sheets;
            file.props = props;
        }
    }

    fn node(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        path: &NodePath,
        services: &mut Services,
    ) {
        let Some(original) = edit::node(document.file(), path).cloned() else {
            return;
        };
        let sheet_names: Vec<String> = document.file().sheets.keys().cloned().collect();
        let mut node = original.clone();
        header(ui, node.kind.name(), &node.name, node.kind.name());

        section(ui, "Node", |ui| {
            grid(ui, "node", |ui| {
                ui.label("visible");
                ui.checkbox(&mut node.visible, "");
                ui.end_row();
                ui.label("tags");
                ui.vertical(|ui| tags_editor(ui, &mut node.tags, &mut self.new_tag));
                ui.end_row();
            });
        });
        if !matches!(node.kind, KindDef::TileLayer(_)) {
            section(ui, "Transform", |ui| transform(ui, &mut node));
        }
        self.kind(ui, &mut node, &sheet_names, document.file(), services);
        section(ui, "Props", |ui| {
            props_editor(
                ui,
                ("props", path.as_slice()),
                &mut node.props,
                &mut self.new_key,
            );
        });

        if node == original {
            return;
        }
        let resized = match (&original.kind, &node.kind) {
            (KindDef::Tilemap(old), KindDef::Tilemap(new))
                if [old.columns, old.rows] != [new.columns, new.rows] =>
            {
                Some([new.columns, new.rows])
            }
            _ => None,
        };
        let file = document.edit();
        *edit::node_mut(file, path).expect("the node is there") = node;
        if let Some([columns, rows]) = resized {
            edit::resize_tilemap(file, path, columns, rows);
        }
    }

    /// The fields of what the node is.
    fn kind(
        &mut self,
        ui: &mut egui::Ui,
        node: &mut NodeDef,
        sheet_names: &[String],
        file: &SceneFile,
        services: &mut Services,
    ) {
        let title = format!(
            "{} {}",
            theme::kind_icon(node.kind.name()),
            node.kind.name()
        );
        match &mut node.kind {
            KindDef::Node => {}
            KindDef::Sprite(sprite) => section(ui, &title, |ui| {
                grid(ui, "sprite", |ui| {
                    ui.label("sheet");
                    egui::ComboBox::from_id_salt("sheet")
                        .selected_text(&sprite.sheet)
                        .show_ui(ui, |ui| {
                            for name in sheet_names {
                                ui.selectable_value(&mut sprite.sheet, name.clone(), name);
                            }
                        });
                    ui.end_row();
                    ui.label("cell");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut sprite.cell[0]).prefix("column "));
                        ui.add(egui::DragValue::new(&mut sprite.cell[1]).prefix("row "));
                        ui.toggle_value(&mut self.picking_cell, "Pick…");
                    });
                    ui.end_row();
                    ui.label("size");
                    ui.horizontal(|ui| {
                        let mut own = sprite.size.is_none();
                        if ui.checkbox(&mut own, "its own").changed() {
                            sprite.size = if own { None } else { Some([16.0, 16.0]) };
                        }
                        if let Some(size) = &mut sprite.size {
                            pair(ui, size, 1.0);
                        }
                    });
                    ui.end_row();
                    ui.label("stretch");
                    pair(ui, &mut sprite.stretch, 0.01);
                    ui.end_row();
                    ui.label("pivot");
                    pair(ui, &mut sprite.pivot, 1.0);
                    ui.end_row();
                    ui.label("tint");
                    color(ui, &mut sprite.tint);
                    ui.end_row();
                });
                if self.picking_cell
                    && let Some(sheet) = file.sheets.get(&sprite.sheet)
                {
                    cell_picker(ui, sheet, &mut sprite.cell, services);
                }
            }),
            KindDef::Rect(rect) => section(ui, &title, |ui| {
                grid(ui, "rect", |ui| {
                    ui.label("size");
                    pair(ui, &mut rect.size, 1.0);
                    ui.end_row();
                    ui.label("stretch");
                    pair(ui, &mut rect.stretch, 0.01);
                    ui.end_row();
                    ui.label("pivot");
                    pair(ui, &mut rect.pivot, 1.0);
                    ui.end_row();
                    ui.label("color");
                    color(ui, &mut rect.color);
                    ui.end_row();
                });
            }),
            KindDef::Text(text) => section(ui, &title, |ui| {
                grid(ui, "text", |ui| {
                    ui.label("text");
                    ui.add(egui::TextEdit::multiline(&mut text.text).desired_rows(2));
                    ui.end_row();
                    ui.label("font scale");
                    ui.add(
                        egui::DragValue::new(&mut text.font_scale)
                            .speed(0.05)
                            .range(0.1..=64.0),
                    );
                    ui.end_row();
                    ui.label("wrap");
                    ui.horizontal(|ui| {
                        let mut wraps = text.wrap.is_some();
                        if ui.checkbox(&mut wraps, "").changed() {
                            text.wrap = wraps.then_some(100.0);
                        }
                        if let Some(wrap) = &mut text.wrap {
                            ui.add(egui::DragValue::new(wrap).range(1.0..=f32::MAX));
                        }
                    });
                    ui.end_row();
                    ui.label("align");
                    egui::ComboBox::from_id_salt("align")
                        .selected_text(text.align.name())
                        .show_ui(ui, |ui| {
                            for align in [Align::Left, Align::Center, Align::Right] {
                                ui.selectable_value(&mut text.align, align, align.name());
                            }
                        });
                    ui.end_row();
                    ui.label("pivot");
                    pair(ui, &mut text.pivot, 1.0);
                    ui.end_row();
                    ui.label("color");
                    color(ui, &mut text.color);
                    ui.end_row();
                });
            }),
            KindDef::Area(area) => section(ui, &title, |ui| {
                grid(ui, "area", |ui| {
                    ui.label("size");
                    pair(ui, &mut area.size, 1.0);
                    ui.end_row();
                    ui.label("stretch");
                    pair(ui, &mut area.stretch, 0.01);
                    ui.end_row();
                    ui.label("pivot");
                    pair(ui, &mut area.pivot, 1.0);
                    ui.end_row();
                });
            }),
            KindDef::Tilemap(map) => section(ui, &title, |ui| {
                grid(ui, "tilemap", |ui| {
                    ui.label("tileset");
                    file_choice(ui, "tileset", &mut map.tileset, services.tilesets);
                    ui.end_row();
                    ui.label("columns");
                    ui.add(egui::DragValue::new(&mut map.columns).range(1..=4096));
                    ui.end_row();
                    ui.label("rows");
                    ui.add(egui::DragValue::new(&mut map.rows).range(1..=4096));
                    ui.end_row();
                });
            }),
            KindDef::TileLayer(layer) => section(ui, &title, |ui| {
                ui.label(format!(
                    "{} rows, {} kinds of tile",
                    layer.tiles.len(),
                    layer.legend.len()
                ));
                grid(ui, "legend", |ui| {
                    for (symbol, tile) in &layer.legend {
                        ui.monospace(format!("'{symbol}'"));
                        ui.label(tile);
                        ui.end_row();
                    }
                });
            }),
            KindDef::Instance(instance) => section(ui, &title, |ui| {
                grid(ui, "instance", |ui| {
                    ui.label("scene");
                    file_choice(ui, "instance scene", &mut instance.scene, services.scenes);
                    ui.end_row();
                });
                if !instance.overrides.is_empty() {
                    ui.label("Overrides");
                    for (path, fields) in &instance.overrides {
                        let fields: Vec<&str> = fields.keys().map(String::as_str).collect();
                        ui.label(format!("{path}: {}", fields.join(", ")));
                    }
                }
            }),
        }
    }
}

/// The name and kind of what's selected, across the top.
fn header(ui: &mut egui::Ui, icon: &str, name: &str, kind: &str) {
    ui.horizontal(|ui| {
        ui.heading(theme::kind_icon(icon));
        ui.heading(name);
        ui.weak(kind);
    });
    ui.separator();
}

fn section(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(egui::RichText::new(title).strong())
        .default_open(true)
        .show(ui, add_contents);
}

fn grid(ui: &mut egui::Ui, id: impl egui::AsIdSalt, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([10.0, 4.0])
        .show(ui, add_contents);
}

/// A field for a number, named `name` for screen readers and tests as its label names it on
/// screen: egui doesn't carry a label's text over to what it labels.
fn number(ui: &mut egui::Ui, name: &str, value: &mut f32, speed: f32) -> egui::Response {
    let response = ui.add(egui::DragValue::new(value).speed(speed));
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::drag_value(response.enabled(), f64::from(*value));
        info.label = Some(name.to_owned());
        info
    });
    response
}

/// A node's position, anchor, rotation and scale.
fn transform(ui: &mut egui::Ui, node: &mut NodeDef) {
    grid(ui, "transform", |ui| {
        ui.label("x");
        number(ui, "x", &mut node.x, 1.0);
        ui.end_row();
        ui.label("y");
        number(ui, "y", &mut node.y, 1.0);
        ui.end_row();
        if matches!(node.kind, KindDef::Tilemap(_) | KindDef::Text(_)) {
            // They move with their node, but don't turn or stretch.
        } else {
            ui.label("rotation");
            let mut degrees = node.rotation.to_degrees();
            let response = ui.add(egui::DragValue::new(&mut degrees).speed(1.0).suffix("°"));
            if response.changed() {
                node.rotation = degrees.to_radians();
            }
            ui.end_row();
            ui.label("scale");
            ui.horizontal(|ui| {
                number(ui, "scale x", &mut node.scale_x, 0.01);
                number(ui, "scale y", &mut node.scale_y, 0.01);
            });
            ui.end_row();
        }
        ui.label("anchor");
        ui.horizontal(|ui| {
            let mut anchored = node.anchor.is_some();
            if ui.checkbox(&mut anchored, "").changed() {
                node.anchor = anchored.then_some([0.0, 0.0]);
            }
            if let Some(anchor) = &mut node.anchor {
                for value in anchor {
                    ui.add(egui::DragValue::new(value).speed(0.01).range(0.0..=1.0));
                }
            }
        });
        ui.end_row();
    });
}

fn pair(ui: &mut egui::Ui, values: &mut [f32; 2], speed: f32) {
    ui.horizontal(|ui| {
        for value in values {
            ui.add(egui::DragValue::new(value).speed(speed));
        }
    });
}

fn color(ui: &mut egui::Ui, color: &mut Color) {
    let bytes =
        [color.r, color.g, color.b, color.a].map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    let mut edited = egui::Color32::from_rgba_unmultiplied(bytes[0], bytes[1], bytes[2], bytes[3]);
    let response = egui::widgets::color_picker::color_edit_button_srgba(
        ui,
        &mut edited,
        egui::widgets::color_picker::Alpha::OnlyBlend,
    );
    if response.changed() {
        let [r, g, b, a] = edited.to_srgba_unmultiplied();
        *color = Color::rgba8(r, g, b, a);
    }
}

/// A choice of one of the project's `files` for `path`, or what it is if it's none of them.
fn file_choice(ui: &mut egui::Ui, id: impl egui::AsIdSalt, path: &mut String, files: &[String]) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(path.as_str())
        .width(180.0)
        .show_ui(ui, |ui| {
            for file in files {
                ui.selectable_value(path, file.clone(), file);
            }
        });
}

fn tags_editor(ui: &mut egui::Ui, tags: &mut Vec<String>, new_tag: &mut String) {
    let mut removed = None;
    ui.horizontal_wrapped(|ui| {
        for (i, tag) in tags.iter().enumerate() {
            if ui
                .button(format!("{tag} ✖"))
                .on_hover_text("Remove the tag")
                .clicked()
            {
                removed = Some(i);
            }
        }
    });
    if let Some(i) = removed {
        tags.remove(i);
    }
    ui.horizontal(|ui| {
        let response = ui.add(
            egui::TextEdit::singleline(new_tag)
                .hint_text("New tag")
                .desired_width(100.0),
        );
        let entered = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let tag = new_tag.trim().to_owned();
        if (ui.button("Add").clicked() || entered) && !tag.is_empty() && !tags.contains(&tag) {
            tags.push(tag);
            new_tag.clear();
        }
    });
}

/// Props: each one's value, to change or remove, and a way to add more.
fn props_editor(
    ui: &mut egui::Ui,
    id: impl egui::AsIdSalt,
    props: &mut Props,
    new_key: &mut String,
) {
    let id = ui.make_persistent_id(id);
    map_entries(ui, id, props);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(new_key)
                .hint_text("New prop's name")
                .desired_width(120.0),
        );
        let key = new_key.trim().to_owned();
        let can_add = !key.is_empty() && !props.contains_key(&key);
        ui.add_enabled_ui(can_add, |ui| {
            ui.menu_button("Add", |ui| {
                if let Some(value) = new_value_menu(ui) {
                    props.insert(key.clone(), value);
                    new_key.clear();
                }
            });
        });
    });
}

/// The entries of a table with strings for keys, each removable.
fn map_entries(ui: &mut egui::Ui, id: egui::Id, map: &mut BTreeMap<String, Value>) {
    let mut removed = None;
    egui::Grid::new(id).num_columns(3).show(ui, |ui| {
        for (key, value) in map.iter_mut() {
            ui.label(key);
            value_editor(ui, id.with(key), value);
            if ui.small_button("🗑").on_hover_text("Remove").clicked() {
                removed = Some(key.clone());
            }
            ui.end_row();
        }
    });
    if let Some(key) = removed {
        map.remove(&key);
    }
}

fn value_editor(ui: &mut egui::Ui, id: egui::Id, value: &mut Value) {
    match value {
        Value::Bool(on) => {
            ui.checkbox(on, "");
        }
        Value::Number(number) => {
            ui.add(egui::DragValue::new(number).speed(0.1));
        }
        Value::String(text) => {
            ui.add(egui::TextEdit::singleline(text).desired_width(160.0));
        }
        Value::List(items) => {
            egui::CollapsingHeader::new(format!("list of {}", items.len()))
                .id_salt(id)
                .show(ui, |ui| {
                    let mut removed = None;
                    for (i, item) in items.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.weak(format!("{}", i + 1));
                            value_editor(ui, id.with(i), item);
                            if ui.small_button("🗑").clicked() {
                                removed = Some(i);
                            }
                        });
                    }
                    if let Some(i) = removed {
                        items.remove(i);
                    }
                    ui.menu_button("Add", |ui| {
                        if let Some(value) = new_value_menu(ui) {
                            items.push(value);
                        }
                    });
                });
        }
        Value::Map(entries) => {
            egui::CollapsingHeader::new(format!("table of {}", entries.len()))
                .id_salt(id)
                .show(ui, |ui| {
                    map_entries(ui, id.with("entries"), entries);
                    let key_id = id.with("new key");
                    let mut key: String =
                        ui.data_mut(|data| data.get_temp(key_id).unwrap_or_default());
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut key)
                                .hint_text("New key")
                                .desired_width(80.0),
                        );
                        let can_add = !key.is_empty() && !entries.contains_key(&key);
                        ui.add_enabled_ui(can_add, |ui| {
                            ui.menu_button("Add", |ui| {
                                if let Some(value) = new_value_menu(ui) {
                                    entries.insert(std::mem::take(&mut key), value);
                                }
                            });
                        });
                    });
                    ui.data_mut(|data| data.insert_temp(key_id, key));
                });
        }
    }
}

/// A menu of kinds of value, returning a new one of the kind clicked.
fn new_value_menu(ui: &mut egui::Ui) -> Option<Value> {
    let kinds = [
        ("Boolean", Value::Bool(false)),
        ("Number", Value::Number(0.0)),
        ("String", Value::String(String::new())),
        ("List", Value::List(Vec::new())),
        ("Table", Value::Map(BTreeMap::new())),
    ];
    kinds
        .into_iter()
        .find_map(|(name, value)| ui.button(name).clicked().then_some(value))
}

/// The sheet, its cells outlined, to pick the sprite's cell from with a click.
fn cell_picker(ui: &mut egui::Ui, sheet: &SheetDef, cell: &mut [u32; 2], services: &mut Services) {
    let texture = match services
        .cache
        .loader(services.ctx, services.folder)
        .texture(&sheet.path)
    {
        Ok(texture) => texture,
        Err(err) => {
            ui.colored_label(ui.visuals().error_fg_color, err.to_string());
            return;
        }
    };
    let shown = services.ctx.gui_texture(&texture);
    let tile = egui::vec2(sheet.tile_width as f32, sheet.tile_height as f32);
    // As large as fits the dock, in whole multiples of the sheet's size where it can be.
    let fit = (ui.available_width() / shown.size.x).max(0.25);
    let scale = if fit >= 1.0 { fit.floor() } else { fit };
    let size = shown.size * scale;
    egui::ScrollArea::both()
        .max_height(320.0)
        .id_salt("cell picker")
        .show(ui, |ui| {
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, theme::BACKGROUND);
            painter.image(
                shown.id,
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            let cell_rect = |[column, row]: [u32; 2]| {
                egui::Rect::from_min_size(
                    rect.min + egui::vec2(column as f32 * tile.x, row as f32 * tile.y) * scale,
                    tile * scale,
                )
            };
            painter.rect_stroke(
                cell_rect(*cell),
                0.0,
                egui::Stroke::new(2.0, theme::ACCENT),
                egui::StrokeKind::Outside,
            );
            if let Some(pointer) = response.hover_pos() {
                let at = ((pointer - rect.min) / scale / tile).floor();
                let hovered = [at.x.max(0.0) as u32, at.y.max(0.0) as u32];
                painter.rect_stroke(
                    cell_rect(hovered),
                    0.0,
                    egui::Stroke::new(1.0, egui::Color32::WHITE),
                    egui::StrokeKind::Outside,
                );
                if response.clicked() {
                    *cell = hovered;
                }
            }
        });
}
