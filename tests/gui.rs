//! Tests for egui interfaces drawn over games, driven through `Simulation`.

use std::{cell::Cell, fs, path::Path, rc::Rc};

use wingine::{
    Context, Game, Redraw, WindowConfig,
    graphics::{Canvas, Color},
    gui::{GuiCanvas, egui},
    headless::Simulation,
    input::{Key, MouseButton},
    math::Rect,
};

/// A game with a window of controls, which counts what reaches it and what reaches the window.
#[derive(Default)]
struct Panel {
    button_clicks: u32,
    name: String,
    game_clicks: u32,
    space_presses: u32,
    f3_presses: u32,
    f3_repeats: u32,
    /// What the game was typed.
    typed: String,
    /// How far the game saw the mouse wheel turn, in all.
    turned: [f32; 2],
    /// egui's context, which other threads can ask to repaint.
    egui: Option<egui::Context>,
}

impl Game for Panel {
    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        let input = ctx.input();
        self.game_clicks += u32::from(input.mouse_pressed(MouseButton::Left));
        self.space_presses += u32::from(input.key_pressed(Key::Space));
        self.f3_presses += u32::from(input.key_pressed(Key::F3));
        self.f3_repeats += u32::from(input.key_repeated(Key::F3));
        self.typed.push_str(input.text());
        let [x, y] = input.wheel();
        self.turned[0] += x;
        self.turned[1] += y;
    }

    fn gui(&mut self, _ctx: &mut Context, ui: &mut egui::Ui) {
        self.egui = Some(ui.ctx().clone());
        egui::Window::new("Panel")
            .fixed_pos([10.0, 10.0])
            .show(ui.ctx(), |ui| {
                if ui.button("Press me").clicked() {
                    self.button_clicks += 1;
                }
                ui.add(egui::TextEdit::singleline(&mut self.name).hint_text("Your name"));
            });
    }

    fn draw(&mut self, canvas: &mut Canvas) {
        canvas.clear(Color::rgb8(0x20, 0x20, 0x30));
    }
}

fn simulation(config: WindowConfig) -> Simulation<Panel> {
    let mut simulation =
        Simulation::new(&config, |_| Ok(Panel::default())).expect("a GPU adapter is available");
    // Lays the interface out, so its widgets can be found. egui sizes a new window in its first
    // frame and shows it in the next.
    simulation.run_frames(2);
    simulation
}

fn config() -> WindowConfig {
    WindowConfig {
        width: 400,
        height: 300,
        ..Default::default()
    }
}

#[test]
fn clicking_a_button_presses_it_but_the_game_never_sees_the_click() {
    let mut simulation = simulation(config());
    simulation.click_gui("Press me");
    let panel = simulation.game();
    assert_eq!(panel.button_clicks, 1);
    assert_eq!(panel.game_clicks, 0);
}

#[test]
fn clicks_away_from_the_interface_reach_the_game() {
    let mut simulation = simulation(config());
    simulation.click(MouseButton::Left, 300.0, 250.0);
    simulation.step();
    let panel = simulation.game();
    assert_eq!(panel.game_clicks, 1);
    assert_eq!(panel.button_clicks, 0);
}

#[test]
fn the_wheel_turns_for_the_game_away_from_the_interface_only() {
    let mut simulation = simulation(config());
    let button = simulation.gui_rect("Press me").unwrap();
    simulation.move_mouse(button.x + button.w / 2.0, button.y + button.h / 2.0);
    simulation.scroll(0.0, 2.0);
    simulation.step();
    assert_eq!(simulation.game().turned, [0.0, 0.0]);

    simulation.move_mouse(300.0, 250.0);
    simulation.scroll(0.0, 2.0);
    simulation.scroll(-0.5, 0.0);
    simulation.step();
    simulation.step();
    assert_eq!(simulation.game().turned, [-0.5, 2.0]);
}

#[test]
fn typing_goes_to_the_focused_field_and_its_keys_not_to_the_game() {
    let mut simulation = simulation(config());
    simulation.click_gui("Your name");
    simulation.type_text("Ada");
    simulation.tap(Key::Space);
    simulation.key_down(Key::LeftShift);
    simulation.tap(Key::L);
    simulation.key_up(Key::LeftShift);
    simulation.step();
    let panel = simulation.game();
    assert_eq!(panel.name, "Ada L");
    assert_eq!(panel.space_presses, 0);
    assert_eq!(panel.typed, "");

    // Escape lets go of the field, and the game has the keyboard again.
    simulation.tap(Key::Escape);
    simulation.tap(Key::Space);
    simulation.type_text("?");
    simulation.step();
    assert_eq!(simulation.game().space_presses, 1);
    assert_eq!(simulation.game().typed, " ?");
}

