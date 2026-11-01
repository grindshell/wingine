//! The Animation panel: the open scene's clips, one at a time on a timeline, as Godot's Animation
//! panel shows an AnimationPlayer's. Each track is a row of keys under a ruler of seconds;
//! dragging along the ruler, or a row, scrubs the playhead, which poses the preview, and playing
//! moves it on in time. Keys are dragged to move them, and their menus take them out.

use wingine::{
    gui::egui,
    scene::{Clip, Interpolation, Keys, Track},
};

use crate::{clips, document::Document, edit, theme};

/// The width of the column of tracks' names, left of the timeline.
const NAMES_WIDTH: f32 = 260.0;
/// The height of a track's row, and of the ruler.
const ROW: f32 = 22.0;
/// Room at either end of the timeline, so keys at its ends show whole.
const PAD: f32 = 10.0;
/// Room for the rows' scroll bar, right of the timeline.
const SCROLL_BAR: f32 = 14.0;
/// Half a key's width.
const KEY_SIZE: f32 = 5.0;
/// Seconds between the ruler's marked times, the first that leaves room between their labels.
const MARKS: [f32; 13] = [
    0.01, 0.02, 0.05, 0.1, 0.2, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0,
];

pub struct Animation {
    /// Whether the playhead moves on in time.
    pub playing: bool,
    /// Whether the playhead and keys snap to whole `step`s of seconds.
    pub snap: bool,
    pub step: f32,
    /// The selected key, by its track's index and its own.
    selected: Option<(usize, usize)>,
    /// A key being dragged, by its track's index and its own, and the time it's dragged to.
    dragging: Option<(usize, usize, f32)>,
    /// When a row's menu was opened, to put a key there.
    menu_time: f32,
    /// The clip's new name, while it's being renamed, and whether its field needs the keyboard.
    renaming: Option<String>,
    focus_rename: bool,
    /// The scene and clip shown last, to forget the selected key when they change.
    shown: Option<(String, String)>,
    /// Whether the pointer was over the panel last frame.
    hovered: bool,
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            playing: false,
            snap: true,
            step: 0.05,
            selected: None,
            dragging: None,
            menu_time: 0.0,
            renaming: None,
            focus_rename: false,
            shown: None,
            hovered: false,
        }
    }
}

/// Where times are along the timeline, between `left` and `right` in points, for a clip
/// `duration` seconds long.
#[derive(Clone, Copy)]
struct Scale {
    left: f32,
    right: f32,
    duration: f32,
}

impl Scale {
    fn x(self, time: f32) -> f32 {
        self.left + PAD + time / self.duration * (self.span())
    }

    fn time(self, x: f32) -> f32 {
        ((x - self.left - PAD) / self.span() * self.duration).clamp(0.0, self.duration)
    }

    fn span(self) -> f32 {
        (self.right - self.left - 2.0 * PAD).max(1.0)
    }
}

impl Animation {
    /// Whether Delete is the selected key's: while the pointer's over the panel with a key
    /// selected.
    pub fn wants_delete(&self) -> bool {
        self.hovered && self.selected.is_some()
    }

    /// Takes the selected key out of the clip open in `document`.
    pub fn delete_selected(&mut self, document: &mut Document) {
        if let (Some((track, key)), Some(name)) =
            (self.selected.take(), document.timeline.clip.clone())
            && let Some(clip) = document.edit().clips.get_mut(&name)
        {
            clips::remove_key(clip, track, key);
        }
        document.end_change();
    }

    /// `time` snapped to the step, if snapping's on, within a clip `duration` long.
    fn snapped(&self, time: f32, duration: f32) -> f32 {
        let time = if self.snap && self.step > 0.0 {
            (time / self.step).round() * self.step
        } else {
            time
        };
        clips::tidy_time(time.clamp(0.0, duration))
    }

