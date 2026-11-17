//! Interfaces for Luau games: the `Ui` that a game's `gui(ui)` lays out with egui.
//!
//! A `Ui` is userdata that borrows egui's `Ui` for as long as the function it's passed to runs,
//! through [`mlua::Lua::scope`], so one kept after fails rather than reach a `Ui` that's gone. Its
//! methods are registered once, for every `Ui`. Containers call the function they're given with
//! the `Ui` inside them, in a scope of its own, while their own `Ui` stays borrowed: inside, a
//! script uses the `Ui` it's given.
//!
//! A canvas (`ui:canvas`) hands its function a `Paint` the same way, borrowed for the call: egui's
//! painter, clipped to the canvas, with positions from the canvas's top-left corner.

use mlua::{
    AnyUserData, FromLuaMulti, Function, IntoLuaMulti, Lua, Table, UserDataRef, UserDataRegistry,
    Value,
};

use super::Lent;
use crate::{
    graphics::{Color, Sprite},
    gui::egui,
};

/// The points between a sprite button's sprite and the edges of its frame.
const SPRITE_BUTTON_MARGIN: f32 = 2.0;

/// `ui:sprite_button`'s arguments: its sprite, text, whether it's selected, tint, width and height.
type SpriteButton = (
    UserDataRef<Sprite>,
    String,
    Option<bool>,
    Option<Color>,
    Option<f32>,
    Option<f32>,
);

/// Calls `f` with `ui`, as a `Ui` scripts can use until `f` returns.
pub(super) fn call<R: FromLuaMulti>(lua: &Lua, ui: &mut egui::Ui, f: &Function) -> mlua::Result<R> {
    lua.scope(|scope| f.call(scope.create_any_userdata_ref_mut(ui)?))
}

/// Registers `Ui`'s methods in `lua`, showing sprites through what `lent` lends, and `Paint`'s,
/// and returns the names of `Ui`'s.
pub(super) fn register(lua: &Lua, lent: &Lent) -> mlua::Result<Vec<&'static str>> {
    let mut names = Vec::new();
    lua.register_userdata_type::<egui::Ui>(|registry| {
        let mut methods = Methods {
            registry,
            names: &mut names,
        };
        add_methods(&mut methods, lent);
    })?;
    register_paint(lua)?;
    Ok(names)
}

/// The methods of `Ui`, as they're added, and their names.
struct Methods<'a, 'r> {
    registry: &'a mut mlua::UserDataRegistry<egui::Ui>,
    names: &'r mut Vec<&'static str>,
}

impl Methods<'_, '_> {
    fn add<A, R>(
        &mut self,
        name: &'static str,
        method: impl Fn(&Lua, &mut egui::Ui, A) -> mlua::Result<R> + 'static,
    ) where
        A: FromLuaMulti,
        R: IntoLuaMulti,
    {
        use mlua::UserDataMethods;
        self.names.push(name);
        // A function borrowing the `Ui` it's called on, rather than a mutable method, which mlua
        // refuses to call while it's running: so a container can hold another of its kind, whose
        // own `Ui` the inner call borrows.
        self.registry
            .add_function(name, move |lua, (this, args): (AnyUserData, A)| {
                this.borrow_mut_scoped::<egui::Ui, _>(|ui| method(lua, ui, args))?
            });
    }
}

