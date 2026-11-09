//! The Inspector: every field of the selected node, or of the scene itself, to change.
//!
//! It edits a copy of what's selected, and puts the copy into the file only if something
//! changed, so the file's history holds changes and nothing else. A node an instance places shows
//! as the instance makes it, and changing it changes the instance's overrides of it: fields they
//! change are marked, with a button that puts each back as the scene has it. While a clip poses
//! the preview, nodes show posed, the properties it has tracks of are marked, and each property a
//! clip can change has a button that keys it.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use wingine::{
    Context,
    graphics::{Align, Blend, Camera3d, Color, Projection},
    gui::egui,
    particles::ParticleSettings,
    scene::{
        CellDef, KindDef, NodeDef, PanelFill, PanelLookDef, Placement3, Props, Shape, SheetDef,
        Stack, StackDirection, Value,
    },
};

use crate::{
    clips,
    document::Document,
    edit::{self, NodePath},
    instance,
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
    pub models: &'a [String],
    pub fonts: &'a [String],
}

#[derive(Default)]
pub struct Inspector {
    /// Names being typed for new tags, props and sheets.
    new_tag: String,
    new_key: String,
    new_sheet: String,
    /// Whether the sheet of the selected sprite is shown to pick a cell from.
    picking_cell: bool,
    /// A node inside the selected instance to select, by its path of names, once its overrides'
    /// list has been shown.
    select_inner: Option<String>,
}

/// What the Inspector marks beside fields: those an instance's override of a node changes, and
/// those asked to be put back as the scene has them, or the properties the clip posing the
/// preview keys.
#[derive(Default)]
struct Marks {
    fields: BTreeSet<String>,
    props: BTreeSet<String>,
    /// The props of the node as its scene has it, which an override changes but can't remove.
    scene_props: BTreeSet<String>,
    reverted: Vec<String>,
    reverted_props: Vec<String>,
    keying: Option<Keying>,
}

/// A clip posing the preview: the properties of the node it has tracks of, and those asked to be
/// keyed.
struct Keying {
    clip: String,
    time: f32,
    tracked: Vec<&'static str>,
    keyed: Vec<String>,
}

impl Marks {
    /// `text`, the label of the field that's `keys` in a scene file, marked if the override
    /// changes any of them, with a button that reverts them, or while a clip poses the preview,
    /// if the clip has tracks of them, with a button that keys them.
    fn label(&mut self, ui: &mut egui::Ui, text: &str, keys: &[&str]) {
        if keys.iter().any(|key| self.fields.contains(*key)) {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(text).color(theme::ACCENT))
                    .on_hover_text("Overridden by the instance");
                if revert_button(ui) {
                    self.reverted.extend(keys.iter().map(|key| key.to_string()));
                }
            });
            return;
        }
        let Some(keying) = self
            .keying
            .as_mut()
            .filter(|_| keys.iter().all(|key| clips::PROPERTIES.contains(key)))
        else {
            ui.label(text);
            return;
        };
        ui.horizontal(|ui| {
            if keys.iter().any(|key| keying.tracked.contains(key)) {
                ui.label(egui::RichText::new(text).color(theme::KEY))
                    .on_hover_text(format!("Keyed by `{}`", keying.clip));
            } else {
                ui.label(text);
            }
            let response = ui.small_button("♦").on_hover_text(format!(
                "Key it at {:.2} s of `{}`",
                keying.time, keying.clip
            ));
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Key {text}"))
            });
            if response.clicked() {
                keying.keyed.extend(keys.iter().map(|key| key.to_string()));
            }
        });
    }
}

/// A button that puts a field back as the scene has it, returning whether it was clicked.
fn revert_button(ui: &mut egui::Ui) -> bool {
    ui.small_button("⟲")
        .on_hover_text("Revert to the scene's value")
        .clicked()
}

impl Inspector {
    pub fn ui(&mut self, ui: &mut egui::Ui, document: &mut Document, services: &mut Services) {
        let Some(path) = document.selected.clone() else {
            ui.weak("Select a node in the Scene dock or the viewport to see its fields.");
            return;
        };
        let inner = document.inner().map(str::to_owned);
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                if let Some(names) = inner {
                    self.inner(ui, document, &path, &names, services);
                } else if path.is_empty() {
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
            props_editor(
                ui,
                "scene props",
                &mut props,
                &mut self.new_key,
                &mut Marks::default(),
            );
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
                ui.weak("Edit them in the Animation panel.");
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
        // As the preview shows it, posed.
        let Some(original) = document.shown_node(path) else {
            return;
        };
        let sheets = document.file().sheets.clone();
        let mut node = original.clone();
        let mut marks = Marks {
            keying: document.posing().map(|(clip, _, time)| Keying {
                clip: clip.to_owned(),
                time,
                tracked: document.keyed(path),
                keyed: Vec::new(),
            }),
            ..Marks::default()
        };
        header(ui, node.kind.name(), &node.name, node.kind.name());
        self.fields(
            ui,
            &mut node,
            &sheets,
            ("props", path.as_slice()),
            services,
            &mut marks,
        );
        if let Some(names) = self.select_inner.take() {
            document.select_inner(path.clone(), names);
        }

        if node != original {
            let resized = match (&original.kind, &node.kind) {
                (KindDef::Tilemap(old), KindDef::Tilemap(new))
                    if [old.columns, old.rows] != [new.columns, new.rows] =>
                {
                    Some([new.columns, new.rows])
                }
                _ => None,
            };
            document.set_node(path, node);
            if let Some([columns, rows]) = resized {
                edit::resize_tilemap(document.edit(), path, columns, rows);
            }
        }
        if let Some(keying) = marks.keying
            && !keying.keyed.is_empty()
        {
            let keyed: Vec<&str> = keying.keyed.iter().map(String::as_str).collect();
            document.key(path, &keyed);
        }
    }

    /// The node at `names` inside the instance at `instance`, as the instance makes it, whose
    /// changes go into the instance's overrides.
    fn inner(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        instance: &NodePath,
        names: &str,
        services: &mut Services,
    ) {
        let Some((instance_name, def)) =
            edit::node(document.file(), instance).and_then(|node| match &node.kind {
                KindDef::Instance(def) => Some((node.name.clone(), def.clone())),
                _ => None,
            })
        else {
            return;
        };
        let change = def.overrides.get(names).cloned().unwrap_or_default();
        let placed = instance::placed(
            &mut services.cache.loader(services.ctx, services.folder),
            &def.scene,
            names,
        );
        let placed = match placed {
            Ok(placed) => placed,
            Err(err) => {
                ui.colored_label(ui.visuals().error_fg_color, err);
                return;
            }
        };
        let name = names.rsplit('/').next().unwrap_or(names);
        let kind = placed.node.kind.name();
        header(ui, kind, name, &format!("{kind} in {}", def.scene));
        ui.horizontal_wrapped(|ui| {
            ui.weak(format!(
                "Placed by `{instance_name}`: changes are kept as its overrides."
            ));
        });
        let mut revert_all = false;
        let mut before = match placed.overridden(&change) {
            Ok(before) => before,
            Err(err) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("Its overrides don't fit it: {err}"),
                );
                revert_all = ui.button("Revert All").clicked();
                placed.node.clone()
            }
        };
        // A node its own file places in the 2D scene may be in a 3D view here, as the preview has
        // it, and is placed in 3D.
        if before.placement3.is_none() && document.is_in_3d(instance, names) {
            before.placement3 = Some(Placement3::default());
            before.rotation = 0.0;
        }
        if !change.is_empty() && !revert_all {
            revert_all = ui
                .button("Revert All")
                .on_hover_text("Put every field back as the scene has it")
                .clicked();
        }
        ui.separator();

