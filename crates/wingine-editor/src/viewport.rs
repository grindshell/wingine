//! The 2D viewport: the open scene, drawn by the engine as the game draws it, panned with the
//! middle button and zoomed with the wheel, over a grid and with the game's canvas outlined.
//! Clicking selects the node under the pointer, and dragging moves, turns or stretches the
//! selected node, by the tool chosen. With a tile layer selected, pressing paints it instead, by
//! the Tiles panel's tool, and a selected tilemap has handles that resize it. Scenes dragged in
//! from the FileSystem dock are placed where they're dropped.
//!
//! For laying out screens, the toolbar's Anchors menu places the selected node by a preset, and
//! with the Select tool, a node with a size has handles on its edges and corners that resize it.
//! The scene can be previewed laid out on canvases of other sizes, chosen in the toolbar or by
//! dragging the canvas's bottom-right corner.
//!
//! In a 3D view, clicking picks the nodes whose boxes are under the pointer, nearest first, the
//! selection's box is outlined, and the Move tool drags a node across the ground at its height, or
//! with Shift, up and down. Scenes and model files dropped in a view are placed where the ray
//! under the pointer meets the ground.

use std::f32::consts::PI;

use wingine::{
    Context,
    graphics::{Camera, Canvas, View3d},
    gui::{GuiCanvas, egui},
    math::{Affine, Affine3, Rect},
    scene::{Kind, KindDef, NodeDef, NodeId, Scene},
};

use crate::{
    docks::{
        filesystem::{ModelPath, ScenePath},
        tiles::{Palette, TileTool, Tiles},
    },
    document::{Document, View},
    edit::{self, NodePath},
    layout::{self, PRESETS, Place, Preset},
    project::is_model_file,
    theme,
};

/// Canvas sizes to preview a scene laid out on, besides the game's own.
const PREVIEW_SIZES: [[f32; 2]; 7] = [
    [320.0, 180.0],
    [640.0, 360.0],
    [1280.0, 720.0],
    [1920.0, 1080.0],
    [800.0, 600.0],
    [360.0, 640.0],
    [1080.0, 1920.0],
];
/// The smallest canvas a preview can have, across or down.
const MIN_PREVIEW: f32 = 16.0;

/// How far from a plain node's origin, in points, a click still picks it.
const PICK_DISTANCE: f32 = 6.0;
/// How far from a handle's middle, in points, a press still takes it.
const HANDLE_SIZE: f32 = 6.0;
/// The most columns or rows a tilemap can be resized to, as the Inspector allows.
const MAX_CELLS: u32 = 4096;
/// The angle turning snaps to.
const ANGLE_STEP: f32 = PI / 12.0;
/// The amount stretching snaps to.
const SCALE_STEP: f32 = 0.1;
const MIN_ZOOM: f32 = 1.0 / 16.0;
const MAX_ZOOM: f32 = 64.0;

/// What dragging in the viewport does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// Clicking selects; dragging a node moves it.
    Select,
    Move,
    Rotate,
    Scale,
}

impl Tool {
    pub const ALL: [(Tool, &'static str, &'static str); 4] = [
        (Tool::Select, "Select", "Q"),
        (Tool::Move, "Move", "W"),
        (Tool::Rotate, "Rotate", "E"),
        (Tool::Scale, "Scale", "S"),
    ];
}

pub struct Viewport {
    canvas: GuiCanvas,
    pub tool: Tool,
    /// Whether moving snaps to the grid, turning to steps of 15°, and stretching to tenths.
    pub snap: bool,
    /// The grid's spacing, in the scene's units.
    pub step: f32,
    drag: Option<Drag>,
    /// Painting a tile layer, in progress.
    stroke: Option<Stroke>,
    /// Resizing a tilemap, in progress.
    resize: Option<Resize>,
    /// Resizing a node with a size, in progress.
    sizing: Option<Sizing>,
    /// Whether the canvas's corner is being dragged, to preview another size.
    sizing_canvas: bool,
    /// The size of canvas scenes are laid out on, instead of the game's, to see how they fit
    /// others.
    pub preview: Option<[f32; 2]>,
    /// How far in from its parent's edges an anchor preset puts a node.
    pub margin: f32,
    /// How the scene mapped to the viewport when it was last shown.
    shown: Option<ViewMap>,
    /// Where the last click selected something, in points.
    last_click: Option<egui::Pos2>,
}

/// Resizing a node by dragging one of the handles on its edges or corners.
struct Sizing {
    path: NodePath,
    start: NodeDef,
    /// Which of its edges move, across and down: -1 the left or top, 1 the right or bottom, 0
    /// neither.
    edges: [i8; 2],
    /// Its own size, before stretching with its parent, and the rectangle it covers as drawn, in
    /// its own coordinates.
    own: [f32; 2],
    rect: Rect,
    /// From the scene to its own coordinates.
    to_node: Affine,
    /// From its own directions to its parent's, for moving it as its left or top edge moves.
    to_parent: Affine,
}

/// What the viewport asks of the editor.
pub enum Action {
    /// Place the scene file at `scene` at `at`, a point of the scene.
    Instance { scene: String, at: [f32; 2] },
    /// Place the scene file at `scene` in the 3D view at `view`, at `at` in its world.
    Instance3d {
        scene: String,
        view: NodePath,
        at: [f32; 3],
    },
    /// Add a node showing the model file at `model` to the 3D view at `view`, at `at` in its
    /// world.
    Model {
        model: String,
        view: NodePath,
        at: [f32; 3],
    },
}

/// Painting a tile layer while a button is held. It ends when the layer isn't painted in a frame,
/// as when it's no longer selected.
struct Stroke {
    /// The layer painted.
    layer: NodePath,
    tool: TileTool,
    /// The tile put down, by its name, or none to empty cells.
    tile: Option<String>,
    /// The cell the stroke started in, and the one the pointer was last in.
    from: [i32; 2],
    last: [i32; 2],
}

/// Resizing a tilemap by dragging one of its handles.
struct Resize {
    path: NodePath,
    /// Whether the drag changes the map's columns, and its rows.
    axes: [bool; 2],
}

/// Where a tilemap is in the scene: its top-left corner and the size of its cells, with its
/// columns and rows.
#[derive(Clone, Copy)]
struct Grid {
    origin: [f32; 2],
    cell: [f32; 2],
    size: [u32; 2],
}

impl Grid {
    /// The grid of the tilemap at `path` in the file, as the preview has it.
    fn of(document: &Document, path: &[usize]) -> Option<Grid> {
        let scene = document.preview.as_ref()?;
        let id = document.live_node(path)?;
        let (_, map) = scene.tilemap(id)?;
        let size = [map.map.columns(), map.map.rows()];
        let bounds = map.map.bounds();
        Some(Grid {
            origin: scene.transform(id).translation,
            cell: [bounds.w / size[0] as f32, bounds.h / size[1] as f32],
            size,
        })
    }

    /// The cell `point` of the scene is in, which may be off the map.
    fn cell_at(&self, point: [f32; 2]) -> [i32; 2] {
        [0, 1].map(|i| ((point[i] - self.origin[i]) / self.cell[i]).floor() as i32)
    }

    fn contains(&self, [column, row]: [i32; 2]) -> bool {
        (0..self.size[0] as i32).contains(&column) && (0..self.size[1] as i32).contains(&row)
    }

    /// The point of the scene at the top-left corner of `cell`.
    fn corner(&self, cell: [i32; 2]) -> [f32; 2] {
        [0, 1].map(|i| self.origin[i] + cell[i] as f32 * self.cell[i])
    }

    /// The handles that resize the map, by the sizes they change and where they are: the middle of
    /// its right edge, of its bottom edge, and its bottom-right corner.
    fn handles(&self) -> [([bool; 2], [f32; 2]); 3] {
        let [right, bottom] = self.corner(self.size.map(|n| n as i32));
        let middle = [0, 1].map(|i| (self.origin[i] + [right, bottom][i]) / 2.0);
        [
            ([true, false], [right, middle[1]]),
            ([false, true], [middle[0], bottom]),
            ([true, true], [right, bottom]),
        ]
    }
}

