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
//!
//! Headings are always in a family of fonts of their own, [`HEADING`], which holds the body's
//! fonts unless `ui:set_fonts` puts another first: so their text style never names a family the
//! fonts of the pass lack, though egui changes fonts only between passes once text has been laid
//! out in one.

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::Arc,
};

use mlua::{
    AnyUserData, FromLuaMulti, Function, IntoLuaMulti, Lua, Table, UserDataRef, UserDataRegistry,
    Value,
};

use super::Lent;
use crate::{
    Error,
    graphics::{Color, Sprite},
    gui::egui,
};

/// The family of fonts headings, and windows' titles, are in.
const HEADING: &str = "heading";

/// What a game's fonts are named among egui's, before their paths, so none is taken for one of
/// egui's own.
const GAME_FONT: &str = "game:";

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

/// Registers `Ui`'s methods in `lua`, showing sprites through what `lent` lends and reading fonts
/// from the game's `folder`, and `Paint`'s, and returns the names of `Ui`'s.
pub(super) fn register(lua: &Lua, lent: &Lent, folder: &Path) -> mlua::Result<Vec<&'static str>> {
    let mut names = Vec::new();
    lua.register_userdata_type::<egui::Ui>(|registry| {
        let mut methods = Methods {
            registry,
            names: &mut names,
        };
        add_methods(&mut methods, lent, folder);
    })?;
    register_paint(lua, lent)?;
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

fn add_methods(methods: &mut Methods<'_, '_>, lent: &Lent, folder: &Path) {
    methods.add("scale", |_, ui, factor: Option<f32>| {
        if let Some(factor) = factor {
            if !(0.25..=4.0).contains(&factor) {
                return Err(mlua::Error::runtime("UI scale must be between 0.25 and 4"));
            }
            ui.ctx().set_zoom_factor(factor);
        }
        Ok(ui.ctx().zoom_factor())
    });
    methods.add("set_theme", |_, ui, palette: Table| set_theme(ui, palette));
    methods.add("set_fonts", {
        let folder = folder.to_owned();
        // The fonts set last, so a game setting the same ones each frame doesn't read their files
        // again, nor have egui compare them byte by byte with those it has.
        let set = RefCell::new(None);
        move |_, ui, fonts: Table| {
            let fonts = Fonts::read(&fonts)?;
            if set.borrow().as_ref() == Some(&fonts) {
                return Ok(());
            }
            fonts.install(ui.ctx(), &folder)?;
            // This `Ui` was made in the styles before; those made from now on are in these.
            ui.style_mut().text_styles = ui.ctx().global_style().text_styles.clone();
            *set.borrow_mut() = Some(fonts);
            Ok(())
        }
    });
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
    methods.add("available_size", |_, ui, ()| {
        let size = ui.available_size();
        Ok((size.x, size.y))
    });
}