#[test]
fn held_keys_repeat_in_a_focused_field() {
    let mut simulation = simulation(config());
    simulation.click_gui("Your name");
    simulation.type_text("Ada Lovelace");
    simulation.step();
    // Pressed, then repeated twice: half a second later, and two frames after that.
    simulation.hold_frames(Key::Backspace, 33);
    simulation.step();
    assert_eq!(simulation.game().name, "Ada Lovel");
}

#[test]
fn widgets_are_found_where_they_are_in_canvas_pixels() {
    let window = simulation(config());
    let button = window.gui_rect("Press me").unwrap();
    // Inside the window at (10, 10), below its title.
    assert!(button.x > 10.0 && button.y > 20.0, "{button:?}");
    assert!(window.gui_rect("Nothing like this").is_none());
    drop(window);

    // Through a fixed resolution's scaling: the 200x150 canvas is drawn at 2x.
    let mut scaled = simulation(WindowConfig {
        resolution: wingine::graphics::Resolution::Fixed {
            width: 200,
            height: 150,
        },
        ..config()
    });
    let scaled_button = scaled.gui_rect("Press me").unwrap();
    assert!((scaled_button.x - button.x / 2.0).abs() < 0.01);
    assert!((scaled_button.w - button.w / 2.0).abs() < 0.01);
    scaled.click_gui("Press me");
    assert_eq!(scaled.game().button_clicks, 1);
}

#[test]
fn the_debug_key_shows_frame_stats_and_never_reaches_the_game() {
    let mut simulation = simulation(WindowConfig {
        debug_key: Some(Key::F3),
        ..config()
    });
    assert!(simulation.gui_rect("Wingine").is_none());
    simulation.tap(Key::F3);
    simulation.step();
    assert!(simulation.gui_rect("Wingine").is_some());
    assert!(simulation.gui_rect("Draw calls").is_some());
    simulation.tap(Key::F3);
    simulation.step();
    assert!(simulation.gui_rect("Wingine").is_none());
    assert_eq!(simulation.game().f3_presses, 0);

    // Held, it shows the window once, and its repeats don't reach the game either.
    simulation.hold(Key::F3, 1.0);
    assert!(simulation.gui_rect("Wingine").is_some());
    assert_eq!(simulation.game().f3_repeats, 0);
}

#[test]
fn the_debug_key_goes_to_a_focused_field_instead() {
    let mut simulation = simulation(WindowConfig {
        debug_key: Some(Key::F3),
        ..config()
    });
    simulation.click_gui("Your name");
    simulation.tap(Key::F3);
    simulation.step();
    assert!(simulation.gui_rect("Wingine").is_none());
    assert_eq!(simulation.game().f3_presses, 0);

    simulation.tap(Key::Escape);
    simulation.tap(Key::F3);
    simulation.step();
    assert!(simulation.gui_rect("Wingine").is_some());
}

/// A game with no interface of its own, where the debug window is all there is.
#[derive(Default)]
struct Plain {
    clicks: u32,
}

impl Game for Plain {
    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        self.clicks += u32::from(ctx.input().mouse_pressed(MouseButton::Left));
    }

    fn draw(&mut self, canvas: &mut Canvas) {
        canvas.clear(Color::BLACK);
    }
}

#[test]
fn a_closed_debug_window_keeps_no_clicks_from_the_game() {
    let config = WindowConfig {
        debug_key: Some(Key::F3),
        ..config()
    };
    let mut simulation = Simulation::new(&config, |_| Ok(Plain::default())).unwrap();
    simulation.step();
    simulation.tap(Key::F3);
    simulation.run_frames(2);
    let window = simulation
        .gui_rect("Wingine")
        .expect("the debug window is open");
    let [x, y] = [window.x + window.w / 2.0, window.y + window.h / 2.0];
    simulation.click(MouseButton::Left, x, y);
    assert_eq!(simulation.game().clicks, 0);

    simulation.tap(Key::F3);
    simulation.run_frames(2);
    simulation.click(MouseButton::Left, x, y);
    assert_eq!(simulation.game().clicks, 1);
}