fn add_methods(methods: &mut Methods<'_, '_>, lent: &Lent) {
    add_containers(methods);
    add_text(methods);
    add_widgets(methods);
    methods.add("sprite", {
        let lent = lent.clone();
        move |_,
              ui,
              (sprite, tint, width, height): (
            UserDataRef<Sprite>,
            Option<Color>,
            Option<f32>,
            Option<f32>,
        )| {
            let texture = lent.context(|ctx| ctx.gui_texture(sprite.texture()))?;
            let (min, max) = sprite.uv_bounds();
            let uv = egui::Rect::from_min_max(min.into(), max.into());
            let size = egui::vec2(
                width.unwrap_or(sprite.width()),
                height.unwrap_or(sprite.height()),
            );
            // At exactly its size, which an image added as a widget would shrink to fit a row.
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            egui::Image::new(texture)
                .uv(uv)
                .tint(color32(tint.unwrap_or(Color::WHITE)))
                .paint_at(ui, rect);
            Ok(())
        }
    });
    methods.add("sprite_button", {
        let lent = lent.clone();
        move |_, ui, (sprite, text, selected, tint, width, height): SpriteButton| {
            let texture = lent.context(|ctx| ctx.gui_texture(sprite.texture()))?;
            let (min, max) = sprite.uv_bounds();
            let uv = egui::Rect::from_min_max(min.into(), max.into());
            let size = egui::vec2(
                width.unwrap_or(sprite.width()),
                height.unwrap_or(sprite.height()),
            );
            let selected = selected.unwrap_or(false);
            // The sprite at exactly its size, in a frame as a button has, a margin wide.
            let (rect, response) = ui.allocate_exact_size(
                size + egui::Vec2::splat(2.0 * SPRITE_BUTTON_MARGIN),
                egui::Sense::click(),
            );
            // Named by its text, which egui doesn't know, as tests and screen readers find it.
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::Button,
                    response.enabled(),
                    selected,
                    text.as_str(),
                )
            });
            if ui.is_rect_visible(rect) {
                let visuals = ui.style().interact_selectable(&response, selected);
                ui.painter().rect(
                    rect,
                    visuals.corner_radius,
                    visuals.weak_bg_fill,
                    visuals.bg_stroke,
                    egui::StrokeKind::Inside,
                );
                egui::Image::new(texture)
                    .uv(uv)
                    .tint(color32(tint.unwrap_or(Color::WHITE)))
                    .paint_at(ui, rect.shrink(SPRITE_BUTTON_MARGIN));
            }
            let clicked = response.clicked();
            response.on_hover_text(text);
            Ok(clicked)
        }
    });
    methods.add("canvas", {
        let lent = lent.clone();
        move |lua, ui, (canvas, f): (Value, Function)| {
            let (label, options) = named(canvas, "canvas", "label")?;
            // All the room its container has left, each way it isn't given a size.
            let mut size = ui.available_size();
            if let Some(options) = &options {
                check_options(options, "canvas", &["label", "width", "height"])?;
                if let Some(width) = options.get::<Option<f32>>("width")? {
                    size.x = width;
                }
                if let Some(height) = options.get::<Option<f32>>("height")? {
                    size.y = height;
                }
            }
            if !(size.x.is_finite() && size.y.is_finite()) {
                return Err(mlua::Error::runtime(format!(
                    "a canvas's size is a number of points each way, not {} by {}",
                    size.x, size.y
                )));
            }
            let (response, painter) =
                ui.allocate_painter(size.max(egui::Vec2::ZERO), egui::Sense::click_and_drag());
            // Named by its label, which nothing shows on screen, as tests and screen readers find it.
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Canvas, response.enabled(), label.as_str())
            });
            let input = canvas_input(lua, ui, &response)?;
            // Where it is on the game's canvas, where `draw` draws, as the mouse's position is.
            let gui = ui.ctx();
            let pixels_per_point = gui.pixels_per_point();
            let scale_factor = gui.native_pixels_per_point().unwrap_or(pixels_per_point);
            let viewport = gui.viewport_rect();
            let output = [viewport.width(), viewport.height()]
                .map(|points| (points * pixels_per_point).round() as u32);
            let bounds = lent.context(|ctx| {
                let to_canvas = |pos: egui::Pos2| {
                    let pos = pos * pixels_per_point;
                    ctx.screen
                        .to_canvas([pos.x.into(), pos.y.into()], output, scale_factor.into())
                };
                crate::gui::rect_to_canvas(response.rect, to_canvas)
            })?;
            let paint = Paint {
                origin: response.rect.min,
                size: response.rect.size(),
                bounds,
                body: egui::TextStyle::Body.resolve(ui.style()),
                monospace: egui::TextStyle::Monospace.resolve(ui.style()),
                painter,
            };
            lua.scope(|scope| f.call::<()>((scope.create_any_userdata_ref(&paint)?, input)))
        }
    });
    methods.add("separator", |_, ui, ()| {
        ui.separator();
        Ok(())
    });
    methods.add("space", |_, ui, amount: Option<f32>| {
        let amount = amount.unwrap_or(ui.spacing().item_spacing.y);
        ui.add_space(amount);
        Ok(())
    });
}