/// Installs a game's palette for this UI and subsequent panels, windows, and popups.
fn set_theme(ui: &mut egui::Ui, palette: Table) -> mlua::Result<()> {
    check_options(
        &palette,
        "theme",
        &[
            "dark", "panel", "window", "field", "control", "hover", "active", "text", "muted",
            "border", "accent", "radius",
        ],
    )?;
    let color = |name: &str| palette.get::<Color>(name).map(color32);
    let theme = if palette.get::<Option<bool>>("dark")?.unwrap_or(true) {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    let mut visuals = theme.default_visuals();
    let panel = color("panel")?;
    let field = color("field")?;
    let control = color("control")?;
    let hover = color("hover")?;
    let active = color("active")?;
    let ink = color("text")?;
    let border = color("border")?;
    let accent = color("accent")?;
    let radius = egui::CornerRadius::same(palette.get::<Option<u8>>("radius")?.unwrap_or(3));

    visuals.panel_fill = panel;
    visuals.window_fill = color("window")?;
    visuals.window_stroke = egui::Stroke::new(1.0, border);
    visuals.window_corner_radius = radius;
    visuals.menu_corner_radius = radius;
    visuals.faint_bg_color = control;
    visuals.extreme_bg_color = field;
    visuals.text_edit_bg_color = Some(field);
    visuals.weak_text_color = Some(color("muted")?);
    visuals.hyperlink_color = accent;
    visuals.selection.bg_fill = active;
    visuals.selection.stroke = egui::Stroke::new(1.0, accent);
    visuals.text_cursor.stroke.color = accent;
    for (widget, fill, outline) in [
        (&mut visuals.widgets.noninteractive, panel, border),
        (&mut visuals.widgets.inactive, control, border),
        (&mut visuals.widgets.hovered, hover, accent),
        (&mut visuals.widgets.active, active, accent),
        (&mut visuals.widgets.open, active, accent),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = egui::Stroke::new(1.0, outline);
        widget.fg_stroke.color = ink;
        widget.corner_radius = radius;
    }
    ui.ctx().set_theme(theme);
    ui.ctx().set_visuals(visuals);
    ui.set_style(ui.ctx().global_style());
    Ok(())
}

/// Sets `gui` in egui's own fonts, at its own sizes, as a game starts: headings in a family of
/// their own, holding the body's fonts.
pub(super) fn reset_fonts(gui: &egui::Context) {
    Fonts::default().apply(gui, font_definitions(None, None, None));
}

/// The fonts a game's interface is in, as `ui:set_fonts` takes them: for each role text plays, a
/// font file in the game's folder, a size in points, or both, each left to egui unless it's given.
#[derive(Clone, Default, PartialEq, Debug)]
struct Fonts {
    /// Labels, buttons, fields, small text and what canvases paint.
    body: FontChoice,
    /// Headings, and windows' titles.
    heading: FontChoice,
    /// Monospace text.
    monospace: FontChoice,
}

/// The font of one role in [`Fonts`].
#[derive(Clone, Default, PartialEq, Debug)]
struct FontChoice {
    /// The font's file, a path from the game's folder.
    file: Option<String>,
    /// The size its text is, in points.
    size: Option<f32>,
}

/// A font file read for an interface: its name among egui's fonts, and what it holds.
type FontFile = (String, Arc<egui::FontData>);

impl Fonts {
    /// The roles text plays, which `ui:set_fonts` takes fonts for.
    const ROLES: [&str; 3] = ["body", "heading", "monospace"];

    fn read(fonts: &Table) -> mlua::Result<Self> {
        for pair in fonts.pairs::<Value, Value>() {
            let key = pair?.0.to_string()?;
            if !Self::ROLES.contains(&key.as_str()) {
                return Err(mlua::Error::runtime(format!(
                    "an interface's fonts are its `body`, `heading` and `monospace`, not `{key}`"
                )));
            }
        }
        let [body, heading, monospace] =
            Self::ROLES.map(|role| FontChoice::read(fonts.get(role)?, role));
        Ok(Self {
            body: body?,
            heading: heading?,
            monospace: monospace?,
        })
    }

    /// Sets `gui` in these fonts, read from `folder`: from this pass, unless text has been laid
    /// out in it, when they show from the next, though their sizes change now.
    fn install<'a>(&'a self, gui: &egui::Context, folder: &Path) -> mlua::Result<()> {
        // Each file once, though it's the font of several roles.
        let mut read: HashMap<&str, FontFile> = HashMap::new();
        let mut file = |choice: &'a FontChoice| -> mlua::Result<Option<FontFile>> {
            let Some(file) = choice.file.as_deref() else {
                return Ok(None);
            };
            if let Some(font) = read.get(file) {
                return Ok(Some(font.clone()));
            }
            let path = folder.join(file);
            let bytes = crate::files::read(&path).map_err(|source| {
                mlua::Error::external(Error::ReadFile {
                    path: path.clone(),
                    source,
                })
            })?;
            // Here, since egui panics on a font it can't read, and only once it lays text out.
            if ab_glyph::FontRef::try_from_slice(&bytes).is_err() {
                return Err(mlua::Error::external(Error::Font {
                    path,
                    message: "this isn't a TrueType or OpenType font".to_owned(),
                }));
            }
            let font = (
                format!("{GAME_FONT}{file}"),
                Arc::new(egui::FontData::from_owned(bytes)),
            );
            read.insert(file, font.clone());
            Ok(Some(font))
        };
        let definitions = font_definitions(
            file(&self.body)?,
            file(&self.heading)?,
            file(&self.monospace)?,
        );
        self.apply(gui, definitions);
        Ok(())
    }

    /// Sets `gui` in the fonts `definitions` holds, at these sizes, in its dark and light styles
    /// alike.
    fn apply(&self, gui: &egui::Context, definitions: egui::FontDefinitions) {
        gui.set_fonts(definitions);
        let styles = self.text_styles();
        gui.all_styles_mut(|style| style.text_styles.clone_from(&styles));
    }

    /// egui's text styles at these sizes, headings in their own family. Small text is in the
    /// body's font, smaller than the body by as much as egui's own is.
    fn text_styles(&self) -> BTreeMap<egui::TextStyle, egui::FontId> {
        use egui::{FontFamily, FontId, TextStyle};
        let mut styles = egui::style::default_text_styles();
        let default = |style: TextStyle| styles[&style].size;
        let body = self.body.size.unwrap_or(default(TextStyle::Body));
        let small = default(TextStyle::Small) * body / default(TextStyle::Body);
        let heading = self.heading.size.unwrap_or(default(TextStyle::Heading));
        let monospace = self.monospace.size.unwrap_or(default(TextStyle::Monospace));
        for (style, font) in [
            (TextStyle::Small, FontId::proportional(small)),
            (TextStyle::Body, FontId::proportional(body)),
            (TextStyle::Button, FontId::proportional(body)),
            (
                TextStyle::Heading,
                FontId::new(heading, FontFamily::Name(HEADING.into())),
            ),
            (TextStyle::Monospace, FontId::monospace(monospace)),
        ] {
            styles.insert(style, font);
        }
        styles
    }
}