    /// The panel, for the scene open in `document`, moving the playhead on by `dt` seconds while
    /// it plays.
    pub fn ui(&mut self, ui: &mut egui::Ui, document: Option<&mut Document>, dt: f32) {
        self.hovered = ui.rect_contains_pointer(ui.max_rect());
        let Some(document) = document else {
            ui.weak("Open a scene to animate it.");
            return;
        };
        // The clip open, or if it's gone, the first.
        let clips = &document.file().clips;
        if document
            .timeline
            .clip
            .as_ref()
            .is_none_or(|name| !clips.contains_key(name))
        {
            document.timeline.clip = clips.keys().next().cloned();
            document.timeline.time = 0.0;
        }
        let shown = document
            .timeline
            .clip
            .clone()
            .map(|clip| (document.path.clone(), clip));
        if shown != self.shown {
            self.shown = shown;
            self.selected = None;
            self.dragging = None;
            self.renaming = None;
            self.playing = false;
        }

        self.toolbar(ui, document);
        ui.separator();
        let Some(name) = document.timeline.clip.clone() else {
            ui.weak(
                "The scene has no clips. New Clip makes one; then the Inspector's ♦ buttons key \
                 the selected node's properties into it.",
            );
            return;
        };
        let clip = document.file().clips[&name].clone();
        if self.playing {
            let time = document.timeline.time + dt;
            document.timeline.time = if clip.looping {
                time.rem_euclid(clip.duration)
            } else if time >= clip.duration {
                self.playing = false;
                clip.duration
            } else {
                time
            };
        }
        if let Some(error) = &document.pose_error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        self.timeline(ui, document, &name, &clip);
    }

