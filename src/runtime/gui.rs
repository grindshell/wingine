//! Interfaces for Luau games: the `Ui` that a game's `gui(ui)` lays out with egui.
//!
//! A `Ui` is userdata that borrows egui's `Ui` for as long as the function it's passed to runs,
//! through [`mlua::Lua::scope`], so one kept after fails rather than reach a `Ui` that's gone. Its
//! methods are registered once, for every `Ui`. Containers call the function they're given with
//! the `Ui` inside them, in a scope of its own, while their own `Ui` stays borrowed: inside, a
//! script uses the `Ui` it's given.

use mlua::{FromLuaMulti, Function, IntoLuaMulti, Lua, Table, UserDataRef, Value};

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

/// Registers `Ui`'s methods in `lua`, showing sprites through what `lent` lends, and returns
/// their names.
pub(super) fn register(lua: &Lua, lent: &Lent) -> mlua::Result<Vec<&'static str>> {
    let mut names = Vec::new();
    lua.register_userdata_type::<egui::Ui>(|registry| {
        let mut methods = Methods {
            registry,
            names: &mut names,
        };
        add_methods(&mut methods, lent);
    })?;
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
        self.registry.add_method_mut(name, method);
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
    methods.add("scroll", |lua, ui, f: Function| {
        // By where it is, since a script may have several side by side.
        let id = ui.next_auto_id();
        egui::ScrollArea::vertical()
            .id_salt(id)
            .show(ui, |ui| call::<()>(lua, ui, &f))
            .inner
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
        |_, ui, (mut text, hint): (String, Option<String>)| {
            let field = egui::TextEdit::singleline(&mut text).hint_text(hint.unwrap_or_default());
            let response = ui.add(field);
            let entered = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            Ok((text, entered))
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

    /// The methods `Ui` is declared with in the definitions file.
    fn declared_methods() -> Vec<String> {
        // As checked out, which may have turned line endings into CRLF.
        let definitions = super::super::DEFINITIONS.replace("\r\n", "\n");
        let start = definitions
            .find("declare extern type Ui with\n")
            .expect("the definitions declare `Ui`");
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
        let mut declared = declared_methods();
        declared.sort_unstable();
        assert_eq!(declared, registered);
    }
}
