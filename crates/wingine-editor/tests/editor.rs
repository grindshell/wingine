//! Tests for the editor, driven through `Simulation` as the examples' games are: by clicking its
//! widgets by their labels, typing, and pressing keys.

use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use wingine::{
    WindowConfig,
    headless::Simulation,
    input::{Key, MouseButton},
    scene::{KindDef, SceneFile},
};
use wingine_editor::{Editor, docks::output::Level, edit, viewport::Tool};

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// A copy of the example game `name` in a folder of the test's own, to change.
fn copy_of(name: &str, test: &str) -> PathBuf {
    let to = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("editor")
        .join(test);
    let _ = fs::remove_dir_all(&to);
    copy_folder(&examples().join(name), &to);
    to
}

fn copy_folder(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_folder(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn editor(folder: Option<PathBuf>) -> Simulation<Editor> {
    let config = WindowConfig {
        width: 1600,
        height: 900,
        ..Default::default()
    };
    let mut simulation =
        Simulation::new(&config, |_| Ok(Editor::new(folder))).expect("a GPU adapter is available");
    simulation.run_frames(2);
    simulation
}

/// The editor with the garden's scene open.
fn garden(folder: PathBuf) -> Simulation<Editor> {
    let mut simulation = editor(Some(folder));
    simulation
        .game_mut()
        .open_scene("garden.scene.luau")
        .unwrap();
    simulation.run_frames(2);
    simulation
}

fn file(simulation: &Simulation<Editor>) -> &SceneFile {
    simulation.game().document().unwrap().file()
}

/// The node of the open scene at `names`.
fn node<'a>(simulation: &'a Simulation<Editor>, names: &str) -> &'a wingine::scene::NodeDef {
    let file = file(simulation);
    edit::node(file, &edit::path_of(file, names)).unwrap()
}

fn selected_name(simulation: &Simulation<Editor>) -> Option<String> {
    let document = simulation.game().document()?;
    let path = document.selected.as_ref()?;
    edit::name_path(document.file(), path)
}

/// Presses `key` with Control held.
fn control(simulation: &mut Simulation<Editor>, key: Key) {
    simulation.key_down(Key::LeftControl);
    simulation.tap(key);
    simulation.key_up(Key::LeftControl);
    simulation.step();
}

fn double_click_gui(simulation: &mut Simulation<Editor>, label: &str) {
    simulation.click_gui(label);
    simulation.click_gui(label);
}

#[test]
fn double_clicking_a_scene_opens_it_in_a_tab() {
    let mut simulation = editor(Some(examples().join("garden")));
    assert!(simulation.gui_rect("garden.scene.luau").is_some());
    double_click_gui(&mut simulation, "garden.scene.luau");
    simulation.step();
    let document = simulation.game().document().expect("the scene is open");
    assert_eq!(document.path, "garden.scene.luau");
    assert!(document.preview.is_some());
    assert!(document.error.is_none(), "{:?}", document.error);
    // Its nodes are in the Scene dock.
    assert!(simulation.gui_rect("duck").is_some());
}

#[test]
fn every_example_scene_opens_without_errors() {
    for game in ["garden", "orchard", "rpg", "shmup"] {
        let mut simulation = editor(Some(examples().join(game)));
        let scenes = simulation
            .game()
            .project()
            .unwrap()
            .files_ending(".scene.luau");
        assert!(!scenes.is_empty());
        for scene in scenes {
            simulation.game_mut().open_scene(&scene).unwrap();
            simulation.step();
            let document = simulation.game().document().unwrap();
            assert!(document.preview.is_some(), "{game}/{scene} shows nothing");
            assert_eq!(document.error, None, "{game}/{scene}");
        }
        assert!(
            !simulation
                .game()
                .output()
                .lines()
                .iter()
                .any(|line| line.level == Level::Error),
            "{game}: {:?}",
            simulation.game().output().lines()
        );
    }
}

#[test]
fn clicking_a_node_in_the_tree_shows_it_in_the_inspector() {
    let mut simulation = garden(examples().join("garden"));
    simulation.click_gui("duck");
    assert_eq!(selected_name(&simulation).as_deref(), Some("pond/duck"));
    simulation.step();
    // Its sprite's fields.
    assert!(simulation.gui_rect("sheet").is_some());
    assert!(simulation.gui_rect("tint").is_some());

    simulation.click_gui("garden");
    assert_eq!(selected_name(&simulation).as_deref(), Some(""));
    simulation.step();
    assert!(simulation.gui_rect("Sheets").is_some());
}