/// A drag changing a node, in progress.
struct Drag {
    /// What the drag does.
    tool: Tool,
    /// The node it changes, which was `start` when it began.
    path: NodePath,
    start: NodeDef,
    /// Where the pointer was in the scene when it began.
    from: [f32; 2],
    /// From the scene's directions to the node's parent's, for turning moves into its `x` and
    /// `y`.
    to_parent: Affine,
    /// Whether its parent mirrors it, which turns it the other way.
    mirrored: bool,
    /// Its origin in the scene.
    origin: [f32; 2],
    /// From the scene to its own coordinates, as it was.
    to_node: Option<Affine>,
    /// For a node placed in 3D, how moving it in its view works.
    space: Option<Drag3d>,
}

/// Moving a node placed in 3D: across the ground at its height, or up and down.
struct Drag3d {
    /// Its view, where it is in the scene.
    view: View3d,
    /// Its origin in the view's world.
    origin: [f32; 3],
    /// Which way the plane it moves up and down in faces: toward the camera, across the ground.
    facing: [f32; 3],
    /// Where the pointer's ray met the ground at its height, and the upright plane, as the drag
    /// began.
    across: Option<[f32; 3]>,
    upright: Option<[f32; 3]>,
    /// From the world's directions to its parent's, for turning moves into its `x`, `y` and `z`.
    to_parent: Affine3,
}

impl Drag3d {
    /// How far the node moves in the world, in its parent's directions, for the pointer at `now`:
    /// across the ground, or if `upright`, up and down. The pointer may leave the view.
    fn moved(&self, now: [f32; 2], upright: bool) -> Option<[f32; 3]> {
        let ray = self.view.ray(now);
        let world = if upright {
            let hit = ray.hit_plane(self.origin, self.facing)?;
            [0.0, hit[1] - self.upright?[1], 0.0]
        } else {
            let hit = ray.hit_plane(self.origin, [0.0, 1.0, 0.0])?;
            let from = self.across?;
            [hit[0] - from[0], 0.0, hit[2] - from[2]]
        };
        Some(self.to_parent.apply_vector(world))
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            canvas: GuiCanvas::new(),
            tool: Tool::Select,
            snap: false,
            step: 8.0,
            drag: None,
            stroke: None,
            resize: None,
            sizing: None,
            sizing_canvas: false,
            preview: None,
            margin: 0.0,
            shown: None,
            last_click: None,
        }
    }
}

/// How the scene maps to the viewport's points this frame, through the engine's camera that draws
/// it on the viewport's pixels.
#[derive(Clone, Copy)]
struct ViewMap {
    rect: egui::Rect,
    pixels_per_point: f32,
    /// Pixels per unit of the scene.
    scale: f32,
    camera: Camera,
    /// The viewport's size in pixels.
    pixels: [f32; 2],
}

impl ViewMap {
    fn new(rect: egui::Rect, pixels_per_point: f32, view: View, pixels: [u32; 2]) -> Self {
        let scale = view.zoom * pixels_per_point;
        Self {
            rect,
            pixels_per_point,
            scale,
            camera: Camera {
                zoom: scale,
                ..Camera::new(view.center[0], view.center[1])
            },
            pixels: pixels.map(|p| p as f32),
        }
    }

    fn to_screen(self, point: [f32; 2]) -> egui::Pos2 {
        let [x, y] = self
            .camera
            .world_to_canvas(point, self.pixels[0], self.pixels[1]);
        self.rect.min + egui::vec2(x, y) / self.pixels_per_point
    }

    fn to_scene(self, pos: egui::Pos2) -> [f32; 2] {
        let pixels = (pos - self.rect.min) * self.pixels_per_point;
        self.camera
            .canvas_to_world([pixels.x, pixels.y], self.pixels[0], self.pixels[1])
    }
}

impl Viewport {
    /// Where `point` of the scene was in the window, in points, when the viewport was last shown.
    pub fn scene_to_window(&self, point: [f32; 2]) -> Option<egui::Pos2> {
        self.shown.map(|map| map.to_screen(point))
    }

    /// The size of canvas scenes are laid out on: the one previewed, or the game's, `game`.
    pub fn canvas_size(&self, game: [f32; 2]) -> [f32; 2] {
        self.preview.unwrap_or(game)
    }

    /// The toolbar over the viewport, for a game whose canvas is `game` in size.
    pub fn toolbar(&mut self, ui: &mut egui::Ui, document: Option<&mut Document>, game: [f32; 2]) {
        let canvas_size = self.canvas_size(game);
        ui.horizontal(|ui| {
            for (tool, name, key) in Tool::ALL {
                ui.selectable_value(&mut self.tool, tool, name)
                    .on_hover_text(format!("{name} ({key})"));
            }
            ui.separator();
            ui.toggle_value(&mut self.snap, "Snap")
                .on_hover_text("Snap to the grid, 15° and tenths of a scale");
            ui.add(
                egui::DragValue::new(&mut self.step)
                    .range(1.0..=1024.0)
                    .prefix("grid "),
            );
            let Some(document) = document else {
                return;
            };
            ui.separator();
            if ui
                .button("Frame")
                .on_hover_text("Show the whole canvas")
                .clicked()
            {
                document.view = None;
            }
            if ui
                .button("1:1")
                .on_hover_text("A pixel of the scene for each of the screen's")
                .clicked()
                && let Some(view) = &mut document.view
            {
                view.zoom = 1.0 / ui.ctx().pixels_per_point();
            }
            if let Some(view) = document.view {
                ui.weak(format!("{:.0}%", view.zoom * 100.0));
            }
            ui.separator();
            self.canvas_menu(ui, canvas_size, game);
            self.anchors_menu(ui, document);
        });
    }

    /// The size of canvas the scene is laid out on, to preview it on others.
    fn canvas_menu(&mut self, ui: &mut egui::Ui, [width, height]: [f32; 2], game: [f32; 2]) {
        let text = match self.preview {
            Some(_) => format!("preview {width} x {height}"),
            None => format!("canvas {width} x {height}"),
        };
        ui.menu_button(text, |ui| {
            let [game_width, game_height] = game;
            if ui
                .button(format!("The game's, {game_width} x {game_height}"))
                .clicked()
            {
                self.preview = None;
            }
            ui.separator();
            for size @ [width, height] in PREVIEW_SIZES {
                if ui.button(format!("{width} x {height}")).clicked() {
                    self.preview = Some(size);
                }
            }
            ui.separator();
            let mut size = [width, height];
            ui.horizontal(|ui| {
                for value in &mut size {
                    ui.add(egui::DragValue::new(value).range(MIN_PREVIEW..=8192.0));
                }
            });
            if size != [width, height] {
                self.preview = Some(size);
            }
        })
        .response
        .on_hover_text("The canvas the scene is laid out on: drag its corner to try others");
    }