fn add_containers(methods: &mut Methods<'_, '_>) {
    methods.add("window", |lua, ui, (window, f): (Value, Function)| {
        let (title, options) = named(window, "window", "title")?;
        let mut window = egui::Window::new(title);
        if let Some(options) = &options {
            check_options(
                options,
                "window",
                &[
                    "title",
                    "x",
                    "y",
                    "width",
                    "height",
                    "resizable",
                    "collapsible",
                ],
            )?;
            let x = options.get::<Option<f32>>("x")?;
            let y = options.get::<Option<f32>>("y")?;
            if x.is_some() || y.is_some() {
                window = window.default_pos([x.unwrap_or(0.0), y.unwrap_or(0.0)]);
            }
            if let Some(width) = options.get::<Option<f32>>("width")? {
                window = window.default_width(width);
            }
            if let Some(height) = options.get::<Option<f32>>("height")? {
                window = window.default_height(height);
            }
            if let Some(resizable) = options.get::<Option<bool>>("resizable")? {
                window = window.resizable(resizable);
            }
            if let Some(collapsible) = options.get::<Option<bool>>("collapsible")? {
                window = window.collapsible(collapsible);
            }
        }
        // Nothing runs while it's collapsed.
        let shown = window.show(ui.ctx(), |ui| call::<()>(lua, ui, &f));
        shown.and_then(|shown| shown.inner).unwrap_or(Ok(()))
    });
    methods.add("panel", |lua, ui, (panel, f): (Value, Function)| {
        let (side, options) = named(panel, "panel", "side")?;
        let id = format!("{side} panel");
        let mut panel = match side.as_str() {
            "left" => egui::Panel::left(id),
            "right" => egui::Panel::right(id),
            "top" => egui::Panel::top(id),
            "bottom" => egui::Panel::bottom(id),
            _ => {
                return Err(mlua::Error::runtime(format!(
                    r#"a panel's side is "left", "right", "top" or "bottom", not "{side}""#
                )));
            }
        };
        if let Some(options) = &options {
            check_options(options, "panel", &["side", "size", "resizable"])?;
            if let Some(size) = options.get::<Option<f32>>("size")? {
                panel = panel.default_size(size);
            }
            if let Some(resizable) = options.get::<Option<bool>>("resizable")? {
                panel = panel.resizable(resizable);
            }
        }
        panel.show(ui, |ui| call::<()>(lua, ui, &f)).inner
    });
    methods.add("horizontal", |lua, ui, f: Function| {
        ui.horizontal(|ui| call::<()>(lua, ui, &f)).inner
    });
    methods.add("vertical", |lua, ui, f: Function| {
        ui.vertical(|ui| call::<()>(lua, ui, &f)).inner
    });
    methods.add("group", |lua, ui, f: Function| {
        ui.group(|ui| call::<()>(lua, ui, &f)).inner
    });
    methods.add("collapsing", |lua, ui, (title, f): (String, Function)| {
        let shown = egui::CollapsingHeader::new(title).show(ui, |ui| call::<()>(lua, ui, &f));
        shown.body_returned.unwrap_or(Ok(()))
    });
    methods.add(
        "scroll",
        |lua, ui, (first, second): (Value, Option<Function>)| {
            // Its contents, or a table of options and then its contents.
            let (options, f) = match (first, second) {
                (Value::Function(f), None) => (None, f),
                (Value::Table(options), Some(f)) => (Some(options), f),
                _ => {
                    return Err(mlua::Error::runtime(
                        "a scroll takes a function, or a table of options and a function",
                    ));
                }
            };
            // By where it is, since a script may have several side by side.
            let id = ui.next_auto_id();
            let mut area = egui::ScrollArea::vertical().id_salt(id);
            if let Some(options) = &options {
                check_options(options, "scroll", &["stick_to_bottom"])?;
                if let Some(stick) = options.get::<Option<bool>>("stick_to_bottom")? {
                    area = area.stick_to_bottom(stick);
                }
            }
            area.show(ui, |ui| call::<()>(lua, ui, &f)).inner
        },
    );
    methods.add("modal", |lua, ui, (name, f): (String, Function)| {
        let modal = egui::Modal::new(egui::Id::unique(("modal", name)));
        let shown = modal.show(ui.ctx(), |ui| call::<()>(lua, ui, &f));
        let dismissed = shown.should_close();
        shown.inner?;
        Ok(dismissed)
    });
    methods.add("grid", |lua, ui, (name, f): (String, Function)| {
        egui::Grid::new(name)
            .show(ui, |ui| call::<()>(lua, ui, &f))
            .inner
    });
    methods.add("end_row", |_, ui, ()| {
        ui.end_row();
        Ok(())
    });
    methods.add("enabled", |lua, ui, (enabled, f): (bool, Function)| {
        ui.add_enabled_ui(enabled, |ui| call::<()>(lua, ui, &f))
            .inner
    });
}