        let mut over = Marks {
            fields: change
                .keys()
                .filter(|key| *key != "props")
                .cloned()
                .collect(),
            props: match change.get("props") {
                Some(Value::Map(props)) => props.keys().cloned().collect(),
                _ => BTreeSet::new(),
            },
            scene_props: placed.node.props.keys().cloned().collect(),
            ..Marks::default()
        };
        let mut after = before.clone();
        self.fields(
            ui,
            &mut after,
            &placed.sheets,
            ("props", instance.as_slice(), names),
            services,
            &mut over,
        );

        let changed = if revert_all {
            BTreeMap::new()
        } else {
            changed_override(&change, &placed.node, &before, &after, &over)
        };
        if changed == change {
            return;
        }
        let file = document.edit();
        if let Some(KindDef::Instance(def)) =
            edit::node_mut(file, instance).map(|node| &mut node.kind)
        {
            if changed.is_empty() {
                def.overrides.remove(names);
            } else {
                def.overrides.insert(names.to_owned(), changed);
            }
        }
    }

    /// The sections of a node's fields, with sprites' sheets from `sheets`.
    fn fields(
        &mut self,
        ui: &mut egui::Ui,
        node: &mut NodeDef,
        sheets: &BTreeMap<String, SheetDef>,
        props_id: impl egui::AsIdSalt,
        services: &mut Services,
        over: &mut Marks,
    ) {
        section(ui, "Node", |ui| {
            grid(ui, "node", |ui| {
                over.label(ui, "visible", &["visible"]);
                ui.checkbox(&mut node.visible, "");
                ui.end_row();
                over.label(ui, "tags", &["tags"]);
                ui.vertical(|ui| tags_editor(ui, &mut node.tags, &mut self.new_tag));
                ui.end_row();
            });
        });
        if node.placement3.is_some() {
            section(ui, "Transform", |ui| transform_3d(ui, node, over));
        } else if !matches!(node.kind, KindDef::TileLayer(_)) {
            section(ui, "Transform", |ui| transform(ui, node, over));
        }
        self.kind(ui, node, sheets, services, over);
        section(ui, "Props", |ui| {
            props_editor(ui, props_id, &mut node.props, &mut self.new_key, over);
        });
    }

    /// The fields of what the node is.
    fn kind(
        &mut self,
        ui: &mut egui::Ui,
        node: &mut NodeDef,
        sheets: &BTreeMap<String, SheetDef>,
        services: &mut Services,
        over: &mut Marks,
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
                    over.label(ui, "sheet", &["sheet"]);
                    egui::ComboBox::from_id_salt("sheet")
                        .selected_text(&sprite.sheet)
                        .show_ui(ui, |ui| {
                            for name in sheets.keys() {
                                ui.selectable_value(&mut sprite.sheet, name.clone(), name);
                            }
                        });
                    ui.end_row();
                    over.label(ui, "cell", &["cell"]);
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut sprite.cell[0]).prefix("column "));
                        ui.add(egui::DragValue::new(&mut sprite.cell[1]).prefix("row "));
                        ui.toggle_value(&mut self.picking_cell, "Pick…");
                    });
                    ui.end_row();
                    over.label(ui, "size", &["width", "height"]);
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
                    over.label(ui, "stretch", &["stretch"]);
                    pair(ui, &mut sprite.stretch, 0.01);
                    ui.end_row();
                    over.label(ui, "pivot", &["pivot"]);
                    pair(ui, &mut sprite.pivot, 1.0);
                    ui.end_row();
                    over.label(ui, "tint", &["tint"]);
                    color(ui, &mut sprite.tint);
                    ui.end_row();
                    over.label(ui, "blend", &["blend"]);
                    blend_choice(ui, &mut sprite.blend);
                    ui.end_row();
                });
                if self.picking_cell
                    && let Some(sheet) = sheets.get(&sprite.sheet)
                {
                    cell_picker(ui, sheet, &mut sprite.cell, services);
                }
            }),
            KindDef::Rect(rect) => section(ui, &title, |ui| {
                grid(ui, "rect", |ui| {
                    over.label(ui, "size", &["width", "height"]);
                    pair(ui, &mut rect.size, 1.0);
                    ui.end_row();
                    over.label(ui, "stretch", &["stretch"]);
                    pair(ui, &mut rect.stretch, 0.01);
                    ui.end_row();
                    over.label(ui, "pivot", &["pivot"]);
                    pair(ui, &mut rect.pivot, 1.0);
                    ui.end_row();
                    over.label(ui, "color", &["color"]);
                    color(ui, &mut rect.color);
                    ui.end_row();
                    over.label(ui, "blend", &["blend"]);
                    blend_choice(ui, &mut rect.blend);
                    ui.end_row();
                });
            }),
            KindDef::Text(text) => section(ui, &title, |ui| {
                grid(ui, "text", |ui| {
                    over.label(ui, "text", &["text"]);
                    ui.add(egui::TextEdit::multiline(&mut text.text).desired_rows(2));
                    ui.end_row();
                    over.label(ui, "font", &["font"]);
                    font_choice(ui, &mut text.font, services.fonts);
                    ui.end_row();
                    over.label(ui, "font scale", &["font_scale"]);
                    ui.add(
                        egui::DragValue::new(&mut text.font_scale)
                            .speed(0.05)
                            .range(0.1..=64.0),
                    );
                    ui.end_row();
                    over.label(ui, "wrap", &["wrap"]);
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
                    over.label(ui, "align", &["align"]);
                    egui::ComboBox::from_id_salt("align")
                        .selected_text(text.align.name())
                        .show_ui(ui, |ui| {
                            for align in [Align::Left, Align::Center, Align::Right] {
                                ui.selectable_value(&mut text.align, align, align.name());
                            }
                        });
                    ui.end_row();
                    over.label(ui, "pivot", &["pivot"]);
                    pair(ui, &mut text.pivot, 1.0);
                    ui.end_row();
                    over.label(ui, "color", &["color"]);
                    color(ui, &mut text.color);
                    ui.end_row();
                    over.label(ui, "blend", &["blend"]);
                    blend_choice(ui, &mut text.blend);
                    ui.end_row();
                });
            }),
            KindDef::Area(area) => section(ui, &title, |ui| {
                grid(ui, "area", |ui| {
                    over.label(ui, "size", &["width", "height"]);
                    pair(ui, &mut area.size, 1.0);
                    ui.end_row();
                    over.label(ui, "stretch", &["stretch"]);
                    pair(ui, &mut area.stretch, 0.01);
                    ui.end_row();
                    over.label(ui, "pivot", &["pivot"]);
                    pair(ui, &mut area.pivot, 1.0);
                    ui.end_row();
                });
            }),
            KindDef::Tilemap(map) => section(ui, &title, |ui| {
                grid(ui, "tilemap", |ui| {
                    over.label(ui, "tileset", &["tileset"]);
                    file_choice(ui, "tileset", &mut map.tileset, services.tilesets);
                    ui.end_row();
                    over.label(ui, "columns", &["columns"]);
                    ui.add(egui::DragValue::new(&mut map.columns).range(1..=4096));
                    ui.end_row();
                    over.label(ui, "rows", &["rows"]);
                    ui.add(egui::DragValue::new(&mut map.rows).range(1..=4096));
                    ui.end_row();
                });
            }),
            KindDef::TileLayer(layer) => section(ui, &title, |ui| {
                ui.label(format!(
                    "{} rows, {} kinds of tile. Paint it from the Tiles panel.",
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
                grid(ui, "tile layer", |ui| {
                    over.label(ui, "blend", &["blend"]);
                    blend_choice(ui, &mut layer.blend);
                    ui.end_row();
                });
            }),
            KindDef::Instance(instance) => {
                section(ui, &title, |ui| {
                    grid(ui, "instance", |ui| {
                        over.label(ui, "scene", &["scene"]);
                        file_choice(ui, "instance scene", &mut instance.scene, services.scenes);
                        ui.end_row();
                    });
                    if instance.overrides.is_empty() {
                        ui.weak("No overrides: open the instance in the Scene dock to change its nodes.");
                        return;
                    }
                    ui.strong("Overrides");
                    let mut reverted = None;
                    for (target, change) in &instance.overrides {
                        ui.horizontal(|ui| {
                            if ui.link(target).on_hover_text("Select it").clicked() {
                                self.select_inner = Some(target.clone());
                            }
                            if ui
                                .small_button("⟲")
                                .on_hover_text("Revert its overrides")
                                .clicked()
                            {
                                reverted = Some(target.clone());
                            }
                        });
                        ui.indent(target, |ui| {
                            for (field, value) in change {
                                ui.weak(format!("{field} = {}", describe(value)));
                            }
                        });
                    }
                    if let Some(target) = reverted {
                        instance.overrides.remove(&target);
                    }
                })
            }
            KindDef::View3d(view) => section(ui, &title, |ui| {
                grid(ui, "view3d", |ui| {
                    over.label(ui, "size", &["width", "height"]);
                    pair(ui, &mut view.size, 1.0);
                    ui.end_row();
                    over.label(ui, "stretch", &["stretch"]);
                    pair(ui, &mut view.stretch, 0.01);
                    ui.end_row();
                    over.label(ui, "pivot", &["pivot"]);
                    pair(ui, &mut view.pivot, 1.0);
                    ui.end_row();
                });
                ui.strong("Camera");
                grid(ui, "camera", |ui| camera(ui, &mut view.camera, over));
                ui.strong("Lighting");
                grid(ui, "lighting", |ui| {
                    let lighting = &mut view.lighting;
                    over.label(ui, "sun's direction", &["lighting"]);
                    triple(ui, "sun's direction", &mut lighting.direction, 0.05);
                    ui.end_row();
                    over.label(ui, "sun", &["lighting"]);
                    color(ui, &mut lighting.color);
                    ui.end_row();
                    over.label(ui, "ambient", &["lighting"]);
                    color(ui, &mut lighting.ambient);
                    ui.end_row();
                });
            }),
            KindDef::Mesh(mesh) => section(ui, &title, |ui| {
                grid(ui, "mesh", |ui| {
                    over.label(ui, "shape", &["shape"]);
                    egui::ComboBox::from_id_salt("shape")
                        .selected_text(mesh.shape.name())
                        .show_ui(ui, |ui| {
                            for shape in [
                                Shape::Cuboid([1.0; 3]),
                                Shape::Plane([1.0; 2]),
                                Shape::Sphere(0.5),
                            ] {
                                let chosen = mesh.shape.name() == shape.name();
                                if ui.selectable_label(chosen, shape.name()).clicked() && !chosen {
                                    mesh.shape = shape;
                                }
                            }
                        });
                    ui.end_row();
                    match &mut mesh.shape {
                        Shape::Cuboid(size) => {
                            over.label(ui, "size", &["size"]);
                            triple(ui, "size", size, 0.05);
                        }
                        Shape::Plane(size) => {
                            over.label(ui, "size", &["size"]);
                            ui.horizontal(|ui| {
                                number(ui, "size x", &mut size[0], 0.05);
                                number(ui, "size z", &mut size[1], 0.05);
                            });
                        }
                        Shape::Sphere(radius) => {
                            over.label(ui, "radius", &["radius"]);
                            number(ui, "radius", radius, 0.05);
                        }
                    }
                    ui.end_row();
                    // Sizes are above 0.
                    match &mut mesh.shape {
                        Shape::Cuboid(size) => size.iter_mut().for_each(at_least_a_little),
                        Shape::Plane(size) => size.iter_mut().for_each(at_least_a_little),
                        Shape::Sphere(radius) => at_least_a_little(radius),
                    }
                    over.label(ui, "sprite", &["sheet", "cell"]);
                    ui.horizontal(|ui| {
                        let mut shows = mesh.sprite.is_some();
                        let first = sheets.keys().next();
                        let response = ui.add_enabled(
                            first.is_some() || shows,
                            egui::Checkbox::new(&mut shows, ""),
                        );
                        if response.changed() {
                            mesh.sprite =
                                shows
                                    .then(|| first.cloned())
                                    .flatten()
                                    .map(|sheet| CellDef {
                                        sheet,
                                        cell: [0, 0],
                                    });
                        }
                        if let Some(sprite) = &mut mesh.sprite {
                            sheet_choice(ui, &mut sprite.sheet, sheets);
                        }
                    });
                    ui.end_row();
                    if let Some(sprite) = &mut mesh.sprite {
                        over.label(ui, "cell", &["cell"]);
                        cell_fields(ui, &mut sprite.cell, &mut self.picking_cell);
                        ui.end_row();
                    }
                    over.label(ui, "tint", &["tint"]);
                    color(ui, &mut mesh.tint);
                    ui.end_row();
                });
                if self.picking_cell
                    && let Some(sprite) = &mut mesh.sprite
                    && let Some(sheet) = sheets.get(&sprite.sheet)
                {
                    cell_picker(ui, sheet, &mut sprite.cell, services);
                }
            }),
            KindDef::Model(model) => section(ui, &title, |ui| {
                grid(ui, "model", |ui| {
                    over.label(ui, "model", &["model"]);
                    file_choice(ui, "model file", &mut model.model, services.models);
                    ui.end_row();
                    over.label(ui, "tint", &["tint"]);
                    color(ui, &mut model.tint);
                    ui.end_row();
                });
            }),
            KindDef::Billboard(billboard) => section(ui, &title, |ui| {
                grid(ui, "billboard", |ui| {
                    over.label(ui, "sheet", &["sheet"]);
                    sheet_choice(ui, &mut billboard.sheet, sheets);
                    ui.end_row();
                    over.label(ui, "cell", &["cell"]);
                    cell_fields(ui, &mut billboard.cell, &mut self.picking_cell);
                    ui.end_row();
                    over.label(ui, "size", &["width", "height"]);
                    ui.horizontal(|ui| {
                        number(ui, "width", &mut billboard.size[0], 0.05);
                        number(ui, "height", &mut billboard.size[1], 0.05);
                    });
                    ui.end_row();
                    over.label(ui, "tint", &["tint"]);
                    color(ui, &mut billboard.tint);
                    ui.end_row();
                });
                if self.picking_cell
                    && let Some(sheet) = sheets.get(&billboard.sheet)
                {
                    cell_picker(ui, sheet, &mut billboard.cell, services);
                }
            }),
            KindDef::Stack(stack) => section(ui, &title, |ui| {
                grid(ui, "stack", |ui| stack_fields(ui, stack, over));
            }),
            KindDef::Panel(panel) => section(ui, &title, |ui| {
                grid(ui, "panel", |ui| {
                    stack_fields(ui, &mut panel.stack, over);
                    over.label(ui, "look", &["sheet", "cell", "color"]);
                    let showing = matches!(panel.look, PanelLookDef::Sprite { .. });
                    let first = sheets.keys().next();
                    egui::ComboBox::from_id_salt("look")
                        .selected_text(if showing { "sprite" } else { "fill" })
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(!showing, "fill").clicked() && showing {
                                panel.look = PanelLookDef::Fill(PanelFill {
                                    color: Color::WHITE,
                                    border: 0.0,
                                    border_color: Color::WHITE,
                                });
                            }
                            let pick = ui.add_enabled(
                                first.is_some(),
                                egui::Button::selectable(showing, "sprite"),
                            );
                            if pick.clicked()
                                && !showing
                                && let Some(sheet) = first
                            {
                                panel.look = PanelLookDef::Sprite {
                                    sprite: CellDef {
                                        sheet: sheet.clone(),
                                        cell: [0, 0],
                                    },
                                    slice: [0.0; 4],
                                    tint: Color::WHITE,
                                };
                            }
                        })
                        .response
                        .widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::ComboBox, true, "Panel look")
                        });
                    ui.end_row();
                    match &mut panel.look {
                        PanelLookDef::Fill(fill) => {
                            over.label(ui, "color", &["color"]);
                            color(ui, &mut fill.color);
                            ui.end_row();
                            over.label(ui, "border", &["border"]);
                            ui.horizontal(|ui| {
                                number(ui, "border", &mut fill.border, 0.1);
                                color(ui, &mut fill.border_color);
                            });
                            fill.border = fill.border.max(0.0);
                            ui.end_row();
                        }
                        PanelLookDef::Sprite {
                            sprite,
                            slice,
                            tint,
                        } => {
                            over.label(ui, "sheet", &["sheet"]);
                            sheet_choice(ui, &mut sprite.sheet, sheets);
                            ui.end_row();
                            over.label(ui, "cell", &["cell"]);
                            cell_fields(ui, &mut sprite.cell, &mut self.picking_cell);
                            ui.end_row();
                            over.label(ui, "slice", &["slice"]);
                            edges(ui, "slice", slice);
                            for texels in slice.iter_mut() {
                                *texels = texels.max(0.0);
                            }
                            ui.end_row();
                            over.label(ui, "tint", &["tint"]);
                            color(ui, tint);
                            ui.end_row();
                        }
                    }
                });
                if self.picking_cell
                    && let PanelLookDef::Sprite { sprite, .. } = &mut panel.look
                    && let Some(sheet) = sheets.get(&sprite.sheet)
                {
                    cell_picker(ui, sheet, &mut sprite.cell, services);
                }
            }),
            KindDef::Particles(particles) => section(ui, &title, |ui| {
                grid(ui, "particles", |ui| {
                    over.label(ui, "sprite", &["sheet", "cell"]);
                    ui.horizontal(|ui| {
                        let mut shows = particles.sprite.is_some();
                        let first = sheets.keys().next();
                        let response = ui.add_enabled(
                            first.is_some() || shows,
                            egui::Checkbox::new(&mut shows, ""),
                        );
                        if response.changed() {
                            particles.sprite =
                                shows
                                    .then(|| first.cloned())
                                    .flatten()
                                    .map(|sheet| CellDef {
                                        sheet,
                                        cell: [0, 0],
                                    });
                        }
                        match &mut particles.sprite {
                            Some(sprite) => sheet_choice(ui, &mut sprite.sheet, sheets),
                            None => {
                                ui.weak("squares");
                            }
                        }
                    });
                    ui.end_row();
                    if let Some(sprite) = &mut particles.sprite {
                        over.label(ui, "cell", &["cell"]);
                        cell_fields(ui, &mut sprite.cell, &mut self.picking_cell);
                        ui.end_row();
                    }
                    over.label(ui, "size", &["width", "height"]);
                    pair(ui, &mut particles.size, 1.0);
                    for side in &mut particles.size {
                        *side = side.max(0.0);
                    }
                    ui.end_row();
                    over.label(ui, "stretch", &["stretch"]);
                    pair(ui, &mut particles.stretch, 0.01);
                    ui.end_row();
                    over.label(ui, "pivot", &["pivot"]);
                    pair(ui, &mut particles.pivot, 1.0);
                    ui.end_row();
                    particle_fields(ui, &mut particles.settings, over);
                });
                if self.picking_cell
                    && let Some(sprite) = &mut particles.sprite
                    && let Some(sheet) = sheets.get(&sprite.sheet)
                {
                    cell_picker(ui, sheet, &mut sprite.cell, services);
                }
            }),
        }
    }
}