/// Runs the frames `simulation` wants until it wants none, and how many that was.
///
/// # Panics
///
/// If it still wants frames after a second's worth.
fn settle<G: Game>(simulation: &mut Simulation<G>) -> u32 {
    let mut frames = 0;
    while simulation.next_frame_in().is_some() {
        assert!(frames < 60, "frames are still wanted");
        simulation.step();
        frames += 1;
    }
    frames
}

#[test]
fn an_interface_wants_frames_only_while_it_changes() {
    let mut simulation = simulation(WindowConfig {
        redraw: Redraw::OnDemand,
        ..config()
    });
    // Its window fades in, then it waits for input.
    assert!(simulation.next_frame_in().is_some());
    let frames = settle(&mut simulation);
    assert!(frames > 0, "{frames}");

    // A button lights up under the pointer, then the interface settles again.
    let button = simulation.gui_rect("Press me").unwrap();
    simulation.move_mouse(button.x + button.w / 2.0, button.y + button.h / 2.0);
    simulation.step();
    assert!(simulation.next_frame_in().is_some());
    settle(&mut simulation);

    // Another thread asks egui to repaint.
    let egui = simulation.game().egui.clone().unwrap();
    std::thread::spawn(move || egui.request_repaint())
        .join()
        .unwrap();
    assert_eq!(simulation.next_frame_in(), Some(0.0));
}

/// A shop window, whose button counts its clicks where a test can read them after a scripted run
/// has ended the game.
struct Shop(Rc<Cell<u32>>);

impl Game for Shop {
    fn update(&mut self, _ctx: &mut Context, _dt: f32) {}

    fn gui(&mut self, _ctx: &mut Context, ui: &mut egui::Ui) {
        egui::Window::new("Shop")
            .fixed_pos([10.0, 10.0])
            .show(ui.ctx(), |ui| {
                if ui.button("Buy a sword").clicked() {
                    self.0.set(self.0.get() + 1);
                }
            });
    }

    fn draw(&mut self, _canvas: &mut Canvas) {}
}

#[test]
fn a_script_clicks_widgets_by_label() {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gui-script");
    fs::create_dir_all(&directory).unwrap();
    let script = directory.join("shop.txt");
    // The window shows from its second frame.
    fs::write(
        &script,
        "wait 2\nclick-gui Buy a sword\nclick-gui Buy a sword\n",
    )
    .unwrap();
    let bought = Rc::new(Cell::new(0));
    let shop = Shop(bought.clone());
    wingine::script::run(&config(), |_| Ok(shop), &script).unwrap();
    assert_eq!(bought.get(), 2);

    fs::write(&script, "click-gui Buy a sword\n").unwrap();
    let err = wingine::script::run(&config(), |_| Ok(Shop(bought.clone())), &script).unwrap_err();
    assert!(
        err.to_string()
            .contains("line 1: the interface had no widget labelled `Buy a sword`"),
        "{err}"
    );
}

#[test]
fn without_a_debug_key_the_game_sees_every_key() {
    let mut simulation = simulation(config());
    simulation.tap(Key::F3);
    assert_eq!(simulation.game().f3_presses, 1);
    assert!(simulation.gui_rect("Wingine").is_none());
}

/// A canvas shown in egui wider than the GPU's largest texture renders at that size instead, and
/// is shown at its own.
#[test]
fn canvases_wider_than_the_gpu_takes_render_smaller() {
    struct Wide {
        canvas: GuiCanvas,
        shown: Option<egui::Vec2>,
    }
    impl Game for Wide {
        fn gui(&mut self, ctx: &mut Context, ui: &mut egui::Ui) {
            let canvas = self.canvas.begin(1 << 20, 4);
            canvas.fill_rect(Rect::new(0.0, 0.0, 8.0, 4.0), Color::RED);
            let texture = self.canvas.render(ctx);
            self.shown = Some(texture.size);
            ui.add(egui::Image::new(texture).max_width(100.0));
        }

        fn draw(&mut self, canvas: &mut Canvas) {
            canvas.clear(Color::BLACK);
        }
    }

    let mut simulation = Simulation::new(&config(), |_| {
        Ok(Wide {
            canvas: GuiCanvas::new(),
            shown: None,
        })
    })
    .expect("a GPU adapter is available");
    simulation.run_frames(1);
    assert_eq!(simulation.game().shown, Some(egui::vec2(1048576.0, 4.0)));
}