fn add_text(methods: &mut Methods<'_, '_>) {
    methods.add("label", |_, ui, (text, color): (String, Option<Color>)| {
        match color {
            Some(color) => ui.colored_label(color32(color), text),
            None => ui.label(text),
        };
        Ok(())
    });
    methods.add("heading", |_, ui, text: String| {
        ui.heading(text);
        Ok(())
    });
    methods.add("monospace", |_, ui, text: String| {
        ui.monospace(text);
        Ok(())
    });
    methods.add("small", |_, ui, text: String| {
        ui.small(text);
        Ok(())
    });
}

fn add_widgets(methods: &mut Methods<'_, '_>) {
    methods.add("button", |_, ui, text: String| {
        Ok(ui.button(text).clicked())
    });
    methods.add("checkbox", |_, ui, (text, mut checked): (String, bool)| {
        ui.checkbox(&mut checked, text);
        Ok(checked)
    });
    methods.add(
        "radio",
        |_, ui, (current, value, text): (Value, Value, String)| {
            let clicked = ui.radio(current == value, text).clicked();
            Ok(if clicked { value } else { current })
        },
    );
    methods.add(
        "combo",
        |_, ui, (text, current, choices): (String, Value, Vec<Value>)| {
            let mut chosen = current.clone();
            egui::ComboBox::from_label(text)
                .selected_text(current.to_string()?)
                .show_ui(ui, |ui| {
                    for choice in choices {
                        let shown = choice.to_string()?;
                        if ui.selectable_label(choice == current, shown).clicked() {
                            chosen = choice;
                        }
                    }
                    Ok::<_, mlua::Error>(())
                })
                .inner
                .unwrap_or(Ok(()))?;
            Ok(chosen)
        },
    );
    methods.add(
        "slider",
        |_, ui, (text, mut value, min, max, step): (String, f64, f64, f64, Option<f64>)| {
            if !(min.is_finite() && max.is_finite()) {
                return Err(mlua::Error::runtime(format!(
                    "a slider's range runs between numbers, not from {min} to {max}"
                )));
            }
            ui.horizontal(|ui| {
                let mut slider = egui::Slider::new(&mut value, min..=max);
                if let Some(step) = step {
                    slider = slider.step_by(step);
                    // Whole steps show whole numbers.
                    if step.fract() == 0.0 {
                        slider = slider.fixed_decimals(0);
                    }
                }
                let response = ui.add(slider);
                // Named as its label names it on screen, which egui doesn't carry over.
                response.widget_info(|| {
                    egui::WidgetInfo::slider(response.enabled(), value, text.as_str())
                });
                ui.label(text.as_str());
            });
            Ok(value)
        },
    );
    methods.add(
        "drag",
        |_,
         ui,
         (text, mut value, speed, min, max): (
            String,
            f64,
            Option<f64>,
            Option<f64>,
            Option<f64>,
        )| {
            ui.horizontal(|ui| {
                let range = min.unwrap_or(f64::NEG_INFINITY)..=max.unwrap_or(f64::INFINITY);
                let field = egui::DragValue::new(&mut value)
                    .speed(speed.unwrap_or(1.0))
                    .range(range);
                let response = ui.add(field);
                // Named as its label names it on screen, which egui doesn't carry over.
                response.widget_info(|| {
                    let mut info = egui::WidgetInfo::drag_value(response.enabled(), value);
                    info.label = Some(text.clone());
                    info
                });
                ui.label(text.as_str());
            });
            Ok(value)
        },
    );
    methods.add(
        "text_input",
        |_, ui, (mut text, field): (String, Option<Value>)| {
            // Its hint, or a table of options holding it.
            let mut id = None;
            let mut label = None;
            let mut hint = String::new();
            match field {
                None | Some(Value::Nil) => {}
                Some(Value::String(given)) => hint = given.to_str()?.to_owned(),
                Some(Value::Table(options)) => {
                    check_options(&options, "text input", &["id", "label", "hint"])?;
                    id = options.get::<Option<String>>("id")?;
                    label = options.get::<Option<String>>("label")?;
                    hint = options.get::<Option<String>>("hint")?.unwrap_or_default();
                }
                Some(other) => {
                    return Err(mlua::Error::runtime(format!(
                        "a text input takes its hint or a table of options, not {}",
                        other.type_name()
                    )));
                }
            }
            let mut field = egui::TextEdit::singleline(&mut text).hint_text(hint.clone());
            // As a text edit's: see there.
            if let Some(id) = id {
                field = field.id_salt(id);
            }
            let response = ui.add(field);
            let entered = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if let Some(label) = label {
                name_field(&response, &text, &hint, label);
            }
            Ok((text, entered))
        },
    );
    methods.add(
        "text_edit",
        |lua, ui, (mut text, options): (String, Option<Table>)| {
            let mut id = None;
            let mut label = None;
            let mut hint = String::new();
            let mut rows = None;
            let mut monospace = false;
            let mut fill = false;
            if let Some(options) = &options {
                check_options(
                    options,
                    "text edit",
                    &["id", "label", "hint", "rows", "monospace", "fill"],
                )?;
                id = options.get::<Option<String>>("id")?;
                label = options.get::<Option<String>>("label")?;
                hint = options.get::<Option<String>>("hint")?.unwrap_or_default();
                rows = options.get::<Option<usize>>("rows")?;
                monospace = options.get::<Option<bool>>("monospace")?.unwrap_or(false);
                fill = options.get::<Option<bool>>("fill")?.unwrap_or(false);
            }
            // The scroll area of a field that fills, by the field's id, or by where it is.
            let scroll = match &id {
                Some(id) => egui::Id::unique(("text edit", id)),
                None => ui.next_auto_id(),
            };
            // As wide as there's room for, as a page is, rather than egui's few hundred points.
            let mut field = egui::TextEdit::multiline(&mut text)
                .desired_width(f32::INFINITY)
                .hint_text(hint.clone());
            // egui keeps a field's cursor, selection and undo history by its id, which is where
            // it is unless it's given one: so a field that shows one text then another, in the
            // same place, would undo the second into the first.
            if let Some(id) = id {
                field = field.id_salt(id);
            }
            if let Some(rows) = rows {
                field = field.desired_rows(rows);
            }
            if monospace {
                field = field.font(egui::TextStyle::Monospace);
            }
            let output = if fill {
                // All the room its container has left, in a scroll area of its own: the field is
                // at least as tall as that, and text taller than it scrolls, following the
                // cursor.
                let room = ui.available_height();
                egui::ScrollArea::vertical()
                    .id_salt(scroll)
                    .auto_shrink(false)
                    .show(ui, |ui| field.min_size(egui::vec2(0.0, room)).show(ui))
                    .inner
            } else {
                field.show(ui)
            };
            let changed = output.response.changed();
            // Named by its label, which nothing shows on screen, as tests and screen readers
            // find it.
            if let Some(label) = label {
                name_field(&output.response, &text, &hint, label);
            }
            let cursor = match output.cursor_range {
                Some(range) => Some(cursor(lua, &text, range)?),
                None => None,
            };
            Ok((text, changed, cursor))
        },
    );
    methods.add(
        "progress",
        |_, ui, (fraction, text): (f32, Option<String>)| {
            let mut bar = egui::ProgressBar::new(fraction);
            if let Some(text) = text {
                bar = bar.text(text);
            }
            ui.add(bar);
            Ok(())
        },
    );
}