#[test]
fn typing_a_field_changes_the_scene_and_undo_restores_it() {
    let mut simulation = garden(examples().join("garden"));
    simulation.click_gui("duck");
    let x = node(&simulation, "pond/duck").x;
    // The field after the label `x`.
    let field = simulation.gui_rects("x")[1];
    simulation.click(
        MouseButton::Left,
        field.x + field.w / 2.0,
        field.y + field.h / 2.0,
    );
    simulation.step();
    control(&mut simulation, Key::A);
    simulation.type_text("123");
    simulation.tap(Key::Enter);
    simulation.run_frames(2);
    assert_eq!(node(&simulation, "pond/duck").x, 123.0);
    assert!(simulation.game().document().unwrap().is_dirty());
    assert!(simulation.gui_rect("garden.scene.luau*").is_some());

    control(&mut simulation, Key::Z);
    assert_eq!(node(&simulation, "pond/duck").x, x);
    assert!(!simulation.game().document().unwrap().is_dirty());
    control(&mut simulation, Key::Y);
    assert_eq!(node(&simulation, "pond/duck").x, 123.0);
}

#[test]
fn dragging_with_the_move_tool_moves_the_selected_node_in_one_change() {
    let mut simulation = garden(examples().join("garden"));
    simulation.click_gui("duck");
    simulation.tap(Key::W);
    assert_eq!(simulation.game().tool(), Tool::Move);
    let start = node(&simulation, "pond/duck").clone();

    // Across the viewport, from anywhere: the Move tool moves what's selected.
    simulation.move_mouse(700.0, 400.0);
    simulation.mouse_down(MouseButton::Left);
    simulation.step();
    for x in [710.0, 720.0, 730.0, 740.0] {
        simulation.move_mouse(x, 410.0);
        simulation.step();
    }
    simulation.mouse_up(MouseButton::Left);
    simulation.run_frames(2);
    // 40 points right and 10 down, at the view's zoom.
    let zoom = simulation.game().document().unwrap().view.unwrap().zoom;
    let moved = node(&simulation, "pond/duck");
    assert!(
        (moved.x - (start.x + 40.0 / zoom)).abs() < 0.01,
        "{}",
        moved.x
    );
    assert!(
        (moved.y - (start.y + 10.0 / zoom)).abs() < 0.01,
        "{}",
        moved.y
    );

    control(&mut simulation, Key::Z);
    assert_eq!(node(&simulation, "pond/duck").x, start.x);
}

#[test]
fn clicking_in_the_viewport_selects_what_is_there() {
    let mut simulation = garden(examples().join("garden"));
    let point_on = |simulation: &Simulation<Editor>, names: &str| {
        let document = simulation.game().document().unwrap();
        let scene = document.preview.as_ref().unwrap();
        let id = document
            .live_node(&edit::path_of(document.file(), names))
            .unwrap();
        let bounds = scene.bounds(id).unwrap();
        // Just inside its top-left corner.
        simulation
            .game()
            .scene_to_window([bounds.x + 1.0, bounds.y + 1.0])
            .unwrap()
    };
    // The water, under the shore, an area that doesn't draw: what can be seen comes first.
    let [x, y] = point_on(&simulation, "pond/water");
    simulation.click(MouseButton::Left, x, y);
    simulation.step();
    assert_eq!(selected_name(&simulation).as_deref(), Some("pond/water"));

    // The duck swims on the water, so it's picked over it.
    let [x, y] = point_on(&simulation, "pond/duck");
    simulation.click(MouseButton::Left, x + 8.0, y + 8.0);
    simulation.step();
    assert_eq!(selected_name(&simulation).as_deref(), Some("pond/duck"));

    // Clicking again in the same place picks what's under it: what can be seen, down to the
    // ground, then the areas, then round again.
    let [x, y] = point_on(&simulation, "pond/water");
    let mut picked = Vec::new();
    for _ in 0..5 {
        simulation.click(MouseButton::Left, x, y);
        simulation.step();
        picked.push(selected_name(&simulation).unwrap());
    }
    assert_eq!(
        picked,
        [
            "pond/water",
            "ground",
            "pond/shore",
            "pond/edge",
            "pond/water"
        ]
    );

    // Nothing there, just off the canvas's right edge: nothing selected.
    let [x, y] = simulation.game().scene_to_window([321.0, 90.0]).unwrap();
    simulation.click(MouseButton::Left, x, y);
    simulation.step();
    assert_eq!(selected_name(&simulation), None);
}