/// The settings of particles, in a grid's rows, kept to values they can have.
fn particle_fields(ui: &mut egui::Ui, settings: &mut ParticleSettings, over: &mut Marks) {
    over.label(ui, "emitting", &["emitting"]);
    let emitting = ui.checkbox(&mut settings.emitting, "");
    emitting.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, true, settings.emitting, "emitting")
    });
    ui.end_row();
    over.label(ui, "rate", &["rate"]);
    number(ui, "rate", &mut settings.rate, 0.1);
    settings.rate = settings.rate.max(0.0);
    ui.end_row();
    over.label(ui, "limit", &["limit"]);
    ui.add(egui::DragValue::new(&mut settings.limit).range(1..=100_000));
    ui.end_row();
    over.label(ui, "warmup", &["warmup"]);
    number(ui, "warmup", &mut settings.warmup, 0.05);
    settings.warmup = settings.warmup.max(0.0);
    ui.end_row();
    over.label(ui, "seed", &["seed"]);
    ui.horizontal(|ui| {
        let mut seeded = settings.seed.is_some();
        if ui.checkbox(&mut seeded, "").changed() {
            settings.seed = seeded.then_some(1);
        }
        match &mut settings.seed {
            Some(seed) => {
                ui.add(egui::DragValue::new(seed));
            }
            None => {
                ui.weak("its path's");
            }
        }
    });
    ui.end_row();
    over.label(ui, "lifetime", &["lifetime"]);
    range(ui, "lifetime", &mut settings.lifetime, 0.05);
    settings.lifetime[0] = settings.lifetime[0].max(0.01);
    settings.lifetime[1] = settings.lifetime[1].max(settings.lifetime[0]);
    ui.end_row();
    over.label(ui, "speed", &["speed"]);
    range(ui, "speed", &mut settings.speed, 0.5);
    settings.speed[1] = settings.speed[1].max(settings.speed[0]);
    ui.end_row();
    over.label(ui, "direction", &["direction"]);
    degrees(ui, "direction", &mut settings.direction);
    ui.end_row();
    over.label(ui, "spread", &["spread"]);
    degrees(ui, "spread", &mut settings.spread);
    settings.spread = settings.spread.clamp(0.0, std::f32::consts::TAU);
    ui.end_row();
    over.label(ui, "gravity", &["gravity"]);
    ui.horizontal(|ui| {
        number(ui, "gravity x", &mut settings.gravity[0], 1.0);
        number(ui, "gravity y", &mut settings.gravity[1], 1.0);
    });
    ui.end_row();
    over.label(ui, "drag", &["drag"]);
    number(ui, "drag", &mut settings.drag, 0.05);
    settings.drag = settings.drag.max(0.0);
    ui.end_row();
    over.label(ui, "angle", &["angle"]);
    ui.horizontal(|ui| {
        degrees(ui, "angle least", &mut settings.angle[0]);
        degrees(ui, "angle most", &mut settings.angle[1]);
    });
    settings.angle[1] = settings.angle[1].max(settings.angle[0]);
    ui.end_row();
    over.label(ui, "spin", &["spin"]);
    ui.horizontal(|ui| {
        degrees(ui, "spin least", &mut settings.spin[0]);
        degrees(ui, "spin most", &mut settings.spin[1]);
    });
    settings.spin[1] = settings.spin[1].max(settings.spin[0]);
    ui.end_row();
    over.label(ui, "scales", &["scales"]);
    ui.horizontal_wrapped(|ui| {
        list(ui, "scale", &mut settings.scales, 1.0, |ui, i, scale| {
            number(ui, &format!("scale {}", i + 1), scale, 0.05);
            *scale = scale.max(0.0);
        });
    });
    ui.end_row();
    over.label(ui, "colors", &["colors"]);
    ui.horizontal_wrapped(|ui| {
        let last = settings.colors.last().copied().unwrap_or(Color::WHITE);
        list(ui, "color", &mut settings.colors, last, |ui, _, value| {
            color(ui, value);
        });
    });
    ui.end_row();
    over.label(ui, "blend", &["blend"]);
    blend_choice(ui, &mut settings.blend);
    ui.end_row();
    over.label(ui, "attached", &["attached"]);
    let attached = ui.checkbox(&mut settings.attached, "move with it");
    attached.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, true, settings.attached, "attached")
    });
    ui.end_row();
}