/// Names a text field by `label`, which nothing shows on screen, as tests and screen readers
/// find it.
fn name_field(response: &egui::Response, text: &str, hint: &str, label: String) {
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::text_edit(response.enabled(), text, text, hint);
        info.label = Some(label.clone());
        info
    });
}

/// Where the cursor of a field holding `text` is, as scripts read it: its `line` and `column`,
/// from 1, counting characters; its `offset`, the bytes of `text` before it, so that
/// `string.sub(text, 1, offset)` is the text before the cursor; and `anchor`, the same of the
/// other end of what's selected, which is `offset` when nothing is.
fn cursor(lua: &Lua, text: &str, range: egui::text::CCursorRange) -> mlua::Result<Table> {
    // The bytes before character `index` of the text, and its line and column.
    let place = |index: usize| {
        let (mut line, mut column) = (1, 1);
        for (count, (offset, character)) in text.char_indices().enumerate() {
            if count == index {
                return (offset, line, column);
            }
            if character == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
        (text.len(), line, column)
    };
    let (offset, line, column) = place(range.primary.index.0);
    let (anchor, _, _) = place(range.secondary.index.0);
    let cursor = lua.create_table()?;
    cursor.set("line", line)?;
    cursor.set("column", column)?;
    cursor.set("offset", offset)?;
    cursor.set("anchor", anchor)?;
    Ok(cursor)
}

/// What the pointer did to the canvas `response` is of this frame, as scripts read it: whether
/// it's over the canvas, and where, from the canvas's top-left corner, while it's over it or
/// dragging it; whether it clicked it; how far it dragged it with the primary button; and how
/// far the wheel scrolled, and how much a zoom (a held Ctrl's scroll, a pinch) zoomed, while it's
/// over it.
fn canvas_input(lua: &Lua, ui: &egui::Ui, response: &egui::Response) -> mlua::Result<Table> {
    let input = lua.create_table()?;
    let hovered = response.hovered();
    input.set("hovered", hovered)?;
    let pointer = response.hover_pos().or_else(|| {
        response
            .dragged()
            .then(|| response.interact_pointer_pos())
            .flatten()
    });
    if let Some(pointer) = pointer {
        let at = pointer - response.rect.min;
        input.set("x", at.x)?;
        input.set("y", at.y)?;
    }
    input.set("clicked", response.clicked())?;
    let dragged = if response.dragged_by(egui::PointerButton::Primary) {
        response.drag_delta()
    } else {
        egui::Vec2::ZERO
    };
    input.set("dragged_x", dragged.x)?;
    input.set("dragged_y", dragged.y)?;
    let (scrolled, zoom) = if hovered {
        ui.input(|input| (input.smooth_scroll_delta(), input.zoom_delta()))
    } else {
        (egui::Vec2::ZERO, 1.0)
    };
    input.set("scrolled_x", scrolled.x)?;
    input.set("scrolled_y", scrolled.y)?;
    input.set("zoom", zoom)?;
    Ok(input)
}

/// What a canvas paints with, which its function is lent: egui's painter, clipped to the canvas,
/// and where the canvas is, so scripts paint in points from its top-left corner.
struct Paint {
    painter: egui::Painter,
    origin: egui::Pos2,
    size: egui::Vec2,
    /// Where the canvas is on the game's canvas, in canvas pixels.
    bounds: crate::math::Rect,
    /// The interface's text, and its monospace text, which text is painted in unless it's given a
    /// size.
    body: egui::FontId,
    monospace: egui::FontId,
}

impl Paint {
    /// Where `x`, `y` in the canvas is in the interface.
    fn at(&self, x: f32, y: f32) -> egui::Pos2 {
        self.origin + egui::vec2(x, y)
    }

    /// The rectangle `width` by `height` with its top-left corner at `x`, `y` in the canvas.
    fn rect(&self, x: f32, y: f32, width: f32, height: f32) -> egui::Rect {
        egui::Rect::from_min_size(self.at(x, y), egui::vec2(width, height))
    }

    /// The points of a path or polygon, given as `x1, y1, x2, y2, ...`: at least `least` of them.
    fn points(&self, what: &str, numbers: &[f32], least: usize) -> mlua::Result<Vec<egui::Pos2>> {
        if !numbers.len().is_multiple_of(2) || numbers.len() < 2 * least {
            return Err(mlua::Error::runtime(format!(
                "a {what} takes at least {least} points, each an x and a y, not {} numbers",
                numbers.len()
            )));
        }
        Ok(numbers
            .chunks_exact(2)
            .map(|point| self.at(point[0], point[1]))
            .collect())
    }

    /// Lays out `text` on one line as `options` say, in `color`, and returns it with how it lines
    /// up with where it's painted.
    fn layout(
        &self,
        text: String,
        color: egui::Color32,
        options: Option<&Table>,
    ) -> mlua::Result<(std::sync::Arc<egui::Galley>, egui::Align)> {
        let mut font = self.body.clone();
        let mut align = egui::Align::Min;
        if let Some(options) = options {
            check_options(options, "text", &["size", "monospace", "align"])?;
            if options.get::<Option<bool>>("monospace")?.unwrap_or(false) {
                font = self.monospace.clone();
            }
            if let Some(size) = options.get::<Option<f32>>("size")? {
                if !(size.is_finite() && size > 0.0) {
                    return Err(mlua::Error::runtime(format!(
                        "text's size is a number of points above 0, not {size}"
                    )));
                }
                font.size = size;
            }
            align = match options.get::<Option<String>>("align")?.as_deref() {
                None | Some("left") => egui::Align::Min,
                Some("center") => egui::Align::Center,
                Some("right") => egui::Align::Max,
                Some(other) => {
                    return Err(mlua::Error::runtime(format!(
                        r#"text is aligned "left", "center" or "right", not "{other}""#
                    )));
                }
            };
        }
        Ok((self.painter.layout_no_wrap(text, font, color), align))
    }
}

/// Registers `Paint`'s methods in `lua`, and returns their names.
fn register_paint(lua: &Lua) -> mlua::Result<Vec<&'static str>> {
    let mut names = Vec::new();
    lua.register_userdata_type::<Paint>(|registry| {
        let mut methods = PaintMethods {
            registry,
            names: &mut names,
        };
        add_paint_methods(&mut methods);
    })?;
    Ok(names)
}