impl FontChoice {
    /// The font of `role`: nothing, its file, or a table of its `file` and `size`.
    fn read(value: Value, role: &str) -> mlua::Result<Self> {
        match value {
            Value::Nil => Ok(Self::default()),
            Value::String(file) => Ok(Self {
                file: Some(file.to_str()?.to_owned()),
                size: None,
            }),
            Value::Table(options) => {
                check_options(&options, &format!("{role} font"), &["file", "size"])?;
                let size = options.get::<Option<f32>>("size")?;
                if let Some(size) = size
                    && !(size.is_finite() && size > 0.0)
                {
                    return Err(mlua::Error::runtime(format!(
                        "a {role} font's size is a number of points above 0, not {size}"
                    )));
                }
                Ok(Self {
                    file: options.get("file")?,
                    size,
                })
            }
            other => Err(mlua::Error::runtime(format!(
                "a {role} font is its file or a table of options, not {}",
                other.type_name()
            ))),
        }
    }
}

/// egui's fonts, with a game's put first: `body` for text, `monospace` for monospace text, and for
/// headings, `heading` and then the body's fonts. A character a font lacks is drawn in the next
/// font of its family that has it, so egui's own fonts are the last of each.
fn font_definitions(
    body: Option<FontFile>,
    heading: Option<FontFile>,
    monospace: Option<FontFile>,
) -> egui::FontDefinitions {
    use egui::FontFamily;
    let mut definitions = egui::FontDefinitions::default();
    put_first(&mut definitions, FontFamily::Proportional, body);
    put_first(&mut definitions, FontFamily::Monospace, monospace);
    let body_fonts = definitions.families[&FontFamily::Proportional].clone();
    let heading_family = FontFamily::Name(HEADING.into());
    definitions
        .families
        .insert(heading_family.clone(), body_fonts);
    put_first(&mut definitions, heading_family, heading);
    definitions
}