    /// The clips to choose from and make, and the open one's playhead, length and looping.
    fn toolbar(&mut self, ui: &mut egui::Ui, document: &mut Document) {
        let current = document.timeline.clip.clone();
        ui.horizontal(|ui| {
            if let Some(name) = &mut self.renaming {
                let response = ui.add(egui::TextEdit::singleline(name).desired_width(140.0));
                if self.focus_rename {
                    response.request_focus();
                    self.focus_rename = false;
                } else if response.lost_focus() {
                    let to = name.trim().to_owned();
                    self.renaming = None;
                    if let Some(from) = &current
                        && !ui.input(|input| input.key_pressed(egui::Key::Escape))
                    {
                        let result = clips::rename_clip(document.edit(), from, &to);
                        document.end_change();
                        if result.is_ok() {
                            document.timeline.clip = Some(to);
                            self.shown = None;
                        }
                    }
                }
            } else {
                let names: Vec<String> = document.file().clips.keys().cloned().collect();
                let response = egui::ComboBox::from_id_salt("clip")
                    .selected_text(current.as_deref().unwrap_or("No clips"))
                    .width(140.0)
                    .show_ui(ui, |ui| {
                        for name in names {
                            ui.selectable_value(
                                &mut document.timeline.clip,
                                Some(name.clone()),
                                name,
                            );
                        }
                    })
                    .response
                    .on_hover_text("The clip on the timeline, which poses the scene");
                response
                    .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, "Clip"));
            }
            if ui
                .button("New Clip")
                .on_hover_text("Add a clip a second long")
                .clicked()
            {
                let name = clips::add_clip(document.edit(), "clip");
                document.end_change();
                document.timeline.clip = Some(name);
                document.timeline.time = 0.0;
            }
            let Some(name) = current else {
                return;
            };
            if ui.button("Rename Clip").clicked() {
                self.renaming = Some(name.clone());
                self.focus_rename = true;
            }
            if ui.button("Delete Clip").clicked() {
                document.edit().clips.remove(&name);
                document.end_change();
                document.timeline.clip = None;
                return;
            }
            let Some(clip) = document.file().clips.get(&name) else {
                return;
            };
            let was = (clip.duration, clip.looping);
            let (mut duration, mut looping) = was;
            let shortest = clips::last_key(clip).max(0.01);
            ui.separator();

            if transport(ui, "⏮", "To Start", "Back to the clip's start") {
                document.timeline.time = 0.0;
                self.playing = false;
            }
            let (icon, label) = if self.playing {
                ("⏸", "Pause Clip")
            } else {
                ("▶", "Play Clip")
            };
            if transport(ui, icon, label, "Play the clip on the preview") {
                self.playing = !self.playing;
                if self.playing && !looping && document.timeline.time >= duration {
                    document.timeline.time = 0.0;
                }
            }
            let mut time = document.timeline.time;
            if number(ui, "time", &mut time, 0.0..=duration).changed() {
                document.timeline.time = clips::tidy_time(time);
                self.playing = false;
            }
            ui.label("of");
            number(ui, "duration", &mut duration, shortest..=3600.0)
                .on_hover_text("How long the clip lasts: no shorter than its last key");
            ui.toggle_value(&mut looping, "Loop")
                .on_hover_text("Whether it starts over after its end");
            ui.separator();
            ui.toggle_value(&mut self.snap, "Snap")
                .on_hover_text("Snap the playhead and keys to the step");
            ui.add(
                egui::DragValue::new(&mut self.step)
                    .speed(0.005)
                    .range(0.005..=1.0)
                    .suffix(" s"),
            )
            .on_hover_text("The step times snap to");

            let duration = clips::tidy_time(duration);
            if (duration, looping) != was
                && let Some(clip) = document.edit().clips.get_mut(&name)
            {
                clip.duration = duration;
                clip.looping = looping;
            }
        });
    }

    /// The ruler, the tracks with their keys, and the playhead over them.
    fn timeline(&mut self, ui: &mut egui::Ui, document: &mut Document, name: &str, clip: &Clip) {
        let full = ui.available_rect_before_wrap();
        let scale = Scale {
            left: full.left() + NAMES_WIDTH,
            right: full.right() - SCROLL_BAR,
            duration: clip.duration,
        };
        let id = ui.make_persistent_id("timeline");

        // The ruler, which scrubs.
        let (row, _) = ui.allocate_exact_size(egui::vec2(full.width(), ROW), egui::Sense::hover());
        let ruler = egui::Rect::from_x_y_ranges(scale.left..=scale.right, row.y_range());
        let response = ui.interact(ruler, id.with("ruler"), egui::Sense::click_and_drag());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, "Timeline"));
        if (response.clicked() || response.dragged())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            document.timeline.time = self.snapped(scale.time(pointer.x), clip.duration);
            self.playing = false;
        }
        paint_ruler(ui, ruler, scale);
        ui.painter().text(
            egui::pos2(row.left() + 4.0, row.center().y),
            egui::Align2::LEFT_CENTER,
            match clip.tracks.len() {
                1 => "1 track".to_owned(),
                n => format!("{n} tracks"),
            },
            egui::FontId::proportional(12.0),
            ui.visuals().weak_text_color(),
        );

        let mut change = None;
        egui::ScrollArea::vertical()
            .id_salt("tracks")
            .auto_shrink(false)
            .show(ui, |ui| {
                if clip.tracks.is_empty() {
                    ui.weak("No tracks: select a node, and key its properties with the Inspector's ♦ buttons.");
                }
                for (i, track) in clip.tracks.iter().enumerate() {
                    if let Some(changed) = self.track(ui, document, id, scale, clip, i, track) {
                        change = Some(changed);
                    }
                }
            });
        if let Some(change) = change {
            if let Some(clip) = document.edit().clips.get_mut(name) {
                self.change(clip, change);
            }
            document.end_change();
        }

        // The playhead, over the ruler and the tracks.
        let x = scale.x(document.timeline.time.clamp(0.0, clip.duration));
        let painter = ui.painter_at(full);
        painter.line_segment(
            [egui::pos2(x, ruler.top()), egui::pos2(x, full.bottom())],
            egui::Stroke::new(1.5, theme::ACCENT),
        );
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(x - 5.0, ruler.top()),
                egui::pos2(x + 5.0, ruler.top()),
                egui::pos2(x, ruler.top() + 7.0),
            ],
            theme::ACCENT,
            egui::Stroke::NONE,
        ));
    }

    /// A track's row: its name and what's done to it, and its keys along the timeline. Returns a
    /// change to make to the clip.
    #[allow(clippy::too_many_arguments)]
    fn track(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        id: egui::Id,
        scale: Scale,
        clip: &Clip,
        i: usize,
        track: &Track,
    ) -> Option<Change> {
        let (row, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), egui::Sense::hover());
        let painter = ui.painter_at(row);
        if i % 2 == 1 {
            painter.rect_filled(row, 0.0, ui.visuals().faint_bg_color);
        }
        let property = track.keys.property_name();
        let node_name = track.node.rsplit('/').next().unwrap_or(&track.node);
        let node_name = if track.node.is_empty() {
            "(the scene)"
        } else {
            node_name
        };
        let label = format!("{node_name}:{property}");
        let mut change = None;

        // Its name, and buttons at the right of the column.
        let names = egui::Rect::from_min_max(
            row.min + egui::vec2(4.0, 0.0),
            egui::pos2(row.left() + NAMES_WIDTH - 6.0, row.bottom()),
        );
        let mut buttons = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(names)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        let remove = buttons.small_button("🗑").on_hover_text("Remove the track");
        remove.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Remove {label}"))
        });
        if remove.clicked() {
            change = Some(Change::RemoveTrack(i));
        }
        if let Keys::Number { interpolation, .. } = &track.keys {
            let other = match interpolation {
                Interpolation::Linear => Interpolation::Step,
                Interpolation::Step => Interpolation::Linear,
            };
            if buttons
                .small_button(interpolation.name())
                .on_hover_text(format!(
                    "How it gets from key to key: click for {}",
                    other.name()
                ))
                .clicked()
            {
                change = Some(Change::Interpolation(i, other));
            }
        }
        let used = buttons.min_rect().width();
        let mut names_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(names.with_max_x(names.right() - used - 4.0))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        // Its node: one of the file's, or one inside an instance of the file's.
        let path = edit::path_of(document.file(), &track.node);
        let deepest = edit::name_path(document.file(), &path).unwrap_or_default();
        let inside = track
            .node
            .strip_prefix(&deepest)
            .and_then(|rest| rest.strip_prefix('/'))
            .filter(|_| !deepest.is_empty())
            .filter(|_| {
                document
                    .preview
                    .as_ref()
                    .is_some_and(|scene| scene.find(scene.root(), &track.node).is_some())
            });
        let found = !track.node.is_empty() && (deepest == track.node || inside.is_some());
        let text = egui::RichText::new(&label);
        let text = if found {
            text
        } else {
            text.color(names_ui.visuals().error_fg_color)
        };
        let hover = if found {
            format!("{} of {}: click to select it", property, track.node)
        } else {
            format!("There's no node `{}` in the scene", track.node)
        };
        let response = names_ui
            .add(
                egui::Label::new(text)
                    .truncate()
                    .sense(egui::Sense::click()),
            )
            .on_hover_text(hover);
        if response.clicked() && found {
            match inside {
                Some(names) => document.select_inner(path, names.to_owned()),
                None => document.select(Some(path)),
            }
        }

        // Its keys along the timeline, over a lane that scrubs.
        let lane = egui::Rect::from_x_y_ranges(scale.left..=scale.right, row.y_range());
        let lane_response = ui.interact(lane, id.with(("lane", i)), egui::Sense::click_and_drag());
        if (lane_response.clicked() || lane_response.dragged())
            && let Some(pointer) = lane_response.interact_pointer_pos()
        {
            document.timeline.time = self.snapped(scale.time(pointer.x), clip.duration);
            self.playing = false;
            self.selected = None;
        }
        if lane_response.secondary_clicked()
            && let Some(pointer) = lane_response.interact_pointer_pos()
        {
            self.menu_time = self.snapped(scale.time(pointer.x), clip.duration);
        }
        lane_response.context_menu(|ui| {
            if ui.button("Insert Key").clicked() {
                change = Some(Change::Insert(i, self.menu_time));
            }
        });
        painter.line_segment(
            [
                egui::pos2(lane.left(), lane.center().y),
                egui::pos2(lane.right(), lane.center().y),
            ],
            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );

        let square = !matches!(
            track.keys,
            Keys::Number {
                interpolation: Interpolation::Linear,
                ..
            }
        );
        for (k, time) in track.keys.times().into_iter().enumerate() {
            let dragged_to = match self.dragging {
                Some((track, key, to)) if (track, key) == (i, k) => Some(to),
                _ => None,
            };
            let center = egui::pos2(scale.x(dragged_to.unwrap_or(time)), lane.center().y);
            let rect = egui::Rect::from_center_size(center, egui::vec2(2.0 * KEY_SIZE + 4.0, ROW));
            let response = ui.interact(rect, id.with(("key", i, k)), egui::Sense::click_and_drag());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Button,
                    true,
                    format!("{label} key {}", k + 1),
                )
            });
            let response = response.on_hover_text(format!("{time} s"));
            if response.clicked() {
                self.selected = Some((i, k));
                document.timeline.time = time;
                self.playing = false;
            }
            if response.drag_started() {
                self.selected = Some((i, k));
                self.dragging = Some((i, k, time));
            }
            if response.dragged()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let to = self.snapped(scale.time(pointer.x), clip.duration);
                if let Some(dragging) = &mut self.dragging {
                    dragging.2 = to;
                }
            }
            if response.drag_stopped()
                && let Some((_, _, to)) = self.dragging.take()
            {
                change = Some(Change::Move(i, k, to));
            }
            response.context_menu(|ui| {
                if ui.button("Delete Key").clicked() {
                    change = Some(Change::RemoveKey(i, k));
                }
            });

            let selected = self.selected == Some((i, k));
            let fill = if selected { theme::ACCENT } else { theme::KEY };
            let stroke = if selected || response.hovered() {
                egui::Stroke::new(1.5, egui::Color32::WHITE)
            } else {
                egui::Stroke::new(1.0, egui::Color32::from_black_alpha(160))
            };
            let s = KEY_SIZE;
            let points = if square {
                let s = s - 1.0;
                vec![
                    center + egui::vec2(-s, -s),
                    center + egui::vec2(s, -s),
                    center + egui::vec2(s, s),
                    center + egui::vec2(-s, s),
                ]
            } else {
                vec![
                    center + egui::vec2(0.0, -s),
                    center + egui::vec2(s, 0.0),
                    center + egui::vec2(0.0, s),
                    center + egui::vec2(-s, 0.0),
                ]
            };
            painter.add(egui::Shape::convex_polygon(points, fill, stroke));
        }
        change
    }

    /// Makes `change` to `clip`, keeping the selected key selected.
    fn change(&mut self, clip: &mut Clip, change: Change) {
        match change {
            Change::Move(track, key, time) => {
                self.selected = clips::move_key(clip, track, key, time).map(|key| (track, key));
            }
            Change::RemoveKey(track, key) => {
                clips::remove_key(clip, track, key);
                self.selected = None;
            }
            Change::Insert(track, time) => {
                let Some(row) = clip.tracks.get(track) else {
                    return;
                };
                let (node, property) = (row.node.clone(), row.keys.property_name());
                if let Some(value) = row.keys.value_at(time) {
                    clips::set_key(clip, &node, property, time, value)
                        .expect("the value is the track's own");
                }
            }
            Change::RemoveTrack(track) => {
                if track < clip.tracks.len() {
                    clip.tracks.remove(track);
                }
                self.selected = None;
            }
            Change::Interpolation(track, to) => clips::set_interpolation(clip, track, to),
        }
    }
}