    /// Anchor presets for the selected node.
    fn anchors_menu(&mut self, ui: &mut egui::Ui, document: &mut Document) {
        let selected = document
            .selected
            .clone()
            .filter(|path| !path.is_empty() && document.inner().is_none());
        let node = selected
            .as_ref()
            .and_then(|path| document.shown_node(path))
            .filter(|node| !matches!(node.kind, KindDef::TileLayer(_)));
        if node.as_ref().is_some_and(|node| node.placement3.is_some()) {
            ui.add_enabled(false, egui::Button::new("Anchors"))
                .on_disabled_hover_text("Nodes in 3D views have no anchors");
            return;
        }
        let (Some(path), Some(node)) = (selected, node) else {
            ui.add_enabled(false, egui::Button::new("Anchors"))
                .on_disabled_hover_text("Select a node to lay it out in its parent");
            return;
        };
        let mut chosen = None;
        let mut unanchor = false;
        ui.menu_button("Anchors", |ui| {
            ui.label(format!("Lay out `{}` in its parent:", node.name));
            egui::Grid::new("anchor presets")
                .spacing([4.0, 4.0])
                .show(ui, |ui| {
                    for (i, preset) in PRESETS.iter().enumerate() {
                        let fits = layout::fits(&node, preset);
                        if preset_button(ui, preset, fits).clicked() {
                            chosen = Some(preset);
                        }
                        if i % 4 == 3 {
                            ui.end_row();
                        }
                    }
                });
            ui.horizontal(|ui| {
                ui.label("margin");
                ui.add(egui::DragValue::new(&mut self.margin).range(0.0..=4096.0));
            });
            if ui
                .add_enabled(node.anchor.is_some(), egui::Button::new("No Anchor"))
                .on_hover_text("Place it from its parent's origin, where it is now")
                .clicked()
            {
                unanchor = true;
            }
        })
        .response
        .on_hover_text("Anchor presets: place the node at a point of its parent, or stretch it");

        let live = document.live_node(&path);
        let scene = document.preview.as_ref();
        let mut changed = node.clone();
        if let Some(preset) = chosen {
            let size = live
                .zip(scene)
                .and_then(|(id, scene)| scene.rect(id))
                .map_or([0.0, 0.0], |rect| [rect.w, rect.h]);
            layout::apply(&mut changed, preset, size, self.margin);
        }
        if unanchor && let (Some(id), Some(scene)) = (live, scene) {
            // Where its origin is in its parent, with the anchor's offset.
            let parent = scene
                .node(id)
                .parent()
                .map_or(Affine::IDENTITY, |parent| scene.transform(parent));
            let origin = scene.transform(id).translation;
            if let Some(to_parent) = parent.inverse() {
                [changed.x, changed.y] = to_parent.apply(origin);
                changed.anchor = None;
            }
        }
        if changed != node {
            document.set_node(&path, changed);
            document.end_change();
        }
    }