/// Puts `font`, if there's one, first among `family`'s fonts in `definitions`.
fn put_first(
    definitions: &mut egui::FontDefinitions,
    family: egui::FontFamily,
    font: Option<FontFile>,
) {
    if let Some((name, data)) = font {
        definitions.font_data.insert(name.clone(), data);
        definitions
            .families
            .entry(family)
            .or_default()
            .insert(0, name);
    }
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
                check_options(options, "scroll", &["stick_to_bottom", "height"])?;
                if let Some(height) = options.get::<Option<f32>>("height")? {
                    if !height.is_finite() || height <= 0.0 {
                        return Err(mlua::Error::runtime(
                            "scroll height must be finite and positive",
                        ));
                    }
                    area = area.min_scrolled_height(0.0).max_height(height);
                }
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
    methods.add(
        "button",
        |_, ui, (text, options): (String, Option<Table>)| {
            let Some(options) = options else {
                return Ok(ui.button(text).clicked());
            };
            check_options(&options, "button", &["width", "height"])?;
            let width = size_option(&options, "button", "width")?;
            let height = size_option(&options, "button", "height")?;
            let button = egui::Button::new(text)
                .min_size(egui::vec2(width.unwrap_or(0.0), height.unwrap_or(0.0)));
            // In room as wide as it's given, so its text wraps to that rather than widening it.
            let room = egui::vec2(
                width.unwrap_or(ui.available_width()),
                height.unwrap_or(ui.available_height()),
            );
            Ok(ui.allocate_ui(room, |ui| ui.add(button)).inner.clicked())
        },
    );
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
            let mut tab = Tab::Focus;
            if let Some(options) = &options {
                check_options(
                    options,
                    "text edit",
                    &["id", "label", "hint", "rows", "monospace", "fill", "tab"],
                )?;
                id = options.get::<Option<String>>("id")?;
                label = options.get::<Option<String>>("label")?;
                hint = options.get::<Option<String>>("hint")?.unwrap_or_default();
                rows = options.get::<Option<usize>>("rows")?;
                monospace = options.get::<Option<bool>>("monospace")?.unwrap_or(false);
                fill = options.get::<Option<bool>>("fill")?.unwrap_or(false);
                tab = Tab::read(options.get("tab")?)?;
            }
            // The scroll area of a field that fills, by the field's id, or by where it is.
            let scroll = match &id {
                Some(id) => egui::Id::unique(("text edit", id)),
                None => ui.next_auto_id(),
            };
            // Whether Tab or Shift+Tab changed the text, before the field saw it.
            let mut tabbed = false;
            let mut show = |ui: &mut egui::Ui, room: Option<f32>| {
                // egui keeps a field's cursor, selection and undo history by its id, which is
                // where it is unless it's given one: so a field that shows one text then another,
                // in the same place, would undo the second into the first.
                let field_id = match &id {
                    Some(id) => ui.make_persistent_id(("text edit field", id)),
                    None => {
                        let field_id = ui.next_auto_id();
                        ui.skip_ahead_auto_ids(1);
                        field_id
                    }
                };
                if let Tab::Spaces(width) = tab {
                    tabbed = indent_on_tab(ui, field_id, &mut text, width);
                }
                // As wide as there's room for, as a page is, rather than egui's few hundred
                // points.
                let mut field = egui::TextEdit::multiline(&mut text)
                    .id(field_id)
                    .desired_width(f32::INFINITY)
                    .hint_text(hint.clone())
                    // Tab keeps the field focused, and types a tab, unless it was taken above.
                    .lock_focus(tab != Tab::Focus);
                if let Some(rows) = rows {
                    field = field.desired_rows(rows);
                }
                if monospace {
                    field = field.font(egui::TextStyle::Monospace);
                }
                if let Some(room) = room {
                    field = field.min_size(egui::vec2(0.0, room));
                }
                field.show(ui)
            };
            let output = if fill {
                // All the room its container has left, in a scroll area of its own: the field is
                // at least as tall as that, and text taller than it scrolls, following the
                // cursor.
                let room = ui.available_height();
                egui::ScrollArea::vertical()
                    .id_salt(scroll)
                    .auto_shrink(false)
                    .show(ui, |ui| show(ui, Some(room)))
                    .inner
            } else {
                show(ui, None)
            };
            let changed = output.response.changed() || tabbed;
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

/// What Tab does in a text edit, its `tab` option.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tab {
    /// Moves focus to the next widget, as in any field: without the option, or `false`.
    Focus,
    /// Types a tab character, `true`; Shift+Tab takes one, or four spaces, off the line's start.
    Character,
    /// A number: indents by that many spaces, to the next stop, or each line selected a level
    /// more; Shift+Tab takes a level off each line.
    Spaces(usize),
}

impl Tab {
    /// The most spaces a level of indentation may be.
    const MOST_SPACES: usize = 16;

    fn read(value: Value) -> mlua::Result<Self> {
        let spaces = match value {
            Value::Nil | Value::Boolean(false) => return Ok(Self::Focus),
            Value::Boolean(true) => return Ok(Self::Character),
            Value::Integer(spaces) => Some(spaces as f64),
            Value::Number(spaces) => Some(spaces),
            _ => None,
        };
        match spaces {
            Some(spaces)
                if spaces.fract() == 0.0 && (1.0..=Self::MOST_SPACES as f64).contains(&spaces) =>
            {
                Ok(Self::Spaces(spaces as usize))
            }
            _ => Err(mlua::Error::runtime(format!(
                "a text edit's tab is true or a whole number of spaces from 1 to {}, not {}",
                Self::MOST_SPACES,
                match spaces {
                    Some(spaces) => spaces.to_string(),
                    None => "that".to_owned(),
                }
            ))),
        }
    }
}

