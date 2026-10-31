//! The Tiles panel, in the bottom panel: the tileset of the selected layer's tilemap to paint
//! with, and the tools painting uses. The viewport paints, with what's chosen here.

use std::{collections::BTreeMap, path::Path};

use wingine::{
    Context,
    graphics::Color,
    gui::egui::{self, load::SizedTexture},
    scene::Assets,
};

use crate::{edit::NodePath, project::AssetCache};

/// What a press in the viewport does to the selected tile layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileTool {
    /// Puts the tile in each cell the pointer crosses.
    Paint,
    /// Fills the rectangle dragged out.
    Rect,
    /// Fills the cell clicked and the cells joined to it that hold the same tile.
    Fill,
    /// Chooses the tile in the cell clicked, then paints with it.
    Pick,
}

impl TileTool {
    pub const ALL: [(TileTool, &'static str, &'static str); 4] = [
        (
            TileTool::Paint,
            "Paint",
            "Put the tile in each cell dragged across",
        ),
        (TileTool::Rect, "Rect", "Fill the rectangle dragged out"),
        (
            TileTool::Fill,
            "Fill",
            "Fill the cell clicked and the cells joined to it with the same tile",
        ),
        (
            TileTool::Pick,
            "Pick",
            "Choose the tile in the cell clicked (or Ctrl+click with any tool)",
        ),
    ];
}

pub struct Tiles {
    pub tool: TileTool,
    /// Whether painting empties cells instead, as the right button does.
    pub erase: bool,
    /// The tile to paint with, by its name in the tileset.
    pub tile: Option<String>,
    filter: String,
}

impl Default for Tiles {
    fn default() -> Self {
        Self {
            tool: TileTool::Paint,
            erase: false,
            tile: None,
            filter: String::new(),
        }
    }
}

/// What the panel has to paint: the selected layer, or a tilemap whose layers can be chosen.
pub enum Target<'a> {
    Nothing,
    /// A tilemap, by its layers' names and paths.
    Map(Vec<(String, NodePath)>),
    /// A layer, by its name, with its tileset's tiles or why there are none.
    Layer {
        name: &'a str,
        palette: Result<&'a Palette, &'a str>,
    },
}

impl Tiles {
    /// The panel. Returns the path of a layer to select, if one is clicked.
    pub fn ui(&mut self, ui: &mut egui::Ui, target: Target) -> Option<NodePath> {
        let (name, palette) = match target {
            Target::Nothing => {
                ui.weak("Select a tilemap's layer in the Scene dock to paint it.");
                return None;
            }
            Target::Map(layers) => {
                let mut chosen = None;
                ui.horizontal_wrapped(|ui| {
                    ui.weak("Choose a layer to paint:");
                    for (name, path) in layers {
                        if ui.button(format!("☰ {name}")).clicked() {
                            chosen = Some(path);
                        }
                    }
                });
                return chosen;
            }
            Target::Layer { name, palette } => (name, palette),
        };
        ui.horizontal(|ui| {
            for (tool, label, hint) in TileTool::ALL {
                ui.selectable_value(&mut self.tool, tool, label)
                    .on_hover_text(hint);
            }
            ui.separator();
            ui.toggle_value(&mut self.erase, "Erase")
                .on_hover_text("Empty cells instead (or use the right button)");
            ui.separator();
            let tile = match (&self.tile, self.erase) {
                (_, true) => "nothing".to_owned(),
                (Some(tile), false) => format!("`{tile}`"),
                (None, false) => "nothing yet".to_owned(),
            };
            ui.label(format!("Painting {name} with {tile}"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("Filter tiles")
                        .desired_width(140.0),
                );
            });
        });
        let palette = match palette {
            Ok(palette) => palette,
            Err(err) => {
                ui.colored_label(ui.visuals().error_fg_color, err);
                return None;
            }
        };
        let filter = self.filter.to_lowercase();
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for tile in &palette.tiles {
                        if !tile.name.to_lowercase().contains(&filter) {
                            continue;
                        }
                        let selected = !self.erase && self.tile.as_ref() == Some(&tile.name);
                        let response = ui
                            .add(egui::Button::selectable(selected, tile.image(32.0)))
                            .on_hover_text(&tile.name);
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::Role::Button,
                                true,
                                selected,
                                &tile.name,
                            )
                        });
                        if response.clicked() {
                            self.choose(tile.name.clone());
                        }
                    }
                });
            });
        None
    }

    /// Paints with `tile` from now on.
    pub fn choose(&mut self, tile: String) {
        self.tile = Some(tile);
        self.erase = false;
        if self.tool == TileTool::Pick {
            self.tool = TileTool::Paint;
        }
    }
}

/// The tiles of a tileset, ready to show.
pub struct Palette {
    /// The tileset file's path.
    pub path: String,
    pub tiles: Vec<PaletteTile>,
}

pub struct PaletteTile {
    pub name: String,
    /// Its sheet's image, sized as a cell of it.
    pub texture: SizedTexture,
    /// Where its sprite is in the image, from 0 to 1.
    pub uv: egui::Rect,
    pub tint: egui::Color32,
}

impl PaletteTile {
    /// Its sprite, `size` points square.
    pub fn image(&self, size: f32) -> egui::Image<'static> {
        egui::Image::from_texture(self.texture)
            .uv(self.uv)
            .tint(self.tint)
            .fit_to_exact_size(egui::vec2(size, size))
    }
}

impl Palette {
    /// The tiles of the tileset file at `path` in the game's `folder`.
    pub fn load(
        ctx: &mut Context,
        cache: &mut AssetCache,
        folder: &Path,
        path: &str,
    ) -> Result<Self, String> {
        let mut loader = cache.loader(ctx, folder);
        let tileset = loader.tileset(path).map_err(|err| err.to_string())?;
        let mut sheets = BTreeMap::new();
        for (name, sheet) in &tileset.sheets {
            let texture = loader.texture(&sheet.path).map_err(|err| err.to_string())?;
            sheets.insert(name, (texture, sheet));
        }
        let mut tiles = Vec::with_capacity(tileset.tiles.len());
        for (name, tile) in &tileset.tiles {
            let Some((texture, sheet)) = sheets.get(&tile.sheet) else {
                continue;
            };
            let shown = ctx.gui_texture(texture);
            let cell = egui::vec2(sheet.tile_width as f32, sheet.tile_height as f32);
            let [column, row] = tile.cell;
            let min = egui::pos2(column as f32 * cell.x, row as f32 * cell.y);
            tiles.push(PaletteTile {
                name: name.clone(),
                texture: SizedTexture::new(shown.id, cell),
                uv: egui::Rect::from_min_size(
                    (min.to_vec2() / shown.size).to_pos2(),
                    cell / shown.size,
                ),
                tint: color32(tile.tint),
            });
        }
        Ok(Self {
            path: path.to_owned(),
            tiles,
        })
    }

    pub fn tile(&self, name: &str) -> Option<&PaletteTile> {
        self.tiles.iter().find(|tile| tile.name == name)
    }
}

fn color32(color: Color) -> egui::Color32 {
    let [r, g, b, a] =
        [color.r, color.g, color.b, color.a].map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}