#[test]
fn adding_renaming_and_deleting_nodes() {
    let mut simulation = garden(examples().join("garden"));
    simulation.click_gui("garden");
    simulation.click_gui("➕");
    // Menus are sized in their first frame, and shown in the next.
    simulation.step();
    simulation.click_gui("■ rect");
    assert_eq!(selected_name(&simulation).as_deref(), Some("rect"));
    assert!(matches!(node(&simulation, "rect").kind, KindDef::Rect(_)));

    simulation.tap(Key::F2);
    simulation.step();
    control(&mut simulation, Key::A);
    simulation.type_text("banner");
    simulation.tap(Key::Enter);
    simulation.run_frames(2);
    assert_eq!(selected_name(&simulation).as_deref(), Some("banner"));

    control(&mut simulation, Key::D);
    assert_eq!(selected_name(&simulation).as_deref(), Some("banner2"));
    simulation.tap(Key::Delete);
    simulation.step();
    let names: Vec<_> = file(&simulation)
        .children
        .iter()
        .map(|node| node.name.as_str())
        .collect();
    assert!(names.contains(&"banner") && !names.contains(&"banner2"));

    // Each was a change of its own: adding, renaming, duplicating and deleting.
    for _ in 0..4 {
        control(&mut simulation, Key::Z);
    }
    assert!(!simulation.game().document().unwrap().is_dirty());
}

#[test]
fn saving_writes_the_canonical_layout() {
    let folder = copy_of("garden", "saving");
    let mut simulation = garden(folder.clone());
    let path = edit::path_of(file(&simulation), "pond/duck");
    let document = simulation.game_mut().document_mut().unwrap();
    edit::node_mut(document.edit(), &path).unwrap().x += 4.0;
    document.end_change();
    control(&mut simulation, Key::S);

    let written = fs::read_to_string(folder.join("garden.scene.luau")).unwrap();
    let document = simulation.game().document().unwrap();
    assert!(!document.is_dirty());
    assert_eq!(written, document.file().to_luau().unwrap());
    let original = fs::read_to_string(examples().join("garden/garden.scene.luau")).unwrap();
    // One line changed: the duck's x.
    let changed = written
        .lines()
        .zip(original.lines())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(changed, 1);
}

#[test]
fn closing_with_unsaved_changes_asks_first() {
    let mut simulation = garden(examples().join("garden"));
    let document = simulation.game_mut().document_mut().unwrap();
    edit::node_mut(document.edit(), &[0]).unwrap().x += 1.0;
    document.end_change();
    assert!(!simulation.request_close());
    simulation.run_frames(2);
    assert!(simulation.gui_rect("Unsaved Changes").is_some());
    simulation.click_gui("Cancel");
    assert!(!simulation.has_quit());
    // Once it has faded out.
    simulation.run_for(0.5);
    assert!(simulation.gui_rect("Unsaved Changes").is_none());

    assert!(!simulation.request_close());
    simulation.run_frames(2);
    simulation.click_gui("Don't Save");
    assert!(simulation.has_quit());
}

#[test]
fn playing_runs_the_game_in_its_own_process() {
    let script = Path::new(env!("CARGO_TARGET_TMPDIR")).join("editor/play.txt");
    fs::create_dir_all(script.parent().unwrap()).unwrap();
    fs::write(&script, "wait 0.1s\n").unwrap();
    let mut simulation = editor(Some(examples().join("pong")));
    simulation.game_mut().set_runner(
        env!("CARGO_BIN_EXE_wingine-editor"),
        vec![("WINGINE_SCRIPT".into(), script.into())],
    );
    simulation.click_gui("▶ Play");
    assert!(simulation.game().is_playing());
    for _ in 0..600 {
        if !simulation.game().is_playing() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
        simulation.step();
    }
    let lines = simulation.game().output().lines();
    assert!(!simulation.game().is_playing(), "{lines:?}");
    assert!(
        lines.iter().any(|line| line.text == "The game ended."),
        "{lines:?}"
    );
}