/// Indents or unindents the text of the field `id` by `width` spaces if it has focus and Tab or
/// Shift+Tab was pressed, taking the key from egui, and puts its cursor where it goes. Returns
/// whether it changed the text.
fn indent_on_tab(ui: &mut egui::Ui, id: egui::Id, text: &mut String, width: usize) -> bool {
    if !ui.memory(|memory| memory.has_focus(id)) {
        return false;
    }
    let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) else {
        return false;
    };
    let Some(range) = state.cursor.char_range() else {
        return false;
    };
    let (a, b) = (range.primary.index.0, range.secondary.index.0);
    let count = text.chars().count();
    let selection = a.min(b).min(count)..a.max(b).min(count);
    // Shift+Tab first: Tab without Shift would take it too.
    let unindented =
        ui.input_mut(|input| input.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
    let indented = !unindented
        && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
    let cursor = if unindented {
        unindent(text, selection, width)
    } else if indented {
        indent(text, selection, width)
    } else {
        return false;
    };
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(cursor),
        )));
    state.store(ui.ctx(), id);
    true
}

/// The byte of `text` its character `index` starts at, or its end.
fn byte_of(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

/// Replaces the characters `range` of `text` with `with`, returning the character after it.
fn replace(text: &mut String, range: std::ops::Range<usize>, with: &str) -> usize {
    let start = byte_of(text, range.start);
    let end = byte_of(text, range.end);
    text.replace_range(start..end, with);
    range.start + with.chars().count()
}

/// The lines `selection` touches, as characters of `text`: the start of the first, and the start
/// of each after it.
fn line_starts(text: &str, selection: &std::ops::Range<usize>) -> Vec<usize> {
    let before = &text[..byte_of(text, selection.start)];
    let first = selection.start
        - before
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .chars()
            .count();
    let mut starts = vec![first];
    for (i, c) in text.chars().enumerate().skip(first) {
        if i >= selection.end.max(first + 1) {
            break;
        }
        if c == '\n' && i + 1 < selection.end {
            starts.push(i + 1);
        }
    }
    starts
}

/// Tab: spaces to the next stop of `width`, in place of what's selected within a line, or with
/// lines selected, a level more of indentation for each. Returns where the cursor goes.
fn indent(text: &mut String, selection: std::ops::Range<usize>, width: usize) -> usize {
    let starts = line_starts(text, &selection);
    if starts.len() == 1 {
        let column = selection.start - starts[0];
        let spaces = width - column % width;
        return replace(text, selection, &" ".repeat(spaces));
    }
    for &start in starts.iter().rev() {
        replace(text, start..start, &" ".repeat(width));
    }
    selection.end + width * starts.len()
}

/// Shift+Tab: a level less of indentation, `width` spaces or a tab, for each line selected, or
/// the cursor's. Returns where the cursor goes.
fn unindent(text: &mut String, selection: std::ops::Range<usize>, width: usize) -> usize {
    let mut cursor = selection.end;
    for &start in line_starts(text, &selection).iter().rev() {
        let mut after = text.chars().skip(start);
        let taken = match after.next() {
            Some('\t') => 1,
            Some(' ') => 1 + after.take(width - 1).take_while(|c| *c == ' ').count(),
            _ => 0,
        };
        replace(text, start..start + taken, "");
        if start < cursor {
            cursor -= taken.min(cursor - start);
        }
    }
    cursor
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
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[x, y]| self.at(x, y))
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
fn register_paint(lua: &Lua, lent: &Lent) -> mlua::Result<Vec<&'static str>> {
    let mut names = Vec::new();
    lua.register_userdata_type::<Paint>(|registry| {
        let mut methods = PaintMethods {
            registry,
            names: &mut names,
        };
        add_paint_methods(&mut methods);
        let lent = lent.clone();
        methods.add(
            "scene",
            move |paint, (width, height, draw): (u32, u32, Function)| {
                if width == 0 || height == 0 {
                    return Err(mlua::Error::runtime(
                        "a scene needs a positive width and height in pixels",
                    ));
                }
                let (slot, mut scene) = lent.context(|ctx| ctx.gui.take_scene(ctx.frame_stats))?;
                lent.lend(scene.begin(width, height), || Ok(draw.call::<()>(())?))
                    .map_err(mlua::Error::external)?;
                let id = lent.context(|ctx| ctx.queue_gui_scene(slot, scene))?;
                paint.painter.image(
                    id,
                    egui::Rect::from_min_size(paint.origin, paint.size),
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
                Ok(())
            },
        );
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

/// The size in points `options`, the options of a `what`, give along `key`, if they give one.
fn size_option(options: &Table, what: &str, key: &str) -> mlua::Result<Option<f32>> {
    match options.get::<Option<f32>>(key)? {
        Some(size) if !(size.is_finite() && size >= 0.0) => Err(mlua::Error::runtime(format!(
            "a {what}'s {key} is a number of points, not {size}"
        ))),
        size => Ok(size),
    }
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
        let mut registered = register(&Lua::new(), &Lent::default(), Path::new("")).unwrap();
        registered.sort_unstable();
        let mut declared = declared_methods("Ui");
        declared.sort_unstable();
        assert_eq!(declared, registered);
    }

    #[test]
    fn the_definitions_declare_every_method_of_paint() {
        let mut registered = register_paint(&Lua::new(), &Lent::default()).unwrap();
        registered.sort_unstable();
        let mut declared = declared_methods("Paint");
        declared.sort_unstable();
        assert_eq!(declared, registered);
    }

    #[test]
    fn tab_indents_by_spaces_and_shift_tab_unindents() {
        // Within a line, to the next stop, in place of what's selected.
        let mut text = String::from("a\nb\nc");
        assert_eq!(indent(&mut text, 1..1, 4), 4);
        assert_eq!(text, "a   \nb\nc");
        let mut text = String::from("ab\n");
        assert_eq!(indent(&mut text, 1..2, 2), 2);
        assert_eq!(text, "a \n");
        // Across lines, each line a level, and back.
        let mut text = String::from("a\nb\nc");
        let cursor = indent(&mut text, 0..3, 4);
        assert_eq!(text, "    a\n    b\nc");
        assert_eq!(cursor, 11);
        let cursor = unindent(&mut text, 2..cursor, 4);
        assert_eq!(text, "a\nb\nc");
        assert_eq!(cursor, 3);
        // A line with less than a level loses what it has, and one with a tab loses the tab.
        let mut text = String::from("  x");
        assert_eq!(unindent(&mut text, 3..3, 4), 1);
        assert_eq!(text, "x");
        let mut text = String::from("\t  x");
        assert_eq!(unindent(&mut text, 4..4, 2), 3);
        assert_eq!(text, "  x");
        // Characters, not bytes.
        let mut text = String::from("é\nü");
        assert_eq!(indent(&mut text, 0..3, 2), 7);
        assert_eq!(text, "  é\n  ü");
    }

    #[test]
    fn headings_are_in_their_own_font_then_the_bodys() {
        use egui::FontFamily;
        let font = |name: &str| {
            let data = Arc::new(egui::FontData::from_static(&[]));
            Some((format!("{GAME_FONT}{name}"), data))
        };
        let heading = FontFamily::Name(HEADING.into());
        // With none of a game's, headings are in the body's fonts, egui's.
        let definitions = font_definitions(None, None, None);
        let egui = egui::FontDefinitions::default();
        assert_eq!(
            definitions.families[&heading],
            egui.families[&FontFamily::Proportional]
        );
        // A game's go first, headings' before the body's, and egui's after them all.
        let definitions = font_definitions(font("body.ttf"), font("title.otf"), font("code.ttf"));
        let family = |family: &FontFamily| &definitions.families[family][..3];
        assert_eq!(family(&heading)[..2], ["game:title.otf", "game:body.ttf"]);
        assert_eq!(family(&FontFamily::Proportional)[0], "game:body.ttf");
        assert_eq!(family(&FontFamily::Monospace)[0], "game:code.ttf");
        assert_eq!(
            definitions.families[&FontFamily::Proportional][1..],
            egui.families[&FontFamily::Proportional][..]
        );
    }

    #[test]
    fn a_text_edits_tab_is_true_or_a_number_of_spaces() {
        assert_eq!(Tab::read(Value::Nil).unwrap(), Tab::Focus);
        assert_eq!(Tab::read(Value::Boolean(false)).unwrap(), Tab::Focus);
        assert_eq!(Tab::read(Value::Boolean(true)).unwrap(), Tab::Character);
        assert_eq!(Tab::read(Value::Number(4.0)).unwrap(), Tab::Spaces(4));
        for wrong in [Value::Number(0.0), Value::Number(2.5), Value::Number(17.0)] {
            let error = Tab::read(wrong).unwrap_err().to_string();
            assert!(error.contains("from 1 to 16"), "{error}");
        }
        assert!(Tab::read(Value::Integer(3)).is_ok());
    }
}