    /// The viewport, painting the selected tile layer with what `tiles` says, from `palette`,
    /// the tiles of its tileset.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &mut Context,
        document: &mut Document,
        canvas_size: [f32; 2],
        tiles: &mut Tiles,
        palette: Option<&Palette>,
    ) -> Option<Action> {
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        if rect.width() < 1.0 || rect.height() < 1.0 {
            return None;
        }
        let pixels_per_point = ui.ctx().pixels_per_point();
        let pixels = [rect.width(), rect.height()].map(|v| (v * pixels_per_point).round() as u32);
        let view = document
            .view
            .get_or_insert_with(|| frame(canvas_size, rect.size()));

        // Zooming about the pointer, so what's under it stays there.
        if let Some(pointer) = response.hover_pos() {
            let scroll = ui.input(|input| {
                let page = input.content_rect().height();
                input
                    .events
                    .iter()
                    .map(|event| match event {
                        egui::Event::MouseWheel { unit, delta, .. } => match unit {
                            egui::MouseWheelUnit::Point => delta.y,
                            egui::MouseWheelUnit::Line => delta.y * 50.0,
                            egui::MouseWheelUnit::Page => delta.y * page,
                        },
                        _ => 0.0,
                    })
                    .sum::<f32>()
            });
            if scroll != 0.0 {
                let map = ViewMap::new(rect, pixels_per_point, *view, pixels);
                let under = map.to_scene(pointer);
                let zoom = (view.zoom * 1.25f32.powf(scroll / 50.0)).clamp(MIN_ZOOM, MAX_ZOOM);
                let kept = view.zoom / zoom;
                view.center = [0, 1].map(|i| under[i] - (under[i] - view.center[i]) * kept);
                view.zoom = zoom;
            }
        }
        let panning = response.dragged_by(egui::PointerButton::Middle)
            || (response.dragged_by(egui::PointerButton::Primary)
                && ui.input(|input| input.key_down(egui::Key::Space)));
        if panning {
            let delta = response.drag_delta();
            view.center[0] -= delta.x / view.zoom;
            view.center[1] -= delta.y / view.zoom;
        }
        let view = *view;
        let map = ViewMap::new(rect, pixels_per_point, view, pixels);
        self.shown = Some(map);

        let mut action = None;
        let pointer = ui.ctx().pointer_latest_pos();
        // Taking a payload takes it whatever it is, so the model's is taken only if it's one.
        let model_dropped = response.dnd_hover_payload::<ModelPath>().is_some();
        if model_dropped
            && let Some(model) = response.dnd_release_payload::<ModelPath>()
            && let Some(pointer) = pointer
            && let Some((view, at)) = ground_under(document, map.to_scene(pointer))
        {
            action = Some(Action::Model {
                model: model.0.clone(),
                view,
                at,
            });
        }
        if !model_dropped
            && let Some(scene) = response.dnd_release_payload::<ScenePath>()
            && let Some(pointer) = pointer
        {
            let point = map.to_scene(pointer);
            let scene = scene.0.clone();
            action = Some(match ground_under(document, point) {
                Some((view, at)) => Action::Instance3d { scene, view, at },
                None => Action::Instance {
                    scene,
                    at: self.snapped(point),
                },
            });
        }
        let pointer_in = pointer.filter(|pointer| rect.contains(*pointer));
        let dropping = response
            .dnd_hover_payload::<ScenePath>()
            .map(|scene| scene.0.clone())
            .or_else(|| {
                let model = response.dnd_hover_payload::<ModelPath>()?;
                Some(model.0.clone())
            })
            .zip(pointer_in);

        // What the selection is, for tiles: a tilemap, or a layer of one.
        let selected = document
            .selected
            .clone()
            .filter(|_| document.inner().is_none());
        let tilemap = selected.as_ref().and_then(|path| {
            let (map_path, _) = edit::tilemap_at(document.file(), path)?;
            let grid = Grid::of(document, &map_path)?;
            Some((map_path, grid))
        });
        let layer = selected.filter(|path| {
            matches!(
                edit::node(document.file(), path).map(|node| &node.kind),
                Some(KindDef::TileLayer(_))
            )
        });
        let busy = panning
            || self.resize(ui, &response, &map, document, tilemap.as_ref())
            || self.size_node(ui, &response, &map, document)
            || self.size_canvas(ui, &response, &map, canvas_size);
        match (&layer, &tilemap) {
            (Some(layer), Some((_, grid))) if !busy => {
                self.paint(ui, &response, &map, document, layer, *grid, tiles);
            }
            _ => {
                // A stroke left unpainted, by Escape, Delete or undo taking its layer away, mustn't
                // draw from its last cell when one's painted again.
                self.stroke = None;
                if !busy {
                    self.interact(ui, &response, &map, document);
                }
            }
        }

        let canvas = self.canvas.begin(pixels[0], pixels[1]);
        draw(canvas, &map, view, self.step, document, canvas_size);
        let texture = self.canvas.render(ctx);
        let painter = ui.painter_at(rect);
        painter.image(
            texture.id,
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        painter.rect_stroke(
            egui::Rect::from_two_pos(map.to_screen([0.0, 0.0]), map.to_screen(canvas_size)),
            0.0,
            egui::Stroke::new(1.0, theme::CANVAS),
            egui::StrokeKind::Middle,
        );
        // Its corner, to drag to preview other sizes.
        painter.rect_filled(
            egui::Rect::from_center_size(
                map.to_screen(canvas_size),
                egui::vec2(HANDLE_SIZE, HANDLE_SIZE),
            ),
            1.0,
            theme::CANVAS,
        );
        self.overlay(&painter, &map, document);
        if let Some((_, grid)) = &tilemap {
            let hovered = response
                .hover_pos()
                .map(|pos| grid.cell_at(map.to_scene(pos)));
            self.tiles_overlay(
                &painter,
                &map,
                grid,
                hovered,
                layer.is_some(),
                tiles,
                palette,
            );
        }
        if let Some((file, pointer)) = dropping {
            let name = file.rsplit('/').next().unwrap_or(&file);
            let point = map.to_scene(pointer);
            let in_view = ground_under(document, point).is_some();
            let at = if in_view {
                pointer
            } else {
                map.to_screen(self.snapped(point))
            };
            cross(&painter, at, 8.0, egui::Stroke::new(1.5, theme::ACCENT));
            let text = if in_view || !is_model_file(&file) {
                format!("Place {name} here")
            } else {
                format!("Drop {name} in a 3D view")
            };
            // Above the pointer, as the name being dragged is below it.
            let galley =
                painter.layout_no_wrap(text, egui::FontId::proportional(13.0), theme::ACCENT);
            let corner = at + egui::vec2(12.0, -8.0 - galley.size().y);
            painter.rect_filled(
                egui::Rect::from_min_size(corner, galley.size()).expand(3.0),
                3.0,
                egui::Color32::from_black_alpha(200),
            );
            painter.galley(corner, galley, theme::ACCENT);
        }
        // What's posing the scene, so it isn't taken for how the file has it.
        if let Some((clip, _, time)) = document.posing() {
            let galley = painter.layout_no_wrap(
                format!("♦ {clip}  {time:.2} s"),
                egui::FontId::proportional(13.0),
                theme::KEY,
            );
            // At the top right, out of the way of scenes about the origin.
            let at = rect.right_top() + egui::vec2(-8.0 - galley.size().x, 8.0);
            painter.rect_filled(
                egui::Rect::from_min_size(at, galley.size()).expand(4.0),
                3.0,
                egui::Color32::from_black_alpha(200),
            );
            painter.galley(at, galley, theme::KEY);
        }
        if let Some(error) = &document.error {
            let galley = painter.layout(
                error.clone(),
                egui::FontId::monospace(12.0),
                ui.visuals().error_fg_color,
                rect.width() - 16.0,
            );
            let at = egui::pos2(rect.left() + 8.0, rect.bottom() - galley.size().y - 8.0);
            painter.rect_filled(
                egui::Rect::from_min_size(at, galley.size()).expand(4.0),
                3.0,
                egui::Color32::from_black_alpha(200),
            );
            painter.galley(at, galley, egui::Color32::WHITE);
        }
        action
    }

    /// `point` snapped to the grid, if snapping's on.
    fn snapped(&self, point: [f32; 2]) -> [f32; 2] {
        if self.snap {
            point.map(|v| (v / self.step).round() * self.step)
        } else {
            point
        }
    }

    /// Resizing the selected node by the handles on its edges and corners, with the Select tool.
    /// Returns whether the pointer is the resize's, so nothing else takes it.
    fn size_node(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        map: &ViewMap,
        document: &mut Document,
    ) -> bool {
        let primary = egui::PointerButton::Primary;
        let handles = node_handles(document).filter(|_| self.tool == Tool::Select);
        let under = |at: egui::Pos2| {
            let (path, handles) = handles.as_ref()?;
            let (edges, _) = handles
                .iter()
                .find(|(_, handle)| map.to_screen(*handle).distance(at) <= HANDLE_SIZE)?;
            Some((path.clone(), *edges))
        };
        if let Some((_, edges)) = response.hover_pos().and_then(under) {
            ui.ctx().set_cursor_icon(resize_cursor(edges));
        }
        // A drag, not a click, so clicking in place still reaches the nodes under it.
        if self.sizing.is_none()
            && response.drag_started_by(primary)
            && let Some(origin) = ui.input(|input| input.pointer.press_origin())
            && let Some((path, edges)) = under(origin)
        {
            self.sizing = start_sizing(document, path, edges);
        }
        let Some(sizing) = &self.sizing else {
            return false;
        };
        ui.ctx().set_cursor_icon(resize_cursor(sizing.edges));
        if response.dragged_by(primary)
            && let Some(pointer) = response.interact_pointer_pos()
        {
            size_to(document, sizing, self.snapped(map.to_scene(pointer)));
        }
        if !response.is_pointer_button_down_on() {
            self.sizing = None;
        }
        true
    }

    /// Previewing the scene on a canvas of another size, by dragging the canvas's bottom-right
    /// corner, `corner`. Returns whether the pointer is the drag's.
    fn size_canvas(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        map: &ViewMap,
        corner: [f32; 2],
    ) -> bool {
        let primary = egui::PointerButton::Primary;
        let on_corner = |at: egui::Pos2| map.to_screen(corner).distance(at) <= HANDLE_SIZE;
        if response.hover_pos().is_some_and(on_corner) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
        }
        if !self.sizing_canvas
            && response.drag_started_by(primary)
            && ui
                .input(|input| input.pointer.press_origin())
                .is_some_and(on_corner)
        {
            self.sizing_canvas = true;
        }
        if !self.sizing_canvas {
            return false;
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
        if response.dragged_by(primary)
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let point = self.snapped(map.to_scene(pointer));
            self.preview = Some(point.map(|v| v.round().max(MIN_PREVIEW)));
        }
        if !response.is_pointer_button_down_on() {
            self.sizing_canvas = false;
        }
        true
    }

    /// Resizing the selected tilemap, `tilemap`, by its handles. Returns whether the pointer is
    /// the resize's, so nothing else takes it.
    fn resize(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        map: &ViewMap,
        document: &mut Document,
        tilemap: Option<&(NodePath, Grid)>,
    ) -> bool {
        let primary = egui::PointerButton::Primary;
        let origin = ui.input(|input| input.pointer.press_origin());
        let on_handle = |grid: &Grid, at: egui::Pos2| {
            grid.handles()
                .into_iter()
                .find(|(_, handle)| map.to_screen(*handle).distance(at) <= HANDLE_SIZE)
                .map(|(axes, _)| axes)
        };
        if let (Some(pointer), Some((_, grid))) = (response.hover_pos(), tilemap)
            && let Some([across, down]) = on_handle(grid, pointer)
        {
            ui.ctx()
                .set_cursor_icon(resize_cursor([across, down].map(i8::from)));
        }
        if self.resize.is_none()
            && response.drag_started_by(primary)
            && let (Some((path, grid)), Some(origin)) = (tilemap, origin)
            && let Some(axes) = on_handle(grid, origin)
        {
            self.resize = Some(Resize {
                path: path.clone(),
                axes,
            });
        }
        let Some(resize) = &self.resize else {
            return false;
        };
        if let (Some(pointer), Some((path, grid))) = (response.interact_pointer_pos(), tilemap)
            && *path == resize.path
            && response.dragged_by(primary)
            && let Some((_, def)) = edit::tilemap_at(document.file(), path)
        {
            let point = map.to_scene(pointer);
            let mut size = [def.columns, def.rows];
            for i in 0..2 {
                if resize.axes[i] {
                    let cells = ((point[i] - grid.origin[i]) / grid.cell[i]).round();
                    size[i] = (cells.max(1.0) as u32).min(MAX_CELLS);
                }
            }
            if size != [def.columns, def.rows] {
                edit::resize_tilemap(document.edit(), path, size[0], size[1]);
            }
        }
        if !response.is_pointer_button_down_on() {
            self.resize = None;
        }
        true
    }

    /// Painting the tile layer at `layer`, on the map `grid`, by the Tiles panel's tool: the
    /// right button empties cells, and Control picks the tile in a cell.
    #[expect(clippy::too_many_arguments)]
    fn paint(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        map: &ViewMap,
        document: &mut Document,
        layer: &NodePath,
        grid: Grid,
        tiles: &mut Tiles,
    ) {
        // A stroke on another layer, selected since, ends.
        if self
            .stroke
            .as_ref()
            .is_some_and(|stroke| stroke.layer != *layer)
        {
            self.stroke = None;
        }
        let (primary, secondary) = (egui::PointerButton::Primary, egui::PointerButton::Secondary);
        let (down, pressed, erasing, control) = ui.input(|input| {
            let pointer = &input.pointer;
            (
                pointer.button_down(primary) || pointer.button_down(secondary),
                pointer.button_pressed(primary) || pointer.button_pressed(secondary),
                pointer.button_down(secondary),
                input.modifiers.command,
            )
        });
        let down = down && response.is_pointer_button_down_on();
        let cell = response
            .interact_pointer_pos()
            .map(|pointer| grid.cell_at(map.to_scene(pointer)));
        // A press on one of the map's handles is for resizing it.
        let on_handle = response.interact_pointer_pos().is_some_and(|pointer| {
            grid.handles()
                .iter()
                .any(|(_, handle)| map.to_screen(*handle).distance(pointer) <= HANDLE_SIZE)
        });
        if let Some(cell) = cell
            && down
            && pressed
            && !on_handle
            && self.stroke.is_none()
        {
            let def = match &edit::node(document.file(), layer).map(|node| &node.kind) {
                Some(KindDef::TileLayer(def)) => def.clone(),
                _ => return,
            };
            if control || tiles.tool == TileTool::Pick {
                if grid.contains(cell) {
                    match edit::cell(&def, cell) {
                        Some(tile) => tiles.choose(tile.to_owned()),
                        None => {
                            tiles.erase = true;
                            if tiles.tool == TileTool::Pick {
                                tiles.tool = TileTool::Paint;
                            }
                        }
                    }
                }
                return;
            }
            let tile = if erasing || tiles.erase {
                None
            } else {
                match &tiles.tile {
                    Some(tile) => Some(tile.clone()),
                    // Nothing to paint with yet.
                    None => return,
                }
            };
            match tiles.tool {
                TileTool::Fill => {
                    let cells = edit::flood(&def, cell, grid.size);
                    paint_cells(document, layer, grid, cells, tile.as_deref());
                }
                tool => {
                    if tool == TileTool::Paint {
                        paint_cells(document, layer, grid, vec![cell], tile.as_deref());
                    }
                    self.stroke = Some(Stroke {
                        layer: layer.clone(),
                        tool,
                        tile,
                        from: cell,
                        last: cell,
                    });
                }
            }
            return;
        }
        let Some(stroke) = &mut self.stroke else {
            return;
        };
        if down && let Some(cell) = cell {
            if stroke.tool == TileTool::Paint && cell != stroke.last {
                let cells = edit::line(stroke.last, cell);
                paint_cells(document, layer, grid, cells, stroke.tile.as_deref());
            }
            stroke.last = cell;
        }
        if !down {
            let stroke = self.stroke.take().expect("there's a stroke");
            if stroke.tool == TileTool::Rect {
                let cells = edit::rect_cells(stroke.from, stroke.last);
                paint_cells(document, layer, grid, cells, stroke.tile.as_deref());
            }
        }
    }

    /// Over a tilemap: its cells, the handles that resize it, and when painting one of its layers,
    /// the cell the pointer's over, `hovered`, with the tile it would get, or the rectangle being
    /// dragged out.
    #[expect(clippy::too_many_arguments)]
    fn tiles_overlay(
        &self,
        painter: &egui::Painter,
        map: &ViewMap,
        grid: &Grid,
        hovered: Option<[i32; 2]>,
        painting: bool,
        tiles: &Tiles,
        palette: Option<&Palette>,
    ) {
        let [right, bottom] = grid.corner(grid.size.map(|n| n as i32));
        let faint = egui::Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.25));
        // Lines closer than a few points would be a haze.
        let spacing = map.to_screen(grid.corner([1, 1])) - map.to_screen(grid.origin);
        if painting && spacing.x.min(spacing.y) >= 6.0 {
            for column in 1..grid.size[0] as i32 {
                let [x, _] = grid.corner([column, 0]);
                painter.line_segment(
                    [
                        map.to_screen([x, grid.origin[1]]),
                        map.to_screen([x, bottom]),
                    ],
                    faint,
                );
            }
            for row in 1..grid.size[1] as i32 {
                let [_, y] = grid.corner([0, row]);
                painter.line_segment(
                    [
                        map.to_screen([grid.origin[0], y]),
                        map.to_screen([right, y]),
                    ],
                    faint,
                );
            }
        }
        for (_, handle) in grid.handles() {
            let at = map.to_screen(handle);
            painter.rect_filled(
                egui::Rect::from_center_size(at, egui::vec2(HANDLE_SIZE, HANDLE_SIZE)),
                1.0,
                theme::ACCENT,
            );
        }
        if !painting {
            return;
        }
        let cells_rect = |a: [i32; 2], b: [i32; 2]| {
            let min = [a[0].min(b[0]), a[1].min(b[1])];
            let max = [a[0].max(b[0]) + 1, a[1].max(b[1]) + 1];
            egui::Rect::from_min_max(
                map.to_screen(grid.corner(min)),
                map.to_screen(grid.corner(max)),
            )
        };
        let stroke = egui::Stroke::new(1.5, theme::ACCENT);
        if let Some(Stroke {
            tool: TileTool::Rect,
            from,
            last,
            ..
        }) = &self.stroke
        {
            painter.rect_stroke(
                cells_rect(*from, *last),
                0.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            return;
        }
        let Some(cell) = hovered.filter(|cell| grid.contains(*cell)) else {
            return;
        };
        let rect = cells_rect(cell, cell);
        let tile = tiles
            .tile
            .as_deref()
            .filter(|_| !tiles.erase && tiles.tool != TileTool::Pick)
            .and_then(|tile| palette?.tile(tile));
        if let Some(tile) = tile {
            painter.image(
                tile.texture.id,
                rect,
                tile.uv,
                tile.tint.gamma_multiply(0.6),
            );
        }
        painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
    }

    /// Selecting with clicks, and changing the selected node with drags.
    fn interact(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        map: &ViewMap,
        document: &mut Document,
    ) {
        let primary = egui::PointerButton::Primary;
        if response.clicked_by(primary)
            && let Some(pointer) = response.interact_pointer_pos()
        {
            // Clicking again in the same place selects the next node under the pointer, so nodes
            // under others can be reached.
            let again = self
                .last_click
                .is_some_and(|last| last.distance(pointer) <= PICK_DISTANCE);
            let under = under(document, map, pointer);
            let next = document
                .selected
                .as_ref()
                .filter(|_| again)
                .and_then(|selected| under.iter().position(|path| path == selected))
                .map_or(0, |i| (i + 1) % under.len());
            document.select(under.into_iter().nth(next));
            self.last_click = Some(pointer);
        }
        if response.drag_started_by(primary)
            && let Some(origin) = ui.input(|input| input.pointer.press_origin())
        {
            let under = under(document, map, origin);
            // The selected node if the drag starts on it, else the one on top.
            let picked = document
                .selected
                .clone()
                .filter(|selected| under.contains(selected))
                .or_else(|| under.into_iter().next());
            let path = match self.tool {
                // Dragging whatever's under the pointer, selecting it.
                Tool::Select => picked,
                // Changing what's selected, wherever the drag starts.
                _ => document
                    .selected
                    .clone()
                    .filter(|path| !path.is_empty())
                    .or(picked),
            };
            let tool = match self.tool {
                Tool::Select => Tool::Move,
                tool => tool,
            };
            self.drag = path.and_then(|path| {
                document.select(Some(path.clone()));
                start_drag(document, tool, path, map.to_scene(origin))
            });
        }
        if response.dragged_by(primary)
            && let (Some(drag), Some(pointer)) = (&self.drag, response.interact_pointer_pos())
        {
            let shift = ui.input(|input| input.modifiers.shift);
            self.drag_to(document, drag, map.to_scene(pointer), shift);
        }
        if response.drag_stopped() {
            self.drag = None;
        }
    }

    /// Changes the dragged node for the pointer at `now`, in the scene.
    fn drag_to(&self, document: &mut Document, drag: &Drag, now: [f32; 2], shift: bool) {
        let Drag {
            tool,
            path,
            start,
            from,
            to_parent,
            mirrored,
            origin,
            to_node,
            space,
        } = drag;
        let from = *from;
        let snap = |value: f32, step: f32| {
            if self.snap {
                (value / step).round() * step
            } else {
                value
            }
        };
        let mut node = start.clone();
        match tool {
            Tool::Select | Tool::Move if space.is_some() => {
                let space = space.as_ref().expect("the node is placed in 3D");
                let (Some([dx, dy, dz]), Some(placement)) =
                    (space.moved(now, shift), &mut node.placement3)
                else {
                    return;
                };
                node.x = start.x + dx;
                node.y = start.y + dy;
                placement.z += dz;
            }
            Tool::Select | Tool::Move => {
                let moved = to_parent.apply([now[0] - from[0], now[1] - from[1]]);
                node.x = snap(start.x + moved[0], self.step);
                node.y = snap(start.y + moved[1], self.step);
            }
            Tool::Rotate => {
                let angle = |[x, y]: [f32; 2]| (y - origin[1]).atan2(x - origin[0]);
                let mut turned = angle(now) - angle(from);
                if *mirrored {
                    turned = -turned;
                }
                let rotation = start.rotation + turned;
                node.rotation = if self.snap || shift {
                    (rotation / ANGLE_STEP).round() * ANGLE_STEP
                } else {
                    rotation
                };
            }
            Tool::Scale => {
                let Some(to_node) = to_node else {
                    return;
                };
                let [ax, ay] = to_node.apply(from);
                let [bx, by] = to_node.apply(now);
                let ratio = |a: f32, b: f32| if a.abs() > 1e-3 { b / a } else { 1.0 };
                let [rx, ry] = if shift {
                    let r = (bx.hypot(by) / ax.hypot(ay).max(1e-3)).max(0.0);
                    [r, r]
                } else {
                    [ratio(ax, bx), ratio(ay, by)]
                };
                node.scale_x = snap(start.scale_x * rx, SCALE_STEP);
                node.scale_y = snap(start.scale_y * ry, SCALE_STEP);
            }
        }
        // Only its transform, as it's shown, keyed if a clip posing the preview moves it.
        let Some(mut target) = document.shown_node(path) else {
            return;
        };
        target.x = node.x;
        target.y = node.y;
        target.rotation = node.rotation;
        target.scale_x = node.scale_x;
        target.scale_y = node.scale_y;
        if let (Some(shown), Some(moved)) = (&mut target.placement3, node.placement3) {
            shown.z = moved.z;
        }
        document.set_node(path, target);
    }

    /// The selected node's outline and origin, and the handle of the tool.
    fn overlay(&self, painter: &egui::Painter, map: &ViewMap, document: &Document) {
        let (Some(path), Some(scene)) = (&document.selected, &document.preview) else {
            return;
        };
        if path.is_empty() {
            return;
        }
        let Some(id) = document.selected_live() else {
            return;
        };
        let stroke = egui::Stroke::new(1.5, theme::ACCENT);
        if let Some(view) = scene.view_of(id) {
            self.overlay_3d(painter, map, document, scene, view, id);
            return;
        }
        let transform = scene.transform(id);
        if let Some((placement, rect)) = scene.placement(id) {
            let corners = placement
                .corners(rect)
                .map(|corner| map.to_screen(corner))
                .to_vec();
            painter.add(egui::Shape::closed_line(corners, stroke));
        }
        let origin = map.to_screen(transform.translation);
        cross(painter, origin, 6.0, stroke);
        // Tools change the nodes of the file, not those instances place.
        if document.inner().is_some() {
            return;
        }
        match self.tool {
            Tool::Select => {
                let handles = node_handles(document).map(|(_, handles)| handles);
                for (_, handle) in handles.into_iter().flatten() {
                    let at = egui::Rect::from_center_size(
                        map.to_screen(handle),
                        egui::vec2(HANDLE_SIZE, HANDLE_SIZE),
                    );
                    painter.rect(
                        at,
                        1.0,
                        egui::Color32::WHITE,
                        egui::Stroke::new(1.0, theme::ACCENT),
                        egui::StrokeKind::Middle,
                    );
                }
            }
            Tool::Move => {
                for (axis, color) in [
                    ([1.0, 0.0], egui::Color32::from_rgb(0xf0, 0x60, 0x70)),
                    ([0.0, 1.0], egui::Color32::from_rgb(0x80, 0xe0, 0x70)),
                ] {
                    painter.arrow(
                        origin,
                        egui::vec2(axis[0], axis[1]) * 28.0,
                        egui::Stroke::new(2.0, color),
                    );
                }
            }
            Tool::Rotate => {
                painter.circle_stroke(origin, 24.0, stroke);
            }
            Tool::Scale => {
                for axis in [transform.x_axis, transform.y_axis] {
                    let direction = egui::vec2(axis[0], axis[1]).normalized();
                    if !direction.is_finite() {
                        continue;
                    }
                    let end = origin + direction * 28.0;
                    painter.line_segment([origin, end], stroke);
                    painter.rect_filled(
                        egui::Rect::from_center_size(end, egui::vec2(6.0, 6.0)),
                        0.0,
                        theme::ACCENT,
                    );
                }
            }
        }
    }
}