/// Fields for a range's least and most, named `name least` and `name most`.
fn range(ui: &mut egui::Ui, name: &str, values: &mut [f32; 2], speed: f32) {
    ui.horizontal(|ui| {
        number(ui, &format!("{name} least"), &mut values[0], speed);
        number(ui, &format!("{name} most"), &mut values[1], speed);
    });
}

/// A field for each of `values`, then buttons that add `new` after them and take the last away,
/// keeping at least one.
fn list<T: Clone>(
    ui: &mut egui::Ui,
    name: &str,
    values: &mut Vec<T>,
    new: T,
    mut field: impl FnMut(&mut egui::Ui, usize, &mut T),
) {
    for (i, value) in values.iter_mut().enumerate() {
        field(ui, i, value);
    }
    let add = ui.small_button("+");
    add.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Add a {name}"))
    });
    if add.on_hover_text(format!("Add a {name}")).clicked() {
        values.push(new);
    }
    let removable = values.len() > 1;
    let remove = ui.add_enabled(removable, egui::Button::new("−").small());
    remove.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, removable, format!("Remove a {name}"))
    });
    if remove
        .on_hover_text(format!("Remove the last {name}"))
        .clicked()
    {
        values.pop();
    }
}

/// A choice of how a node mixes with what's under it.
fn blend_choice(ui: &mut egui::Ui, blend: &mut Blend) {
    egui::ComboBox::from_id_salt("blend")
        .selected_text(blend.name())
        .show_ui(ui, |ui| {
            for mode in Blend::ALL {
                ui.selectable_value(blend, mode, mode.name());
            }
        })
        .response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, "blend"));
}