/// The methods of `Paint`, as they're added, and their names.
struct PaintMethods<'a, 'r> {
    registry: &'a mut UserDataRegistry<Paint>,
    names: &'r mut Vec<&'static str>,
}

impl PaintMethods<'_, '_> {
    fn add<A, R>(
        &mut self,
        name: &'static str,
        method: impl Fn(&Paint, A) -> mlua::Result<R> + 'static,
    ) where
        A: FromLuaMulti,
        R: IntoLuaMulti,
    {
        use mlua::UserDataMethods;
        self.names.push(name);
        self.registry
            .add_method(name, move |_, paint, args| method(paint, args));
    }
}

/// A line, path or outline in `color`, `thickness` wide, or 1.
fn stroke(color: Color, thickness: Option<f32>) -> egui::Stroke {
    egui::Stroke::new(thickness.unwrap_or(1.0), color32(color))
}

fn add_paint_methods(methods: &mut PaintMethods<'_, '_>) {
    methods.add("size", |paint, ()| Ok((paint.size.x, paint.size.y)));
    methods.add("bounds", |paint, ()| Ok(paint.bounds));
    methods.add(
        "line",
        |paint, (x1, y1, x2, y2, color, thickness): (f32, f32, f32, f32, Color, Option<f32>)| {
            let points = [paint.at(x1, y1), paint.at(x2, y2)];
            paint.painter.line_segment(points, stroke(color, thickness));
            Ok(())
        },
    );
    methods.add(
        "path",
        |paint, (numbers, color, thickness): (Vec<f32>, Color, Option<f32>)| {
            let points = paint.points("path", &numbers, 2)?;
            paint.painter.line(points, stroke(color, thickness));
            Ok(())
        },
    );
    methods.add(
        "fill_polygon",
        |paint, (numbers, color): (Vec<f32>, Color)| {
            let points = paint.points("polygon", &numbers, 3)?;
            paint.painter.add(egui::Shape::convex_polygon(
                points,
                color32(color),
                egui::Stroke::NONE,
            ));
            Ok(())
        },
    );
    methods.add(
        "fill_rect",
        |paint,
         (x, y, width, height, color, rounding): (f32, f32, f32, f32, Color, Option<f32>)| {
            let rect = paint.rect(x, y, width, height);
            paint
                .painter
                .rect_filled(rect, rounding.unwrap_or(0.0), color32(color));
            Ok(())
        },
    );
    methods.add(
        "outline_rect",
        |paint,
         (x, y, width, height, color, thickness, rounding): (
            f32,
            f32,
            f32,
            f32,
            Color,
            Option<f32>,
            Option<f32>,
        )| {
            let rect = paint.rect(x, y, width, height);
            paint.painter.rect_stroke(
                rect,
                rounding.unwrap_or(0.0),
                stroke(color, thickness),
                egui::StrokeKind::Inside,
            );
            Ok(())
        },
    );
    methods.add(
        "fill_circle",
        |paint, (x, y, radius, color): (f32, f32, f32, Color)| {
            paint
                .painter
                .circle_filled(paint.at(x, y), radius, color32(color));
            Ok(())
        },
    );
    methods.add(
        "outline_circle",
        |paint, (x, y, radius, color, thickness): (f32, f32, f32, Color, Option<f32>)| {
            // Inside its edge, as a rectangle's outline is.
            let stroke = stroke(color, thickness);
            let radius = (radius - stroke.width / 2.0).max(0.0);
            paint.painter.circle_stroke(paint.at(x, y), radius, stroke);
            Ok(())
        },
    );
    methods.add(
        "text",
        |paint, (text, x, y, color, options): (String, f32, f32, Color, Option<Table>)| {
            let color = color32(color);
            let (galley, align) = paint.layout(text, color, options.as_ref())?;
            let size = galley.size();
            let anchor = egui::Align2([align, egui::Align::Min]);
            let rect = anchor.anchor_size(paint.at(x, y), size);
            paint.painter.galley(rect.min, galley, color);
            Ok((size.x, size.y))
        },
    );
    methods.add(
        "measure",
        |paint, (text, options): (String, Option<Table>)| {
            let (galley, _) = paint.layout(text, egui::Color32::WHITE, options.as_ref())?;
            let size = galley.size();
            Ok((size.x, size.y))
        },
    );
}