impl Viewport {
    /// The outline of the box around what `id`, a node in the 3D view `view`, shows, as its camera
    /// shows it, its origin, and with the Move tool, the world's axes from it.
    fn overlay_3d(
        &self,
        painter: &egui::Painter,
        map: &ViewMap,
        document: &Document,
        scene: &Scene,
        view: NodeId,
        id: NodeId,
    ) {
        let stroke = egui::Stroke::new(1.5, theme::ACCENT);
        let shown = |point: [f32; 3]| scene.project(view, point).map(|at| map.to_screen(at));
        if let Some(bounds) = scene.bounds_3d(id) {
            let corner = |i: usize| {
                [0, 1, 2].map(|axis| {
                    if i >> axis & 1 == 0 {
                        bounds.min[axis]
                    } else {
                        bounds.max[axis]
                    }
                })
            };
            for a in 0..8 {
                for axis in 0..3 {
                    let b = a | 1 << axis;
                    if b == a {
                        continue;
                    }
                    if let (Some(from), Some(to)) = (shown(corner(a)), shown(corner(b))) {
                        painter.line_segment([from, to], stroke);
                    }
                }
            }
        }
        let Some(transform) = scene.transform_3d(id) else {
            return;
        };
        let origin = transform.translation;
        let Some(at) = shown(origin) else {
            return;
        };
        cross(painter, at, 6.0, stroke);
        if document.inner().is_some() || !matches!(self.tool, Tool::Move) {
            return;
        }
        for (axis, color) in [
            ([1.0, 0.0, 0.0], egui::Color32::from_rgb(0xf0, 0x60, 0x70)),
            ([0.0, 1.0, 0.0], egui::Color32::from_rgb(0x80, 0xe0, 0x70)),
            ([0.0, 0.0, 1.0], egui::Color32::from_rgb(0x70, 0x90, 0xf0)),
        ] {
            let end = [0, 1, 2].map(|i| origin[i] + axis[i] * 0.5);
            let Some(end) = shown(end) else {
                continue;
            };
            let direction = (end - at).normalized();
            if direction.is_finite() {
                painter.arrow(at, direction * 28.0, egui::Stroke::new(2.0, color));
            }
        }
    }
}