/// The fields of a stack or a panel that size, place and lay it out, in a grid's rows: along each
/// axis, its own size or fitting what it holds.
fn stack_fields(ui: &mut egui::Ui, stack: &mut Stack, over: &mut Marks) {
    for (i, (field, fits)) in [("width", "fit width"), ("height", "fit height")]
        .into_iter()
        .enumerate()
    {
        over.label(ui, field, &[field]);
        ui.horizontal(|ui| {
            let mut fitting = stack.size[i].is_none();
            let response = ui.checkbox(&mut fitting, "fits");
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::CheckBox, true, fitting, fits)
            });
            if response.changed() {
                stack.size[i] = if fitting { None } else { Some(16.0) };
                if fitting {
                    stack.stretch[i] = 0.0;
                }
            }
            if let Some(length) = &mut stack.size[i] {
                number(ui, field, length, 1.0);
            }
        });
        ui.end_row();
    }
    over.label(ui, "stretch", &["stretch"]);
    ui.horizontal(|ui| {
        for (i, axis) in ["x", "y"].into_iter().enumerate() {
            ui.add_enabled_ui(stack.size[i].is_some(), |ui| {
                number(ui, &format!("stretch {axis}"), &mut stack.stretch[i], 0.01);
            });
        }
    });
    ui.end_row();
    over.label(ui, "direction", &["direction"]);
    egui::ComboBox::from_id_salt("direction")
        .selected_text(stack.direction.name())
        .show_ui(ui, |ui| {
            for direction in [StackDirection::Column, StackDirection::Row] {
                ui.selectable_value(&mut stack.direction, direction, direction.name());
            }
        });
    ui.end_row();
    over.label(ui, "gap", &["gap"]);
    number(ui, "gap", &mut stack.gap, 0.1);
    ui.end_row();
    over.label(ui, "padding", &["padding"]);
    edges(ui, "padding", &mut stack.padding);
    ui.end_row();
    over.label(ui, "pivot", &["pivot"]);
    pair(ui, &mut stack.pivot, 1.0);
    ui.end_row();
}