/// A change to the clip on the timeline, by its tracks' and keys' indices.
enum Change {
    Move(usize, usize, f32),
    RemoveKey(usize, usize),
    Insert(usize, f32),
    RemoveTrack(usize),
    Interpolation(usize, Interpolation),
}

/// A button of the playhead's, showing `icon` and named `label`, returning whether it was
/// clicked.
fn transport(ui: &mut egui::Ui, icon: &str, label: &str, hover: &str) -> bool {
    let response = ui.button(icon).on_hover_text(hover);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    response.clicked()
}

/// A field for seconds, named `name` for screen readers and tests.
fn number(
    ui: &mut egui::Ui,
    name: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
) -> egui::Response {
    let response = ui.add(
        egui::DragValue::new(value)
            .speed(0.01)
            .range(range)
            .max_decimals(4)
            .suffix(" s"),
    );
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::drag_value(response.enabled(), f64::from(*value));
        info.label = Some(name.to_owned());
        info
    });
    response
}

/// Seconds along the ruler, marked as often as leaves room for their labels.
fn paint_ruler(ui: &egui::Ui, ruler: egui::Rect, scale: Scale) {
    let painter = ui.painter_at(ruler.expand2(egui::vec2(PAD, 0.0)));
    painter.rect_filled(ruler, 0.0, theme::BACKGROUND);
    let every = MARKS
        .into_iter()
        .find(|every| every / scale.duration * scale.span() >= 56.0)
        .unwrap_or(60.0);
    let color = ui.visuals().weak_text_color();
    let mut n = 0;
    loop {
        let time = n as f32 * every;
        if time > scale.duration + 1e-4 {
            break;
        }
        let x = scale.x(time);
        painter.line_segment(
            [
                egui::pos2(x, ruler.bottom() - 6.0),
                egui::pos2(x, ruler.bottom()),
            ],
            egui::Stroke::new(1.0, color),
        );
        painter.text(
            egui::pos2(x + 3.0, ruler.top() + 2.0),
            egui::Align2::LEFT_TOP,
            clips::tidy_time(time).to_string(),
            egui::FontId::proportional(11.0),
            color,
        );
        // Halfway between, a shorter mark.
        let half = scale.x(time + every / 2.0);
        if time + every / 2.0 <= scale.duration {
            painter.line_segment(
                [
                    egui::pos2(half, ruler.bottom() - 3.0),
                    egui::pos2(half, ruler.bottom()),
                ],
                egui::Stroke::new(1.0, color),
            );
        }
        n += 1;
    }
}