/// A view showing the whole of a canvas `canvas_size` in size in a viewport `size` points in
/// size, with a margin around it, at a whole number of points per unit if it's at least 1.
fn frame(canvas_size: [f32; 2], size: egui::Vec2) -> View {
    const MARGIN: f32 = 16.0;
    let fit = ((size.x - MARGIN) / canvas_size[0].max(1.0))
        .min((size.y - MARGIN) / canvas_size[1].max(1.0))
        .max(MIN_ZOOM);
    let zoom = if fit >= 1.0 { fit.floor() } else { fit };
    View {
        center: [canvas_size[0] / 2.0, canvas_size[1] / 2.0],
        zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
    }
}

/// The grid, the axes and the scene, on the viewport's canvas.
fn draw(
    canvas: &mut Canvas,
    map: &ViewMap,
    view: View,
    step: f32,
    document: &mut Document,
    canvas_size: [f32; 2],
) {
    canvas.clear(theme::VIEWPORT);
    canvas.set_camera(Some(map.camera));
    let visible = canvas.visible_area();
    let line = 1.0 / map.scale;
    // Lines closer than a few points would be a haze.
    if step * view.zoom >= 6.0 {
        let first = (visible.x / step).floor() as i64;
        let last = ((visible.x + visible.w) / step).ceil() as i64;
        for i in first..=last {
            let x = i as f32 * step;
            canvas.fill_rect(Rect::new(x, visible.y, line, visible.h), theme::GRID);
        }
        let first = (visible.y / step).floor() as i64;
        let last = ((visible.y + visible.h) / step).ceil() as i64;
        for i in first..=last {
            let y = i as f32 * step;
            canvas.fill_rect(Rect::new(visible.x, y, visible.w, line), theme::GRID);
        }
    }
    canvas.fill_rect(Rect::new(visible.x, 0.0, visible.w, line), theme::X_AXIS);
    canvas.fill_rect(Rect::new(0.0, visible.y, line, visible.h), theme::Y_AXIS);
    if let Some(scene) = &mut document.preview {
        scene.draw_with_size(canvas, canvas_size);
    }
}