/// Numbers for the left, top, right and bottom edges of something, named `name` and the edge.
fn edges(ui: &mut egui::Ui, name: &str, edges: &mut [f32; 4]) {
    ui.horizontal(|ui| {
        for (edge, value) in ["left", "top", "right", "bottom"].into_iter().zip(edges) {
            number(ui, &format!("{name} {edge}"), value, 0.1);
        }
    });
}

/// Keeps a size above 0, as a shape's must be.
fn at_least_a_little(size: &mut f32) {
    *size = size.max(0.01);
}

/// A choice of one of `sheets` for `sheet`.
fn sheet_choice(ui: &mut egui::Ui, sheet: &mut String, sheets: &BTreeMap<String, SheetDef>) {
    egui::ComboBox::from_id_salt("sheet")
        .selected_text(sheet.as_str())
        .show_ui(ui, |ui| {
            for name in sheets.keys() {
                ui.selectable_value(sheet, name.clone(), name);
            }
        });
}

/// A sprite's cell, its column and row, and a toggle that shows its sheet to pick one from.
fn cell_fields(ui: &mut egui::Ui, cell: &mut [u32; 2], picking: &mut bool) {
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut cell[0]).prefix("column "));
        ui.add(egui::DragValue::new(&mut cell[1]).prefix("row "));
        ui.toggle_value(picking, "Pick…");
    });
}

/// A view's camera: where it is and looks, which way is up, how it projects, and how near and far
/// it sees.
fn camera(ui: &mut egui::Ui, camera: &mut Camera3d, over: &mut Marks) {
    over.label(ui, "position", &["camera"]);
    triple(ui, "camera position", &mut camera.position, 0.05);
    ui.end_row();
    over.label(ui, "target", &["camera"]);
    triple(ui, "camera target", &mut camera.target, 0.05);
    ui.end_row();
    over.label(ui, "up", &["camera"]);
    triple(ui, "camera up", &mut camera.up, 0.05);
    ui.end_row();
    over.label(ui, "projection", &["camera"]);
    ui.horizontal(|ui| {
        let perspective = matches!(camera.projection, Projection::Perspective { .. });
        if ui.selectable_label(perspective, "perspective").clicked() && !perspective {
            camera.projection = Projection::Perspective { fov_y: 0.8 };
        }
        if ui.selectable_label(!perspective, "orthographic").clicked() && perspective {
            let distance = camera
                .position
                .iter()
                .zip(camera.target)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f32>()
                .sqrt();
            camera.projection = Projection::Orthographic {
                height: distance.max(1.0),
            };
        }
    });
    ui.end_row();
    match &mut camera.projection {
        Projection::Perspective { fov_y } => {
            over.label(ui, "field of view", &["camera"]);
            degrees(ui, "field of view", fov_y);
            *fov_y = fov_y.clamp(1f32.to_radians(), 179f32.to_radians());
        }
        Projection::Orthographic { height } => {
            over.label(ui, "height", &["camera"]);
            number(ui, "camera height", height, 0.05);
            *height = height.max(0.01);
        }
    }
    ui.end_row();
    over.label(ui, "near, far", &["camera"]);
    ui.horizontal(|ui| {
        number(ui, "near", &mut camera.near, 0.01);
        number(ui, "far", &mut camera.far, 1.0);
    });
    camera.near = camera.near.max(0.001);
    camera.far = camera.far.max(camera.near + 0.001);
    ui.end_row();
}