/// What a `what` is given first: a string, or a table of options holding the string as `key`, as
/// a window takes its title or its options.
fn named(value: Value, what: &str, key: &str) -> mlua::Result<(String, Option<Table>)> {
    match value {
        Value::String(name) => Ok((name.to_str()?.to_owned(), None)),
        Value::Table(options) => match options.get::<Option<String>>(key)? {
            Some(name) => Ok((name, Some(options))),
            None => Err(mlua::Error::runtime(format!(
                "a {what}'s options need its `{key}`"
            ))),
        },
        other => Err(mlua::Error::runtime(format!(
            "a {what} takes its {key} or a table of options, not {}",
            other.type_name()
        ))),
    }
}

/// Fails if `options`, the options of a `what`, has a key that isn't one of `known`.
fn check_options(options: &Table, what: &str, known: &[&str]) -> mlua::Result<()> {
    for pair in options.pairs::<Value, Value>() {
        let key = pair?.0.to_string()?;
        if !known.contains(&key.as_str()) {
            let known = known
                .iter()
                .map(|key| format!("`{key}`"))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(mlua::Error::runtime(format!(
                "a {what}'s options are {known}, not `{key}`"
            )));
        }
    }
    Ok(())
}

fn color32(color: Color) -> egui::Color32 {
    let [r, g, b, a] = color.to_rgba8();
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The methods `type` is declared with in the definitions file.
    fn declared_methods(type_name: &str) -> Vec<String> {
        // As checked out, which may have turned line endings into CRLF.
        let definitions = super::super::DEFINITIONS.replace("\r\n", "\n");
        let start = definitions
            .find(&format!("declare extern type {type_name} with\n"))
            .unwrap_or_else(|| panic!("the definitions declare `{type_name}`"));
        let block = &definitions[start..];
        let block = &block[..block.find("\nend\n").expect("`Ui`'s block ends")];
        // Methods, and generic ones as properties of generic function types, which methods can't
        // be: `function name(self...` and `name: <T>(self: Ui...`.
        block
            .lines()
            .skip(1)
            .map(str::trim)
            .filter(|line| !line.starts_with("--"))
            .map(|line| {
                let line = line.strip_prefix("function ").unwrap_or(line);
                line[..line.find(['(', ':']).expect("a method has parameters")].to_owned()
            })
            .collect()
    }

    #[test]
    fn the_definitions_declare_every_method_of_ui() {
        let mut registered = register(&Lua::new(), &Lent::default()).unwrap();
        registered.sort_unstable();
        let mut declared = declared_methods("Ui");
        declared.sort_unstable();
        assert_eq!(declared, registered);
    }

    #[test]
    fn the_definitions_declare_every_method_of_paint() {
        let mut registered = register_paint(&Lua::new()).unwrap();
        registered.sort_unstable();
        let mut declared = declared_methods("Paint");
        declared.sort_unstable();
        assert_eq!(declared, registered);
    }
}