/// Whether `id` and every node it's inside are visible.
fn shown(scene: &Scene, id: NodeId) -> bool {
    let mut at = Some(id);
    while let Some(id) = at {
        let node = scene.node(id);
        if !node.is_visible() {
            return false;
        }
        at = node.parent();
    }
    true
}

/// What a click picks first, of what's under it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pickable {
    /// Sprites, rects and text: what can be seen.
    Seen,
    /// Areas, which don't draw, and often cover what does.
    Area,
    /// Tilemaps and their layers, which cover everything on them.
    Tiles,
}

/// The nodes of the file under `pos`, in points, in the order a click picks them: what can be
/// seen, topmost first, a 3D view's nodes whose boxes are under it, nearest first, before the
/// view, then areas, then plain nodes with their origins nearby, then tilemaps.
fn under(document: &Document, map: &ViewMap, pos: egui::Pos2) -> Vec<NodePath> {
    let Some(scene) = document.preview.as_ref() else {
        return Vec::new();
    };
    let point = map.to_scene(pos);
    let root = scene.root();
    let mut nodes = scene.descendants(root);
    nodes.retain(|&id| id != root && shown(scene, id));
    nodes.reverse();
    let pickable = |id: NodeId| match scene.node(id).kind() {
        Kind::Tilemap(_) | Kind::TileLayer(_) => Pickable::Tiles,
        Kind::Area(_) | Kind::Stack(_) | Kind::Particles(_) => Pickable::Area,
        _ => Pickable::Seen,
    };
    let covering = |which: Pickable| {
        nodes
            .iter()
            .copied()
            .filter(move |&id| pickable(id) == which && scene.contains(id, point))
    };
    // A view's nodes, picked by their boxes, come before the view.
    let seen = covering(Pickable::Seen).flat_map(|id| {
        let inside = match scene.node(id).kind() {
            Kind::View3d(_) => scene.pick(id, point),
            _ => Vec::new(),
        };
        inside.into_iter().chain([id])
    });
    // Plain nodes, and particles sent out from a point, by where their origins show, in 3D views
    // as their cameras show them.
    let near_origin = nodes.iter().copied().filter(|&id| {
        let node = scene.node(id);
        let point = |rect: Rect| {
            rect.w == 0.0 && rect.h == 0.0 && matches!(node.kind(), Kind::Particles(_))
        };
        let origin = match node.placement3() {
            Some(_) if matches!(node.kind(), Kind::Node) => origin_3d(scene, id),
            Some(_) => None,
            None => scene
                .rect(id)
                .is_none_or(point)
                .then(|| scene.transform(id).translation),
        };
        origin.is_some_and(|origin| map.to_screen(origin).distance(pos) <= PICK_DISTANCE)
    });
    let mut found: Vec<NodePath> = Vec::new();
    let ids = seen
        .chain(covering(Pickable::Area))
        .chain(near_origin)
        .chain(covering(Pickable::Tiles));
    for id in ids {
        // Nodes an instance placed are its, so it may come up more than once.
        if let Some(path) = document.file_node(id)
            && !path.is_empty()
            && !found.contains(&path)
        {
            found.push(path);
        }
    }
    found
}

/// Puts `tile`, or nothing, in `cells` of the tile layer at `layer`, on the map `grid`, in the file
/// and the preview alike, so painting doesn't build the scene again with every cell.
fn paint_cells(
    document: &mut Document,
    layer: &[usize],
    grid: Grid,
    mut cells: Vec<[i32; 2]>,
    tile: Option<&str>,
) {
    cells.retain(|&cell| grid.contains(cell));
    let live = document.live_node(layer);
    let (file, preview) = document.edit_with_preview();
    let Some(def) = edit::tile_layer_mut(file, layer) else {
        return;
    };
    if !edit::set_cells(def, cells.iter().copied(), tile) {
        return;
    }
    let patched = match (preview, live) {
        (Some(scene), Some(id)) => cells
            .iter()
            .all(|&cell| scene.set_tile(id, cell, tile).is_ok()),
        _ => false,
    };
    if !patched {
        document.rebuild();
    }
}

/// A button for `preset`, drawn as a node in its parent where the preset puts it, named for
/// screen readers and tests by the preset's name.
fn preset_button(ui: &mut egui::Ui, preset: &Preset, enabled: bool) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, preset.name));
    let visuals = ui.style().interact(&response);
    let painter = ui.painter();
    painter.rect(
        rect,
        3.0,
        visuals.weak_bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let parent = rect.shrink(6.0);
    painter.rect_stroke(
        parent,
        0.0,
        egui::Stroke::new(1.0, visuals.fg_stroke.color.gamma_multiply(0.5)),
        egui::StrokeKind::Middle,
    );
    // The node: a small square, or stretched across.
    let span = |place: Place, start: f32, end: f32| match place {
        Place::At(fraction) => {
            let from = start + (end - start - 6.0) * fraction;
            (from, from + 6.0)
        }
        Place::Wide => (start, end),
    };
    let (left, right) = span(preset.places[0], parent.left(), parent.right());
    let (top, bottom) = span(preset.places[1], parent.top(), parent.bottom());
    let color = if enabled {
        theme::ACCENT
    } else {
        visuals.fg_stroke.color.gamma_multiply(0.3)
    };
    painter.rect_filled(
        egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom)),
        1.0,
        color,
    );
    let response = response.on_hover_text(preset.name);
    if enabled {
        response
    } else {
        response
            .on_disabled_hover_text("Only sprites, rects, areas, stacks, panels and views stretch")
    }
}

/// A handle that resizes a node: the edges it moves, as [`Sizing::edges`], and where it is in the
/// scene.
type Handle = ([i8; 2], [f32; 2]);

/// The handles that resize the selected node, if it's a sprite, rect or area of the file, with its
/// path.
fn node_handles(document: &Document) -> Option<(NodePath, Vec<Handle>)> {
    let path = document
        .selected
        .clone()
        .filter(|path| !path.is_empty() && document.inner().is_none())?;
    let node = edit::node(document.file(), &path)?;
    if !matches!(
        node.kind,
        KindDef::Sprite(_)
            | KindDef::Rect(_)
            | KindDef::Area(_)
            | KindDef::Stack(_)
            | KindDef::Panel(_)
            | KindDef::View3d(_)
            | KindDef::Particles(_)
    ) {
        return None;
    }
    let scene = document.preview.as_ref()?;
    let id = document.live_node(&path)?;
    let rect = scene.rect(id)?;
    let transform = scene.transform(id);
    let mut handles = Vec::with_capacity(8);
    for down in [-1, 0, 1] {
        for across in [-1, 0, 1] {
            if [across, down] == [0, 0] {
                continue;
            }
            let at = |edge: i8, start: f32, size: f32| start + size * (f32::from(edge) + 1.0) / 2.0;
            let point = [at(across, rect.x, rect.w), at(down, rect.y, rect.h)];
            handles.push(([across, down], transform.apply(point)));
        }
    }
    Some((path, handles))
}

/// A resize of the node at `path` by the handle on `edges`, from how it is now.
fn start_sizing(document: &Document, path: NodePath, edges: [i8; 2]) -> Option<Sizing> {
    let start = document.shown_node(&path)?;
    let scene = document.preview.as_ref()?;
    let id = document.live_node(&path)?;
    let transform = scene.transform(id);
    let parent = scene
        .node(id)
        .parent()
        .map_or(Affine::IDENTITY, |parent| scene.transform(parent));
    let local = parent.inverse()? * transform;
    Some(Sizing {
        path,
        start,
        edges,
        own: scene.size_of(id)?,
        rect: scene.rect(id)?,
        to_node: transform.inverse()?,
        to_parent: Affine {
            translation: [0.0, 0.0],
            ..local
        },
    })
}