/// An instance's override of a node, `change`, after the node as the instance made it, `before`,
/// became `after` in the Inspector, and fields in `over` were reverted. A field changed back to
/// what the scene has, as `base`, leaves the override, and a field changed to anything else joins
/// it. Props are changed one by one.
fn changed_override(
    change: &BTreeMap<String, Value>,
    base: &NodeDef,
    before: &NodeDef,
    after: &NodeDef,
    over: &Marks,
) -> BTreeMap<String, Value> {
    let mut change = change.clone();
    let (base, before, after) = (base.fields(), before.fields(), after.fields());
    for (key, value) in &after {
        if key == "props" || before.get(key) == Some(value) {
            continue;
        }
        if base.get(key) == Some(value) {
            change.remove(key);
        } else {
            change.insert(key.clone(), tidy(key, value));
        }
    }
    // Drop overrides of fields that left, including those of a panel's previous appearance.
    // An optional stack size has an explicit `false` value when it fits instead.
    for key in before.keys() {
        if !after.contains_key(key) {
            change.remove(key);
        }
    }
    let props = |fields: &BTreeMap<String, Value>| match fields.get("props") {
        Some(Value::Map(props)) => props.clone(),
        _ => BTreeMap::new(),
    };
    let (base_props, before_props, after_props) = (props(&base), props(&before), props(&after));
    let mut changed_props = match change.remove("props") {
        Some(Value::Map(props)) => props,
        _ => BTreeMap::new(),
    };
    for (key, value) in &after_props {
        if before_props.get(key) == Some(value) {
            continue;
        }
        if base_props.get(key) == Some(value) {
            changed_props.remove(key);
        } else {
            changed_props.insert(key.clone(), value.clone());
        }
    }
    // A prop removed leaves the override: one only it adds goes, and one the scene has goes back
    // to the scene's.
    for key in before_props.keys() {
        if !after_props.contains_key(key) {
            changed_props.remove(key);
        }
    }
    for key in &over.reverted {
        change.remove(key);
    }
    for key in &over.reverted_props {
        changed_props.remove(key);
    }
    if !changed_props.is_empty() {
        change.insert("props".into(), Value::Map(changed_props));
    }
    change
}

/// A field's value as a person would write it: numbers as short as reads back the same, and
/// opaque colors without their alpha.
fn tidy(key: &str, value: &Value) -> Value {
    match value {
        Value::Number(number) => {
            let single = *number as f32;
            let short = if f64::from(single) == *number {
                format!("{single}").parse().unwrap_or(*number)
            } else {
                *number
            };
            Value::Number(short)
        }
        Value::List(items) => Value::List(items.iter().map(|item| tidy(key, item)).collect()),
        Value::String(hex)
            if matches!(key, "tint" | "color" | "colors")
                && hex.len() == 9
                && hex.ends_with("ff") =>
        {
            Value::String(hex[..7].to_owned())
        }
        value => value.clone(),
    }
}

