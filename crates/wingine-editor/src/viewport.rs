//! The 2D viewport: the open scene, drawn by the engine as the game draws it, panned with the
//! middle button and zoomed with the wheel, over a grid and with the game's canvas outlined.
//! Clicking selects the node under the pointer, and dragging moves, turns or stretches the
//! selected node, by the tool chosen.

use std::f32::consts::PI;

use wingine::{
    Context,
    graphics::{Camera, Canvas},
    gui::{GuiCanvas, egui},
    math::{Affine, Rect},
    scene::{Kind, NodeDef, NodeId, Scene},
};

use crate::{
    document::{Document, View},
    edit::{self, NodePath},
    theme,
};

/// How far from a plain node's origin, in points, a click still picks it.
const PICK_DISTANCE: f32 = 6.0;
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
    /// How the scene mapped to the viewport when it was last shown.
    shown: Option<ViewMap>,
    /// Where the last click selected something, in points.
    last_click: Option<egui::Pos2>,
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
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            canvas: GuiCanvas::new(),
            tool: Tool::Select,
            snap: false,
            step: 8.0,
            drag: None,
            shown: None,
            last_click: None,
        }
    }
}

/// How the scene maps to the viewport's points this frame, as the engine's camera maps it to the
/// viewport's pixels.
#[derive(Clone, Copy)]
struct ViewMap {
    rect: egui::Rect,
    pixels_per_point: f32,
    /// Pixels per unit of the scene.
    scale: f32,
    /// Where the scene's origin is, in pixels from the viewport's top-left, negated.
    offset: [f32; 2],
}

impl ViewMap {
    fn new(rect: egui::Rect, pixels_per_point: f32, view: View, pixels: [u32; 2]) -> Self {
        let scale = view.zoom * pixels_per_point;
        // As `Camera` places it, snapped to whole pixels.
        let offset = [0, 1].map(|i| (view.center[i] * scale - pixels[i] as f32 / 2.0).round());
        Self {
            rect,
            pixels_per_point,
            scale,
            offset,
        }
    }

    fn to_screen(self, [x, y]: [f32; 2]) -> egui::Pos2 {
        self.rect.min
            + egui::vec2(
                x * self.scale - self.offset[0],
                y * self.scale - self.offset[1],
            ) / self.pixels_per_point
    }

    fn to_scene(self, pos: egui::Pos2) -> [f32; 2] {
        let pixels = (pos - self.rect.min) * self.pixels_per_point;
        [
            (pixels.x + self.offset[0]) / self.scale,
            (pixels.y + self.offset[1]) / self.scale,
        ]
    }
}

impl Viewport {
    /// Where `point` of the scene was in the window, in points, when the viewport was last shown.
    pub fn scene_to_window(&self, point: [f32; 2]) -> Option<egui::Pos2> {
        self.shown.map(|map| map.to_screen(point))
    }

    pub fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        document: Option<&mut Document>,
        canvas_size: [f32; 2],
    ) {
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
            let [width, height] = canvas_size;
            ui.weak(format!("canvas {width} x {height}"));
        });
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &mut Context,
        document: &mut Document,
        canvas_size: [f32; 2],
    ) {
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        if rect.width() < 1.0 || rect.height() < 1.0 {
            return;
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

        if !panning {
            self.interact(ui, &response, &map, document);
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
        self.overlay(&painter, &map, document);
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
            document.selected = under.into_iter().nth(next);
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
                document.selected = Some(path.clone());
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
        let current = edit::node(document.file(), path);
        if current.is_some_and(|current| {
            [
                current.x,
                current.y,
                current.rotation,
                current.scale_x,
                current.scale_y,
            ] != [node.x, node.y, node.rotation, node.scale_x, node.scale_y]
        }) {
            let target = edit::node_mut(document.edit(), path).expect("the node is there");
            target.x = node.x;
            target.y = node.y;
            target.rotation = node.rotation;
            target.scale_x = node.scale_x;
            target.scale_y = node.scale_y;
        }
    }

    /// The selected node's outline and origin, and the handle of the tool.
    fn overlay(&self, painter: &egui::Painter, map: &ViewMap, document: &Document) {
        let (Some(path), Some(scene)) = (&document.selected, &document.preview) else {
            return;
        };
        if path.is_empty() {
            return;
        }
        let Some(id) = document.live_node(path) else {
            return;
        };
        let stroke = egui::Stroke::new(1.5, theme::ACCENT);
        let transform = scene.transform(id);
        if let Some((placement, rect)) = placement(scene, id) {
            let corners = placement
                .corners(rect)
                .map(|corner| map.to_screen(corner))
                .to_vec();
            painter.add(egui::Shape::closed_line(corners, stroke));
        }
        let origin = map.to_screen(transform.translation);
        let arm = 6.0;
        painter.line_segment(
            [origin - egui::vec2(arm, 0.0), origin + egui::vec2(arm, 0.0)],
            stroke,
        );
        painter.line_segment(
            [origin - egui::vec2(0.0, arm), origin + egui::vec2(0.0, arm)],
            stroke,
        );
        match self.tool {
            Tool::Select => {}
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
    canvas.set_camera(Some(Camera {
        zoom: map.scale,
        ..Camera::new(view.center[0], view.center[1])
    }));
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

/// Where the node shows what it shows, and the rectangle it covers in its own coordinates.
/// Text and tilemaps only move with their node.
fn placement(scene: &Scene, id: NodeId) -> Option<(Affine, Rect)> {
    let rect = scene.rect(id)?;
    let transform = scene.transform(id);
    let only_moves = matches!(
        scene.node(id).kind(),
        Kind::Text(_) | Kind::Tilemap(_) | Kind::TileLayer(_)
    );
    let transform = if only_moves {
        Affine::from_translation(transform.translation)
    } else {
        transform
    };
    Some((transform, rect))
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
/// seen, topmost first, then areas, then plain nodes with their origins nearby, then tilemaps.
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
        Kind::Area(_) => Pickable::Area,
        _ => Pickable::Seen,
    };
    let covering = |which: Pickable| {
        nodes
            .iter()
            .copied()
            .filter(move |&id| pickable(id) == which && scene.contains(id, point))
    };
    let near_origin = nodes.iter().copied().filter(|&id| {
        scene.rect(id).is_none()
            && map.to_screen(scene.transform(id).translation).distance(pos) <= PICK_DISTANCE
    });
    let mut found: Vec<NodePath> = Vec::new();
    let ids = covering(Pickable::Seen)
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

/// A drag changing the node at `path` with `tool`, from how it is now, with the pointer at
/// `from` in the scene.
fn start_drag(document: &Document, tool: Tool, path: NodePath, from: [f32; 2]) -> Option<Drag> {
    let start = edit::node(document.file(), &path)?.clone();
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
    Some(Drag {
        tool,
        path,
        start,
        from,
        to_parent: directions.inverse().unwrap_or(Affine::IDENTITY),
        mirrored: a * d - b * c < 0.0,
        origin: transform.translation,
        to_node: transform.inverse(),
    })
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