/// Resizes the node `sizing` is resizing for the pointer at `point`, in the scene: the edges it
/// moves follow the pointer, and the others stay where they were.
fn size_to(document: &mut Document, sizing: &Sizing, point: [f32; 2]) {
    let pointer = sizing.to_node.apply(point);
    let rect = [
        [sizing.rect.x, sizing.rect.w],
        [sizing.rect.y, sizing.rect.h],
    ];
    let mut own = sizing.own;
    // How far its start edges move, in its own coordinates, which moves its origin with them.
    let mut moved = [0.0; 2];
    for i in 0..2 {
        let [start, size] = rect[i];
        match sizing.edges[i] {
            1 => own[i] += (pointer[i] - (start + size)).max(1.0 - size),
            -1 => {
                let by = (pointer[i] - start).min(size - 1.0);
                own[i] -= by;
                moved[i] = by;
            }
            _ => {}
        }
    }
    let mut node = sizing.start.clone();
    if let Some(stack) = node.kind.stack_mut() {
        // A stack's pivot is from the point its anchor names, which moves as it grows, so its
        // origin makes up the difference. It has a size of its own along the axes it's resized.
        let anchor = node.anchor.unwrap_or([0.0, 0.0]);
        for i in 0..2 {
            if sizing.edges[i] != 0 {
                moved[i] += (anchor[i] * own[i]).floor() - (anchor[i] * sizing.own[i]).floor();
                stack.size[i] = Some(own[i]);
            }
        }
    }
    let [dx, dy] = sizing.to_parent.apply(moved);
    node.x += dx;
    node.y += dy;
    match &mut node.kind {
        KindDef::Sprite(sprite) => sprite.size = Some(own),
        KindDef::Rect(rect) => rect.size = own,
        KindDef::Area(area) => area.size = own,
        KindDef::View3d(view) => view.size = own,
        KindDef::Particles(particles) => particles.size = own.map(|side| side.max(0.0)),
        KindDef::Stack(_) | KindDef::Panel(_) => {}
        _ => return,
    }
    // Keyed, if a clip posing the preview moves it.
    document.set_node(&sizing.path, node);
}

/// The cursor for resizing by a handle on `edges`.
fn resize_cursor(edges: [i8; 2]) -> egui::CursorIcon {
    match edges {
        [0, _] => egui::CursorIcon::ResizeVertical,
        [_, 0] => egui::CursorIcon::ResizeHorizontal,
        [across, down] if across == down => egui::CursorIcon::ResizeNwSe,
        _ => egui::CursorIcon::ResizeNeSw,
    }
}

/// A cross centered on `at`, its arms `arm` long.
fn cross(painter: &egui::Painter, at: egui::Pos2, arm: f32, stroke: egui::Stroke) {
    painter.line_segment(
        [at - egui::vec2(arm, 0.0), at + egui::vec2(arm, 0.0)],
        stroke,
    );
    painter.line_segment(
        [at - egui::vec2(0.0, arm), at + egui::vec2(0.0, arm)],
        stroke,
    );
}

/// A drag changing the node at `path` with `tool`, from how it is now, with the pointer at
/// `from` in the scene.
fn start_drag(document: &Document, tool: Tool, path: NodePath, from: [f32; 2]) -> Option<Drag> {
    let start = document.shown_node(&path)?;
    let scene = document.preview.as_ref()?;
    let id = document.live_node(&path)?;
    let parent = scene
        .node(id)
        .parent()
        .map_or(Affine::IDENTITY, |parent| scene.transform(parent));
    let directions = Affine {
        translation: [0.0, 0.0],
        ..parent
    };
    let [a, b] = parent.x_axis;
    let [c, d] = parent.y_axis;
    let transform = scene.transform(id);
    let space = match scene.view_of(id) {
        // Nodes in 3D views only move: the Inspector turns and stretches them.
        Some(_) if !matches!(tool, Tool::Select | Tool::Move) => return None,
        Some(view) => Some(start_drag_3d(scene, view, id, from)?),
        None => None,
    };
    Some(Drag {
        tool,
        path,
        start,
        from,
        to_parent: directions.inverse().unwrap_or(Affine::IDENTITY),
        mirrored: a * d - b * c < 0.0,
        origin: transform.translation,
        to_node: transform.inverse(),
        space,
    })
}

/// Moving `id`, a node in the 3D view `view`, with the pointer at `from` in the scene.
fn start_drag_3d(scene: &Scene, view: NodeId, id: NodeId, from: [f32; 2]) -> Option<Drag3d> {
    let view = scene.view(view)?;
    let origin = scene.transform_3d(id)?.translation;
    let parent = scene
        .node(id)
        .parent()
        .and_then(|parent| scene.transform_3d(parent))
        .unwrap_or(Affine3::IDENTITY);
    let mut drag = Drag3d {
        view,
        origin,
        facing: [0.0, 0.0, 1.0],
        across: None,
        upright: None,
        to_parent: Affine3 {
            translation: [0.0; 3],
            ..parent
        }
        .inverse()
        .unwrap_or(Affine3::IDENTITY),
    };
    let ray = drag.view.ray(from);
    // Upright, facing back along the ray, across the ground.
    let [x, _, z] = ray.direction;
    let length = x.hypot(z);
    if length > 1e-6 {
        drag.facing = [-x / length, 0.0, -z / length];
    }
    drag.across = ray.hit_plane(origin, [0.0, 1.0, 0.0]);
    drag.upright = ray.hit_plane(origin, drag.facing);
    Some(drag)
}

/// The 3D view of the file under `point`, in the scene, topmost first, and where the ray under it
/// meets the ground in the view's world, or if it doesn't, where it's as far from the camera as
/// what the camera looks at.
fn ground_under(document: &Document, point: [f32; 2]) -> Option<(NodePath, [f32; 3])> {
    let scene = document.preview.as_ref()?;
    let root = scene.root();
    let mut nodes = scene.descendants(root);
    nodes.reverse();
    nodes.into_iter().find_map(|id| {
        let Kind::View3d(view) = scene.node(id).kind() else {
            return None;
        };
        if !shown(scene, id) {
            return None;
        }
        let ray = scene.ray(id, point)?;
        let path = document.file_node(id)?;
        // Only a view of the file itself, not one an instance places.
        if !matches!(
            edit::node(document.file(), &path).map(|node| &node.kind),
            Some(KindDef::View3d(_))
        ) {
            return None;
        }
        let camera = view.camera;
        let distance = camera
            .position
            .iter()
            .zip(camera.target)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>()
            .sqrt();
        let at = ray
            .hit_plane([0.0; 3], [0.0, 1.0, 0.0])
            .unwrap_or_else(|| ray.at(distance));
        Some((path, at))
    })
}

/// Where the origin of `id`, a node in a 3D view, shows in the scene, if it's in front of the
/// view's camera.
fn origin_3d(scene: &Scene, id: NodeId) -> Option<[f32; 2]> {
    let origin = scene.transform_3d(id)?.translation;
    scene.project(scene.view_of(id)?, origin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_maps_both_ways() {
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(400.0, 300.0));
        let view = View {
            center: [160.0, 90.0],
            zoom: 2.0,
        };
        let map = ViewMap::new(rect, 1.5, view, [600, 450]);
        // The center is in the middle.
        let middle = map.to_screen([160.0, 90.0]);
        assert!((middle - rect.center()).length() < 1.0);
        let back = map.to_scene(map.to_screen([12.5, -3.0]));
        assert!((back[0] - 12.5).abs() < 1e-3 && (back[1] + 3.0).abs() < 1e-3);
    }

    #[test]
    fn framing_shows_the_whole_canvas() {
        let view = frame([320.0, 180.0], egui::vec2(1000.0, 600.0));
        assert_eq!(view.center, [160.0, 90.0]);
        // 984 / 320 = 3.08, down to 3.
        assert_eq!(view.zoom, 3.0);
        // Twice its size just fits, with the margin.
        let view = frame([320.0, 180.0], egui::vec2(656.0, 400.0));
        assert_eq!(view.zoom, 2.0);
        let view = frame([1280.0, 720.0], egui::vec2(656.0, 376.0));
        assert!((view.zoom - 0.5).abs() < 1e-6);
    }
}