/// `value` as a scene file would write it, on one line.
fn describe(value: &Value) -> String {
    match value {
        Value::Bool(on) => on.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => format!("{text:?}"),
        Value::List(items) => {
            let items: Vec<String> = items.iter().map(describe).collect();
            format!("{{ {} }}", items.join(", "))
        }
        Value::Map(entries) => {
            let entries: Vec<String> = entries
                .iter()
                .map(|(key, value)| format!("{key} = {}", describe(value)))
                .collect();
            format!("{{ {} }}", entries.join(", "))
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
fn transform(ui: &mut egui::Ui, node: &mut NodeDef, over: &mut Marks) {
    grid(ui, "transform", |ui| {
        over.label(ui, "x", &["x"]);
        number(ui, "x", &mut node.x, 1.0);
        ui.end_row();
        over.label(ui, "y", &["y"]);
        number(ui, "y", &mut node.y, 1.0);
        ui.end_row();
        if matches!(
            node.kind,
            KindDef::Tilemap(_) | KindDef::Text(_) | KindDef::View3d(_)
        ) {
            // They move with their node, but don't turn or stretch.
        } else {
            over.label(ui, "rotation", &["rotation"]);
            let mut degrees = node.rotation.to_degrees();
            let response = ui.add(egui::DragValue::new(&mut degrees).speed(1.0).suffix("°"));
            if response.changed() {
                node.rotation = degrees.to_radians();
            }
            ui.end_row();
            over.label(ui, "scale", &["scale_x", "scale_y"]);
            ui.horizontal(|ui| {
                number(ui, "scale x", &mut node.scale_x, 0.01);
                number(ui, "scale y", &mut node.scale_y, 0.01);
            });
            ui.end_row();
        }
        over.label(ui, "anchor", &["anchor"]);
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

/// A node placed in 3D's position, orientation and scale, along its view's world's axes, with `y`
/// up. Billboards only move, so they have no orientation or scale.
fn transform_3d(ui: &mut egui::Ui, node: &mut NodeDef, over: &mut Marks) {
    let only_moves = matches!(node.kind, KindDef::Billboard(_));
    let Some(placement) = &mut node.placement3 else {
        return;
    };
    grid(ui, "transform", |ui| {
        for (name, value) in [
            ("x", &mut node.x),
            ("y", &mut node.y),
            ("z", &mut placement.z),
        ] {
            over.label(ui, name, &[name]);
            number(ui, name, value, 0.05);
            ui.end_row();
        }
        if only_moves {
            return;
        }
        for (name, value) in [
            ("yaw", &mut placement.yaw),
            ("pitch", &mut placement.pitch),
            ("roll", &mut placement.roll),
        ] {
            over.label(ui, name, &[name]);
            degrees(ui, name, value);
            ui.end_row();
        }
        over.label(ui, "scale", &["scale_x", "scale_y", "scale_z"]);
        ui.horizontal(|ui| {
            number(ui, "scale x", &mut node.scale_x, 0.01);
            number(ui, "scale y", &mut node.scale_y, 0.01);
            number(ui, "scale z", &mut placement.scale_z, 0.01);
        });
        ui.end_row();
    });
}

/// A field for an angle in radians, shown in degrees, named `name` as [`number`] names fields.
fn degrees(ui: &mut egui::Ui, name: &str, radians: &mut f32) {
    let mut degrees = radians.to_degrees();
    let response = ui.add(egui::DragValue::new(&mut degrees).speed(1.0).suffix("°"));
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::drag_value(response.enabled(), f64::from(degrees));
        info.label = Some(name.to_owned());
        info
    });
    if response.changed() {
        *radians = degrees.to_radians();
    }
}

/// Fields for three numbers, `x`, `y` and `z`, named `name` and each axis's name.
fn triple(ui: &mut egui::Ui, name: &str, values: &mut [f32; 3], speed: f32) {
    ui.horizontal(|ui| {
        for (axis, value) in ["x", "y", "z"].into_iter().zip(values) {
            number(ui, &format!("{name} {axis}"), value, speed);
        }
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

/// A choice of the built-in font or one of the project's font `files` for `font`.
fn font_choice(ui: &mut egui::Ui, font: &mut Option<String>, files: &[String]) {
    let choice = egui::ComboBox::from_id_salt("font")
        .selected_text(font.as_deref().unwrap_or(BUILT_IN_FONT))
        .width(180.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(font, None, BUILT_IN_FONT);
            for file in files {
                ui.selectable_value(font, Some(file.clone()), file);
            }
        });
    // egui doesn't carry the label beside it over to it.
    choice
        .response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, "font"));
}

/// What the Inspector calls the font text has without a font file.
const BUILT_IN_FONT: &str = "Built-in";

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

/// Props: each one's value, to change or remove, and a way to add more. Props an override changes
/// are marked, with a button that reverts each.
fn props_editor(
    ui: &mut egui::Ui,
    id: impl egui::AsIdSalt,
    props: &mut Props,
    new_key: &mut String,
    over: &mut Marks,
) {
    let id = ui.make_persistent_id(id);
    map_entries(ui, id, props, Some(over));
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

/// The entries of a table with strings for keys, each removable, and for props, marked if an
/// override changes them.
fn map_entries(
    ui: &mut egui::Ui,
    id: egui::Id,
    map: &mut BTreeMap<String, Value>,
    mut over: Option<&mut Marks>,
) {
    let mut removed = None;
    egui::Grid::new(id).num_columns(3).show(ui, |ui| {
        for (key, value) in map.iter_mut() {
            match over.as_deref_mut().filter(|over| over.props.contains(key)) {
                Some(over) => {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(key).color(theme::ACCENT))
                            .on_hover_text("Overridden by the instance");
                        if revert_button(ui) {
                            over.reverted_props.push(key.clone());
                        }
                    });
                }
                None => {
                    ui.label(key);
                }
            }
            value_editor(ui, id.with(key), value);
            // An instance's override can't remove a prop its scene has.
            let removable = !over
                .as_deref()
                .is_some_and(|over| over.scene_props.contains(key));
            if removable && ui.small_button("🗑").on_hover_text("Remove").clicked() {
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
                    map_entries(ui, id.with("entries"), entries, None);
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

#[cfg(test)]
mod tests {
    use wingine::scene::SceneFile;

    use super::*;

    fn flower() -> NodeDef {
        let file = SceneFile::parse(
            r##"return { format = 1,
                sheets = { tiles = { path = "tiles.png", tile_width = 16, tile_height = 16 } },
                children = {
                    { name = "flower", kind = "sprite", sheet = "tiles", cell = { 15, 6 },
                      tint = "#e8a0b0", props = { scent = 1 } },
                } }"##,
            "flower.scene.luau",
        )
        .unwrap();
        file.children[0].clone()
    }

    #[test]
    fn overrides_keep_what_differs_from_the_scene() {
        let base = flower();
        let change: BTreeMap<String, Value> =
            [("tint".to_owned(), Value::String("#e0c070".into()))].into();
        let mut before = base.clone();
        let KindDef::Sprite(sprite) = &mut before.kind else {
            unreachable!()
        };
        sprite.tint = Color::rgba8(0xe0, 0xc0, 0x70, 0xff);

        // Moving it adds its position, tidily.
        let mut after = before.clone();
        after.x = 12.3;
        let changed = changed_override(&change, &base, &before, &after, &Marks::default());
        assert_eq!(changed["x"], Value::Number(12.3));
        assert_eq!(changed["tint"], Value::String("#e0c070".into()));
        assert!(!changed.contains_key("y"));

        // Tinting it as the scene does takes the tint out.
        let mut after = before.clone();
        let KindDef::Sprite(sprite) = &mut after.kind else {
            unreachable!()
        };
        sprite.tint = Color::rgba8(0xe8, 0xa0, 0xb0, 0xff);
        let changed = changed_override(&change, &base, &before, &after, &Marks::default());
        assert!(changed.is_empty(), "{changed:?}");

        // Props one by one, and reverting.
        let mut after = before.clone();
        after.props.insert("scent".into(), Value::Number(2.0));
        let over = Marks {
            reverted: vec!["tint".into()],
            ..Marks::default()
        };
        let changed = changed_override(&change, &base, &before, &after, &over);
        assert_eq!(
            changed,
            [(
                "props".to_owned(),
                Value::Map([("scent".to_owned(), Value::Number(2.0))].into())
            )]
            .into()
        );
    }

    #[test]
    fn props_removed_leave_the_override() {
        let base = flower();
        let props = Value::Map([("petals".to_owned(), Value::Number(5.0))].into());
        let change: BTreeMap<String, Value> = [("props".to_owned(), props)].into();
        let mut before = base.clone();
        before.props.insert("petals".into(), Value::Number(5.0));
        let mut after = before.clone();
        after.props.remove("petals");
        let changed = changed_override(&change, &base, &before, &after, &Marks::default());
        assert!(changed.is_empty(), "{changed:?}");
    }

    #[test]
    fn panel_appearance_changes_discard_overrides_of_the_old_look() {
        let scene = SceneFile::parse(
            "return { format = 3,
                sheets = { ui = { path = 'ui.png', tile_width = 5, tile_height = 5 } },
                children = {
                    { name = 'fill', kind = 'panel', color = '#123456', border = 2 },
                    { name = 'sprite', kind = 'panel', sheet = 'ui', cell = { 0, 0 }, slice = 1 },
                } }",
            "panels.scene.luau",
        )
        .unwrap();
        // Start with an existing override of either appearance, then change to the other.
        for (from, to, field, value) in [
            (0, 1, "border", Value::Number(3.0)),
            (1, 0, "slice", Value::Number(2.0)),
        ] {
            let base = &scene.children[from];
            let change = [(field.to_owned(), value)].into();
            let before = base.overridden(&change, &scene.sheets, None).unwrap();
            let mut after = before.clone();
            after.kind = scene.children[to].kind.clone();
            let changed = changed_override(&change, base, &before, &after, &Marks::default());
            assert!(!changed.contains_key(field));
            assert_eq!(
                base.overridden(&changed, &scene.sheets, None).unwrap(),
                after
            );
            let reverted = changed_override(&changed, base, &after, base, &Marks::default());
            assert!(reverted.is_empty(), "{reverted:?}");
        }
    }
}
