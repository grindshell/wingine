//! Tests for the editor, driven through `Simulation` as the examples' games are: by clicking its
//! widgets by their labels, typing, and pressing keys.

use std::{
    f32::consts::FRAC_PI_2,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime},
};

use wingine::{
    Redraw, WindowConfig,
    headless::{Headless, Simulation},
    input::{Key, MouseButton},
    scene::{
        Assets, Clip, Interpolation, KeyValue, Keys, KindDef, MeshDef, Placement3, Property,
        SceneFile, Shape, TileLayerDef, Value,
    },
};
use wingine_editor::{
    Editor, clips,
    docks::{output::Level, tiles::TileTool},
    document::{Timeline, View},
    edit,
    editor::{BottomTab, MainScreen},
    lsp::{self, Severity},
    project::AssetCache,
    viewport::Tool,
};

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
        redraw: Redraw::OnDemand,
        ..Default::default()
    };
    let mut simulation = Simulation::new(&config, |ctx| Ok(Editor::new(ctx, folder)))
        .expect("a GPU adapter is available");
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

/// Runs the frames the editor wants until it wants none, as its window would before it waits
/// for input, and how many that was.
///
/// # Panics
///
/// If it still wants frames after two seconds' worth.
fn settle(simulation: &mut Simulation<Editor>) -> u32 {
    let mut frames = 0;
    while simulation.next_frame_in().is_some() {
        assert!(frames < 120, "the editor still wants frames");
        simulation.step();
        frames += 1;
    }
    frames
}

/// Runs frames as the editor's window would, until `done` is true of the editor: when it wants
/// one, and otherwise not until another thread wakes it, as luau-lsp, a game played and the
/// project's watcher do when there's something new. Frames it wants later run at once, since a
/// simulation's time is its own.
///
/// # Panics
///
/// After 30 seconds.
fn wait_for(simulation: &mut Simulation<Editor>, what: &str, done: impl Fn(&Editor) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done(simulation.game()) {
        let lines = simulation.game().output().lines();
        let script = simulation.game().scripts().script();
        let text = script.map(|script| (&script.text, &script.diagnostics));
        assert!(
            Instant::now() < deadline,
            "waited for {what}: {lines:?}\n{text:#?}"
        );
        if simulation.next_frame_in().is_some() {
            simulation.step();
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }
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

/// Presses the left button at `from`, moves to `to` over a few frames, as a hand does, and lets
/// go there.
fn drag(simulation: &mut Simulation<Editor>, from: [f32; 2], to: [f32; 2]) {
    simulation.move_mouse(from[0], from[1]);
    simulation.mouse_down(MouseButton::Left);
    simulation.step();
    for i in 1..=6 {
        let t = i as f32 / 6.0;
        simulation.move_mouse(
            from[0] + (to[0] - from[0]) * t,
            from[1] + (to[1] - from[1]) * t,
        );
        simulation.step();
    }
    simulation.mouse_up(MouseButton::Left);
    simulation.run_frames(2);
}

fn middle(rect: wingine::math::Rect) -> [f32; 2] {
    [rect.x + rect.w / 2.0, rect.y + rect.h / 2.0]
}

/// The editor with the RPG's village open, and its layer `floor` selected, in a copy of the game
/// of the test's own.
fn village(test: &str) -> Simulation<Editor> {
    let mut simulation = editor(Some(copy_of("rpg", test)));
    simulation
        .game_mut()
        .open_scene("maps/village.scene.luau")
        .unwrap();
    simulation.run_frames(2);
    select(&mut simulation, "map/floor");
    simulation
}

fn select(simulation: &mut Simulation<Editor>, names: &str) {
    let path = edit::path_of(file(simulation), names);
    simulation
        .game_mut()
        .document_mut()
        .unwrap()
        .select(Some(path));
    simulation.run_frames(2);
}

fn floor(simulation: &Simulation<Editor>) -> &TileLayerDef {
    match &node(simulation, "map/floor").kind {
        KindDef::TileLayer(layer) => layer,
        _ => panic!("the floor is a tile layer"),
    }
}

/// The tile in `cell` of the village's floor.
fn floor_cell(simulation: &Simulation<Editor>, cell: [i32; 2]) -> Option<String> {
    edit::cell(floor(simulation), cell).map(str::to_owned)
}

/// The middle of `cell` of the village's map, in the window.
fn cell_middle(simulation: &Simulation<Editor>, [column, row]: [i32; 2]) -> [f32; 2] {
    simulation
        .game()
        .scene_to_window([column as f32 * 16.0 + 8.0, row as f32 * 16.0 + 8.0])
        .unwrap()
}

/// Clicks the tile `name` in the Tiles panel's palette, at the bottom, rather than its line in the
/// layer's legend in the Inspector.
fn choose_tile(simulation: &mut Simulation<Editor>, name: &str) {
    let rects = simulation.gui_rects(name);
    let tile = rects
        .iter()
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .unwrap_or_else(|| panic!("no tile `{name}`"));
    let [x, y] = middle(*tile);
    simulation.click(MouseButton::Left, x, y);
    simulation.step();
    assert_eq!(simulation.game().tiles().tile.as_deref(), Some(name));
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
    for game in [
        "garden",
        "orchard",
        "rpg",
        "shmup",
        "cellar",
        "course",
        "lettering",
    ] {
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
fn the_inspector_chooses_the_font_text_is_in() {
    let mut simulation = editor(Some(copy_of("rpg", "fonts_chosen")));
    simulation
        .game_mut()
        .open_scene("title.scene.luau")
        .unwrap();
    simulation.run_frames(2);
    select(&mut simulation, "tagline");
    let height = |simulation: &Simulation<Editor>| {
        let scene = simulation
            .game()
            .document()
            .unwrap()
            .preview
            .as_ref()
            .unwrap();
        let id = scene.find(scene.root(), "tagline").unwrap();
        scene.bounds(id).unwrap().h
    };
    assert_eq!(height(&simulation), 10.0, "the built-in font's line");

    // The font's choice, after its label, offers the built-in font and the game's font files.
    let choices = simulation.gui_rects("font");
    let [x, y] = middle(*choices.last().expect("a choice of font"));
    simulation.click(MouseButton::Left, x, y);
    simulation.run_frames(2);
    let offered = simulation.gui_rects("title.font.luau");
    let title = offered.iter().find(|rect| rect.x > x - 100.0);
    let [x, y] = middle(*title.expect("the title's font is offered"));
    simulation.click(MouseButton::Left, x, y);
    simulation.run_frames(2);

    let KindDef::Text(text) = &node(&simulation, "tagline").kind else {
        panic!("the tagline is text");
    };
    assert_eq!(text.font.as_deref(), Some("title.font.luau"));
    assert_eq!(height(&simulation), 15.0, "the title font's line");
    control(&mut simulation, Key::Z);
    let KindDef::Text(text) = &node(&simulation, "tagline").kind else {
        panic!("the tagline is text");
    };
    assert_eq!(text.font, None, "back to the built-in font");
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
fn painting_a_tile_layer_in_strokes() {
    let mut simulation = village("painting");
    // Selecting the layer shows its tileset's tiles.
    assert_eq!(simulation.game().bottom_tab(), BottomTab::Tiles);
    choose_tile(&mut simulation, "water");

    // A stroke along the row, across grass and the path.
    let (from, to) = (
        cell_middle(&simulation, [2, 1]),
        cell_middle(&simulation, [5, 1]),
    );
    drag(&mut simulation, from, to);
    for column in 2..=5 {
        assert_eq!(
            floor_cell(&simulation, [column, 1]).as_deref(),
            Some("water")
        );
    }
    assert_eq!(floor_cell(&simulation, [6, 1]).as_deref(), Some("grass"));
    // The preview has it too, without the scene being built again.
    let document = simulation.game().document().unwrap();
    let layer = document
        .live_node(&edit::path_of(document.file(), "map/floor"))
        .unwrap();
    let scene = document.preview.as_ref().unwrap();
    assert_eq!(scene.tile(layer, [3, 1]), Some("water"));

    // The right button empties a cell, and Control picks the tile in one.
    let [x, y] = cell_middle(&simulation, [2, 1]);
    simulation.click(MouseButton::Right, x, y);
    simulation.step();
    assert_eq!(floor_cell(&simulation, [2, 1]), None);
    let [x, y] = cell_middle(&simulation, [6, 1]);
    simulation.key_down(Key::LeftControl);
    simulation.click(MouseButton::Left, x, y);
    simulation.key_up(Key::LeftControl);
    simulation.step();
    assert_eq!(simulation.game().tiles().tile.as_deref(), Some("grass"));
    assert_eq!(floor_cell(&simulation, [6, 1]).as_deref(), Some("grass"));

    // Each stroke is a change of its own.
    control(&mut simulation, Key::Z);
    assert_eq!(floor_cell(&simulation, [2, 1]).as_deref(), Some("water"));
    control(&mut simulation, Key::Z);
    assert_eq!(floor_cell(&simulation, [3, 1]).as_deref(), Some("grass"));
    assert!(!simulation.game().document().unwrap().is_dirty());
}

#[test]
fn filling_rectangles_and_regions_of_tiles() {
    let mut simulation = village("filling");
    choose_tile(&mut simulation, "water");
    simulation.click_gui("Rect");
    assert_eq!(simulation.game().tiles().tool, TileTool::Rect);
    // Dragged out from its bottom-right corner to its top-left.
    let (from, to) = (
        cell_middle(&simulation, [3, 4]),
        cell_middle(&simulation, [1, 3]),
    );
    drag(&mut simulation, from, to);
    let pond: Vec<[i32; 2]> = (3..=4)
        .flat_map(|row| (1..=3).map(move |column| [column, row]))
        .collect();
    for &cell in &pond {
        assert_eq!(floor_cell(&simulation, cell).as_deref(), Some("water"));
    }
    assert_eq!(floor_cell(&simulation, [4, 3]).as_deref(), Some("grass"));

    // Filling the water fills what joins it, and nothing else.
    simulation.click_gui("Fill");
    choose_tile(&mut simulation, "path");
    let mut joined = edit::flood(floor(&simulation), [2, 3], [40, 26]);
    joined.sort();
    let [x, y] = cell_middle(&simulation, [2, 3]);
    simulation.click(MouseButton::Left, x, y);
    simulation.step();
    for &cell in &joined {
        assert_eq!(floor_cell(&simulation, cell).as_deref(), Some("path"));
    }
    assert!(pond.iter().all(|cell| joined.contains(cell)));
    assert_eq!(floor_cell(&simulation, [4, 3]).as_deref(), Some("grass"));
}

#[test]
fn dragging_a_handle_resizes_a_tilemap() {
    let mut simulation = village("resizing");
    select(&mut simulation, "map");
    // Zoomed out, with the whole map in view: 40 columns by 26 rows of 16.
    simulation.game_mut().document_mut().unwrap().view = Some(View {
        center: [320.0, 208.0],
        zoom: 1.0,
    });
    simulation.step();
    let from = simulation.game().scene_to_window([640.0, 208.0]).unwrap();
    // Two columns wider.
    drag(&mut simulation, from, [from[0] + 33.0, from[1]]);
    let KindDef::Tilemap(map) = &node(&simulation, "map").kind else {
        panic!("the map is a tilemap");
    };
    assert_eq!([map.columns, map.rows], [42, 26]);
    assert!(floor(&simulation).tiles.iter().all(|row| row.len() == 42));
    assert_eq!(simulation.game().document().unwrap().error, None);

    control(&mut simulation, Key::Z);
    assert!(!simulation.game().document().unwrap().is_dirty());
}

#[test]
fn dragging_a_scene_in_places_it_where_it_is_dropped() {
    let mut simulation = garden(examples().join("garden"));
    let row = middle(simulation.gui_rect("flower.scene.luau").unwrap());
    let to = simulation.game().scene_to_window([100.0, 50.0]).unwrap();
    drag(&mut simulation, row, to);

    let document = simulation.game().document().unwrap();
    let path = document.selected.clone().expect("the new node is selected");
    let placed = edit::node(document.file(), &path).unwrap();
    assert_eq!(path.len(), 1, "it's at the top of the scene");
    assert_eq!(placed.name, "flower");
    assert!(
        matches!(&placed.kind, KindDef::Instance(def) if def.scene == "flower.scene.luau"),
        "{:?}",
        placed.kind
    );
    let zoom = document.view.unwrap().zoom;
    assert!((placed.x - 100.0).abs() <= 1.0 / zoom, "{}", placed.x);
    assert!((placed.y - 50.0).abs() <= 1.0 / zoom, "{}", placed.y);
}

#[test]
fn the_inspector_overrides_nodes_inside_instances_and_reverts_them() {
    let mut simulation = garden(examples().join("garden"));
    select(&mut simulation, "flowers/2");
    let overrides = |simulation: &Simulation<Editor>| match &node(simulation, "flowers/2").kind {
        KindDef::Instance(def) => def.overrides.get("flower").cloned().unwrap_or_default(),
        _ => panic!("the flower is an instance"),
    };
    // The instance lists what it overrides, and clicking one selects it.
    simulation.click_gui("flower");
    assert_eq!(
        simulation.game().document().unwrap().inner(),
        Some("flower")
    );
    simulation.run_frames(2);

    // Its cell and tint are marked, each with a button to put it back; the cell's is first.
    let reverts = simulation.gui_rects("⟲");
    assert_eq!(reverts.len(), 2);
    let [x, y] = middle(reverts[0]);
    simulation.click(MouseButton::Left, x, y);
    simulation.run_frames(2);
    let tint: Vec<String> = overrides(&simulation).into_keys().collect();
    assert_eq!(tint, ["tint"]);

    // Changing a field overrides it.
    let field = simulation.gui_rects("x")[1];
    let [x, y] = middle(field);
    simulation.click(MouseButton::Left, x, y);
    simulation.step();
    control(&mut simulation, Key::A);
    simulation.type_text("5");
    simulation.tap(Key::Enter);
    simulation.run_frames(2);
    assert_eq!(overrides(&simulation)["x"], Value::Number(5.0));

    simulation.click_gui("Revert All");
    assert!(
        matches!(&node(&simulation, "flowers/2").kind, KindDef::Instance(def) if def.overrides.is_empty())
    );
    // Each was a change of its own.
    for _ in 0..3 {
        control(&mut simulation, Key::Z);
    }
    assert!(!simulation.game().document().unwrap().is_dirty());
}

/// The editor with the cellar's level open, in a copy of the game of the test's own.
fn cellar(test: &str) -> (Simulation<Editor>, PathBuf) {
    let folder = copy_of("cellar", test);
    let mut simulation = editor(Some(folder.clone()));
    simulation
        .game_mut()
        .open_scene("cellar.scene.luau")
        .unwrap();
    simulation.run_frames(2);
    (simulation, folder)
}

/// The middle of the box around what the open scene's node at `names` shows, in its view's
/// world.
fn center_3d(simulation: &Simulation<Editor>, names: &str) -> [f32; 3] {
    let document = simulation.game().document().unwrap();
    let id = document
        .live_node(&edit::path_of(document.file(), names))
        .unwrap();
    document
        .preview
        .as_ref()
        .unwrap()
        .bounds_3d(id)
        .unwrap()
        .center()
}

/// Where `point` in the world of the open scene's 3D view at `view` shows in the window.
fn world_to_window(simulation: &Simulation<Editor>, view: &str, point: [f32; 3]) -> [f32; 2] {
    let document = simulation.game().document().unwrap();
    let scene = document.preview.as_ref().unwrap();
    let id = document
        .live_node(&edit::path_of(document.file(), view))
        .unwrap();
    let at = scene
        .project(id, point)
        .expect("the point shows in the view");
    simulation.game().scene_to_window(at).unwrap()
}

/// Types `text` into the number field named `name` in the Inspector, the one after its label.
fn set_field(simulation: &mut Simulation<Editor>, name: &str, text: &str) {
    let fields = simulation.gui_rects(name);
    let field = fields.last().unwrap_or_else(|| panic!("no field `{name}`"));
    let [x, y] = middle(*field);
    simulation.click(MouseButton::Left, x, y);
    simulation.step();
    control(simulation, Key::A);
    simulation.type_text(text);
    simulation.tap(Key::Enter);
    simulation.run_frames(2);
}

#[test]
fn views_and_their_nodes_are_added_and_inspected() {
    let mut simulation = garden(copy_of("garden", "views_added"));
    simulation.click_gui("garden");
    menu(&mut simulation, "➕", "📷 view3d");
    assert_eq!(selected_name(&simulation).as_deref(), Some("view3d"));
    assert!(matches!(
        node(&simulation, "view3d").kind,
        KindDef::View3d(_)
    ));

    // Inside a view, only what's placed in 3D can be added, and a model needs a model file.
    simulation.click_gui("➕");
    simulation.step();
    assert!(simulation.gui_rect("■ rect").is_none());
    assert!(simulation.gui_rect("👤 billboard").is_some());
    simulation.click_gui("⬛ mesh");
    simulation.run_frames(2);
    assert_eq!(selected_name(&simulation).as_deref(), Some("view3d/mesh"));
    let mesh = node(&simulation, "view3d/mesh");
    assert_eq!(mesh.placement3, Some(Placement3::default()));
    assert!(simulation.game().document().unwrap().error.is_none());

    // Its fields are 3D: no rotation, but a z and turns about each axis.
    assert!(simulation.gui_rect("rotation").is_none());
    set_field(&mut simulation, "z", "2.5");
    set_field(&mut simulation, "yaw", "90");
    let placement = node(&simulation, "view3d/mesh").placement3.unwrap();
    assert_eq!(placement.z, 2.5);
    assert!(
        (placement.yaw - FRAC_PI_2).abs() < 1e-5,
        "{}",
        placement.yaw
    );
    set_field(&mut simulation, "size y", "3");
    assert!(matches!(
        node(&simulation, "view3d/mesh").kind,
        KindDef::Mesh(MeshDef { shape: Shape::Cuboid([_, height, _]), .. }) if height == 3.0
    ));

    // The view's camera.
    select(&mut simulation, "view3d");
    set_field(&mut simulation, "camera position y", "6");
    let KindDef::View3d(view) = &node(&simulation, "view3d").kind else {
        unreachable!()
    };
    assert_eq!(view.camera.position, [0.0, 6.0, 5.0]);

    // Each was a change of its own.
    for _ in 0..6 {
        control(&mut simulation, Key::Z);
    }
    assert!(!simulation.game().document().unwrap().is_dirty());
}

#[test]
fn stacks_and_panels_are_added_and_inspected() {
    let mut simulation = garden(copy_of("garden", "panels_added"));
    simulation.click_gui("garden");
    menu(&mut simulation, "➕", "▣ panel");
    assert_eq!(selected_name(&simulation).as_deref(), Some("panel"));
    let KindDef::Panel(panel) = &node(&simulation, "panel").kind else {
        panic!("a panel");
    };
    assert_eq!(panel.stack.size, [None, None], "fitting what it holds");

    // What's added inside it is laid out by it: new rects are 16 square, one under the other
    // inside the panel's padding of 4.
    menu(&mut simulation, "➕", "■ rect");
    select(&mut simulation, "panel");
    menu(&mut simulation, "➕", "■ rect");
    let names: Vec<String> = node(&simulation, "panel")
        .children
        .iter()
        .map(|child| child.name.clone())
        .collect();
    assert_eq!(names.len(), 2);
    let bounds = |simulation: &Simulation<Editor>, path: &str| {
        let scene = simulation
            .game()
            .document()
            .unwrap()
            .preview
            .as_ref()
            .unwrap();
        let id = scene.find(scene.root(), path).unwrap();
        scene.bounds(id).unwrap()
    };
    let panel = bounds(&simulation, "panel");
    let first = bounds(&simulation, &format!("panel/{}", names[0]));
    let second = bounds(&simulation, &format!("panel/{}", names[1]));
    assert_eq!([first.x, first.y], [panel.x + 4.0, panel.y + 4.0]);
    assert_eq!([second.x, second.y], [first.x, first.bottom()]);
    assert_eq!([panel.w, panel.h], [24.0, 40.0]);

    // A gap between them, and a width of its own.
    select(&mut simulation, "panel");
    set_field(&mut simulation, "gap", "3");
    simulation.click_gui("fit width");
    simulation.run_frames(2);
    set_field(&mut simulation, "width", "40");
    let stack = node(&simulation, "panel").kind.stack().unwrap();
    assert_eq!((stack.gap, stack.size), (3.0, [Some(40.0), None]));
    let panel = bounds(&simulation, "panel");
    assert_eq!([panel.w, panel.h], [40.0, 43.0]);
    assert!(simulation.game().document().unwrap().error.is_none());
}

#[test]
fn instance_panels_keep_appearance_and_fitting_overrides() {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("editor/panel-overrides");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("main.luau"), "").unwrap();
    fs::copy(examples().join("rpg/frame.png"), folder.join("frame.png")).unwrap();
    fs::write(
        folder.join("base.scene.luau"),
        "return { format = 3,
        sheets = { ui = { path = 'frame.png', tile_width = 5, tile_height = 5 } },
        children = {
            { name = 'filled', kind = 'panel', width = 40, height = 30, color = '#123456',
              children = {{ name = 'child', kind = 'rect', width = 20, height = 10 }} },
            { name = 'framed', kind = 'panel', sheet = 'ui', cell = { 0, 0 }, slice = 1,
              width = 40, height = 30 },
        } }",
    )
    .unwrap();
    fs::write(
        folder.join("screen.scene.luau"),
        "return { format = 3, children = {
        { name = 'instance', kind = 'instance', scene = 'base.scene.luau' }
    } }",
    )
    .unwrap();
    let mut simulation = editor(Some(folder.clone()));
    simulation
        .game_mut()
        .open_scene("screen.scene.luau")
        .unwrap();
    simulation.run_frames(2);
    for (name, choice, field, value) in [
        ("filled", "sprite", "border", "3"),
        ("framed", "fill", "slice left", "2"),
    ] {
        simulation
            .game_mut()
            .document_mut()
            .unwrap()
            .select_inner(vec![0], name.into());
        simulation.run_frames(2);
        // An override of the old appearance must go when the look changes.
        set_field(&mut simulation, field, value);
        menu(&mut simulation, "Panel look", choice);
        let document = simulation.game().document().unwrap();
        assert!(document.error.is_none(), "{:?}", document.error);
        let scene = document.preview.as_ref().unwrap();
        let id = scene
            .find(scene.root(), &format!("instance/{name}"))
            .unwrap();
        let wingine::scene::Kind::Panel(panel) = scene.node(id).kind() else {
            unreachable!()
        };
        assert_eq!(
            matches!(panel.look, wingine::scene::PanelLook::Sprite { .. }),
            choice == "sprite"
        );
    }
    simulation
        .game_mut()
        .document_mut()
        .unwrap()
        .select_inner(vec![0], "filled".into());
    simulation.run_frames(2);
    simulation.click_gui("fit width");
    simulation.run_frames(2);
    simulation.click_gui("fit height");
    simulation.run_frames(2);
    let document = simulation.game().document().unwrap();
    let KindDef::Instance(instance) = &document.file().children[0].kind else {
        unreachable!()
    };
    assert_eq!(instance.overrides["filled"]["width"], Value::Bool(false));
    assert_eq!(instance.overrides["filled"]["height"], Value::Bool(false));
    let scene = document.preview.as_ref().unwrap();
    let id = scene.find(scene.root(), "instance/filled").unwrap();
    let rect = scene.bounds(id).unwrap();
    assert_eq!([rect.w, rect.h], [20.0, 10.0]);
    simulation
        .game_mut()
        .document_mut()
        .unwrap()
        .save(&folder)
        .unwrap();
    drop(simulation);

    // A saved override loads into a fresh editor, with fitting still following its content.
    let mut simulation = editor(Some(folder));
    simulation
        .game_mut()
        .open_scene("screen.scene.luau")
        .unwrap();
    simulation.run_frames(2);
    let document = simulation.game_mut().document_mut().unwrap();
    assert!(document.error.is_none(), "{:?}", document.error);
    let scene = document.preview.as_mut().unwrap();
    let panel = scene.find(scene.root(), "instance/filled").unwrap();
    assert_eq!(scene.node(panel).kind().stack().unwrap().size, [None, None]);
    let child = scene.find(panel, "child").unwrap();
    let wingine::scene::Kind::Rect(rect) = scene.node_mut(child).kind_mut() else {
        unreachable!()
    };
    rect.size = [60.0, 25.0];
    let rect = scene.bounds(panel).unwrap();
    assert_eq!([rect.w, rect.h], [60.0, 25.0]);
}

#[test]
fn clicking_in_a_view_picks_its_nodes_nearest_first() {
    let (mut simulation, _) = cellar("views_picked");
    let crate_ = world_to_window(
        &simulation,
        "view",
        center_3d(&simulation, "view/props/crate_2"),
    );
    let mut picked = Vec::new();
    for _ in 0..3 {
        simulation.click(MouseButton::Left, crate_[0], crate_[1]);
        simulation.step();
        picked.push(selected_name(&simulation).unwrap());
    }
    assert_eq!(picked, ["view/props/crate_2", "view/floor", "view"]);
}

#[test]
fn the_move_tool_drags_nodes_across_the_ground_and_up() {
    let (mut simulation, _) = cellar("views_dragged");
    select(&mut simulation, "view/props/crate_2");
    simulation.tap(Key::W);
    let start = node(&simulation, "view/props/crate_2").clone();
    let from = world_to_window(&simulation, "view", [8.5, 0.0, 3.5]);
    let to = world_to_window(&simulation, "view", [9.5, 0.0, 3.0]);
    drag(&mut simulation, from, to);
    let moved = node(&simulation, "view/props/crate_2").clone();
    assert!((moved.x - (start.x + 1.0)).abs() < 0.05, "{}", moved.x);
    assert!(
        (moved.placement3.unwrap().z - (start.placement3.unwrap().z - 0.5)).abs() < 0.05,
        "{:?}",
        moved.placement3
    );
    assert_eq!(moved.y, start.y, "across the ground");

    // With Shift, up and down.
    let from = world_to_window(&simulation, "view", [9.5, 0.0, 3.0]);
    let to = world_to_window(&simulation, "view", [9.5, 1.0, 3.0]);
    simulation.key_down(Key::LeftShift);
    drag(&mut simulation, from, to);
    simulation.key_up(Key::LeftShift);
    let lifted = node(&simulation, "view/props/crate_2").clone();
    assert!((lifted.y - 1.0).abs() < 0.05, "{}", lifted.y);
    assert_eq!(lifted.x, moved.x);

    // The Rotate tool leaves nodes in views alone: the Inspector turns them.
    simulation.tap(Key::E);
    drag(&mut simulation, from, to);
    assert_eq!(*node(&simulation, "view/props/crate_2"), lifted);

    // Each drag was a change of its own.
    control(&mut simulation, Key::Z);
    control(&mut simulation, Key::Z);
    assert_eq!(*node(&simulation, "view/props/crate_2"), start);
}

#[test]
fn model_files_dropped_in_a_view_are_placed_on_the_ground() {
    let (mut simulation, folder) = cellar("models_dropped");
    simulation.click_gui("📁 models");
    simulation.run_frames(2);
    let row = middle(simulation.gui_rect("barrel.glb").unwrap());
    let to = world_to_window(&simulation, "view", [6.5, 0.0, 5.5]);
    drag(&mut simulation, row, to);

    assert_eq!(selected_name(&simulation).as_deref(), Some("view/barrel"));
    let barrel = node(&simulation, "view/barrel").clone();
    assert!(
        matches!(&barrel.kind, KindDef::Model(def) if def.model == "models/barrel.glb"),
        "{:?}",
        barrel.kind
    );
    let z = barrel.placement3.unwrap().z;
    assert!(
        (barrel.x - 6.5).abs() < 0.05 && barrel.y == 0.0 && (z - 5.5).abs() < 0.05,
        "{barrel:?}"
    );
    assert!(simulation.game().document().unwrap().error.is_none());

    // Saved, the file holds it, and reads back as it is.
    control(&mut simulation, Key::S);
    let saved = SceneFile::read(folder.join("cellar.scene.luau")).unwrap();
    let path = edit::path_of(&saved, "view/barrel");
    assert_eq!(edit::node(&saved, &path), Some(&barrel));

    control(&mut simulation, Key::Z);
    let view = node(&simulation, "view");
    assert!(!view.children.iter().any(|child| child.name == "barrel"));
}

#[test]
fn scenes_that_cannot_go_in_a_view_are_refused() {
    let folder = copy_of("cellar", "scenes_refused");
    let sign = "return { format = 2, children = { \
                { name = 'board', kind = 'rect', width = 8, height = 4 } } }";
    fs::write(folder.join("sign.scene.luau"), sign).unwrap();
    let mut simulation = editor(Some(folder));
    simulation
        .game_mut()
        .open_scene("cellar.scene.luau")
        .unwrap();
    simulation.run_frames(2);
    let row = middle(simulation.gui_rect("sign.scene.luau").unwrap());
    let to = world_to_window(&simulation, "view", [6.5, 0.0, 5.5]);
    drag(&mut simulation, row, to);

    let view = node(&simulation, "view");
    assert!(!view.children.iter().any(|child| child.name == "sign"));
    let lines = simulation.game().output().lines();
    assert!(
        lines.iter().any(|line| line.level == Level::Error
            && line.text
                == "`sign.scene.luau` can't be placed in a 3D view: its node `board` is a rect \
                    node, which a 3D view can't hold"),
        "{:?}",
        lines.iter().map(|line| &line.text).collect::<Vec<_>>()
    );
    assert!(!simulation.game().has_unsaved_changes());
    assert!(simulation.game().document().unwrap().error.is_none());
}

#[test]
fn models_are_read_again_when_files_beside_them_change() {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("editor")
        .join("model_files_changed");
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    // A triangle `size` across, in a buffer beside the model.
    let write_triangle = |size: f32| {
        let corners = [0.0, 0.0, 0.0, size, 0.0, 0.0, 0.0, size, 0.0f32];
        let bytes: Vec<u8> = corners.iter().flat_map(|v| v.to_le_bytes()).collect();
        fs::write(folder.join("tri.bin"), bytes).unwrap();
    };
    write_triangle(1.0);
    let gltf = r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36,"uri":"tri.bin"}],
        "bufferViews":[{"buffer":0,"byteLength":36}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}]}"#;
    fs::write(folder.join("tri.gltf"), gltf).unwrap();

    let mut headless = Headless::new().expect("a GPU adapter is available");
    let mut cache = AssetCache::default();
    let mut loader = cache.loader(headless.context(), &folder);
    let mut width = || loader.model("tri.gltf").unwrap().bounds().unwrap().max[0];
    assert_eq!(width(), 1.0);
    write_triangle(2.0);
    // Later than it was, however coarsely the file system keeps times.
    let buffer = fs::File::options()
        .write(true)
        .open(folder.join("tri.bin"))
        .unwrap();
    buffer
        .set_modified(SystemTime::now() + Duration::from_secs(10))
        .unwrap();
    assert_eq!(width(), 2.0, "the buffer changed, though the .gltf didn't");
}

/// Adds a 16 by 16 rect named `box` at the top of the open scene, at `x`, `y`, and selects it.
fn add_box(simulation: &mut Simulation<Editor>, x: f32, y: f32) {
    let document = simulation.game_mut().document_mut().unwrap();
    let mut node = edit::new_node("rect", edit::Showing::default());
    node.name = "box".into();
    node.x = x;
    node.y = y;
    let path = edit::add(document.edit(), &[], node).unwrap();
    document.end_change();
    document.select(Some(path));
    simulation.run_frames(2);
}

/// Where the open scene's node at `names` is drawn, in the scene.
fn bounds(simulation: &Simulation<Editor>, names: &str) -> wingine::math::Rect {
    let document = simulation.game().document().unwrap();
    let id = document
        .live_node(&edit::path_of(document.file(), names))
        .unwrap();
    document.preview.as_ref().unwrap().bounds(id).unwrap()
}

fn assert_bounds(simulation: &Simulation<Editor>, names: &str, [x, y, w, h]: [f32; 4]) {
    let rect = bounds(simulation, names);
    assert_eq!([rect.x, rect.y, rect.w, rect.h], [x, y, w, h]);
}

/// Opens the menu `menu` and clicks its item `item`: menus are sized in their first frame, and
/// shown in the next.
fn menu(simulation: &mut Simulation<Editor>, menu: &str, item: &str) {
    simulation.click_gui(menu);
    simulation.step();
    simulation.click_gui(item);
    simulation.run_frames(2);
}

#[test]
fn anchor_presets_lay_nodes_out_on_canvases_of_any_size() {
    let mut simulation = garden(examples().join("garden"));
    add_box(&mut simulation, 40.0, 30.0);

    // Into the canvas's corner, and its middle.
    menu(&mut simulation, "Anchors", "Bottom Right");
    assert_bounds(&simulation, "box", [304.0, 164.0, 16.0, 16.0]);
    menu(&mut simulation, "Anchors", "Center");
    assert_bounds(&simulation, "box", [152.0, 82.0, 16.0, 16.0]);

    // Without an anchor, it stays where it is.
    menu(&mut simulation, "Anchors", "No Anchor");
    assert_eq!(node(&simulation, "box").anchor, None);
    assert_bounds(&simulation, "box", [152.0, 82.0, 16.0, 16.0]);

    // Stretched across the canvas, and on a bigger one, stretched with it.
    menu(&mut simulation, "Anchors", "Full Rect");
    assert_bounds(&simulation, "box", [0.0, 0.0, 320.0, 180.0]);
    menu(&mut simulation, "canvas 320 x 180", "640 x 360");
    assert_eq!(
        simulation
            .game()
            .document()
            .unwrap()
            .preview
            .as_ref()
            .unwrap()
            .size(),
        [640.0, 360.0]
    );
    assert_bounds(&simulation, "box", [0.0, 0.0, 640.0, 360.0]);
    menu(
        &mut simulation,
        "preview 640 x 360",
        "The game's, 320 x 180",
    );
    assert_bounds(&simulation, "box", [0.0, 0.0, 320.0, 180.0]);

    // Each was a change of its own, back to where it was added.
    for _ in 0..4 {
        control(&mut simulation, Key::Z);
    }
    assert_eq!(node(&simulation, "box").anchor, None);
    assert_bounds(&simulation, "box", [40.0, 30.0, 16.0, 16.0]);
}

#[test]
fn handles_resize_nodes_with_the_select_tool() {
    let mut simulation = garden(examples().join("garden"));
    add_box(&mut simulation, 100.0, 60.0);
    let zoom = simulation.game().document().unwrap().view.unwrap().zoom;
    let size = |simulation: &Simulation<Editor>| match &node(simulation, "box").kind {
        KindDef::Rect(rect) => rect.size,
        _ => panic!("the box is a rect"),
    };
    let near = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3;

    // Its right edge, 24 points further right.
    let from = simulation.game().scene_to_window([116.0, 68.0]).unwrap();
    drag(&mut simulation, from, [from[0] + 24.0, from[1]]);
    let wider = size(&simulation);
    assert!(near(wider, [16.0 + 24.0 / zoom, 16.0]), "{wider:?}");
    assert_eq!(node(&simulation, "box").x, 100.0);

    // Its top-left corner, out by 8 points each way: it grows, and moves with the corner.
    let from = simulation.game().scene_to_window([100.0, 60.0]).unwrap();
    drag(&mut simulation, from, [from[0] - 8.0, from[1] - 8.0]);
    let bigger = size(&simulation);
    assert!(
        near(bigger, [16.0 + 32.0 / zoom, 16.0 + 8.0 / zoom]),
        "{bigger:?}"
    );
    let moved = node(&simulation, "box");
    let at = [moved.x, moved.y];
    assert!(near(at, [100.0 - 8.0 / zoom, 60.0 - 8.0 / zoom]), "{at:?}");

    control(&mut simulation, Key::Z);
    control(&mut simulation, Key::Z);
    assert_eq!(size(&simulation), [16.0, 16.0]);
}

#[test]
fn dragging_the_canvas_corner_previews_another_size() {
    let mut simulation = garden(examples().join("garden"));
    let zoom = simulation.game().document().unwrap().view.unwrap().zoom;
    let from = simulation.game().scene_to_window([320.0, 180.0]).unwrap();
    drag(&mut simulation, from, [from[0] + 40.0, from[1] + 20.0]);
    let size = simulation
        .game()
        .document()
        .unwrap()
        .preview
        .as_ref()
        .unwrap()
        .size();
    assert_eq!(
        size,
        [320.0 + 40.0 / zoom, 180.0 + 20.0 / zoom].map(f32::round)
    );
    // Only a preview: the scene hasn't changed.
    assert!(!simulation.game().document().unwrap().is_dirty());
}

/// The editor with the orchard's picker open and the Animation panel showing its clip `clip`, in
/// a copy of the game of the test's own.
fn picker(test: &str, clip: &str) -> Simulation<Editor> {
    let mut simulation = editor(Some(copy_of("orchard", test)));
    simulation
        .game_mut()
        .open_scene("picker.scene.luau")
        .unwrap();
    simulation.run_frames(2);
    simulation.click_gui("Animation");
    simulation.run_frames(2);
    assert_eq!(simulation.game().bottom_tab(), BottomTab::Animation);
    menu(&mut simulation, "Clip", clip);
    assert_eq!(timeline(&simulation).clip.as_deref(), Some(clip));
    simulation
}

fn timeline(simulation: &Simulation<Editor>) -> &Timeline {
    &simulation.game().document().unwrap().timeline
}

/// Moves the playhead to `time` seconds.
fn set_time(simulation: &mut Simulation<Editor>, time: f32) {
    simulation.game_mut().document_mut().unwrap().timeline.time = time;
    simulation.run_frames(2);
}

fn clip<'a>(simulation: &'a Simulation<Editor>, name: &str) -> &'a Clip {
    &file(simulation).clips[name]
}

/// The keys of the track of `property` of the node at `names` in the clip `clip`.
fn keys(simulation: &Simulation<Editor>, clip_name: &str, names: &str, property: &str) -> Keys {
    let clip = clip(simulation, clip_name);
    let track = clips::track(clip, names, property)
        .unwrap_or_else(|| panic!("`{clip_name}` has no track of {names}'s {property}"));
    clip.tracks[track].keys.clone()
}

/// The node of the preview made from the node at `names`, as it's posed.
fn live<'a>(simulation: &'a Simulation<Editor>, names: &str) -> &'a wingine::scene::Node {
    let document = simulation.game().document().unwrap();
    let scene = document.preview.as_ref().unwrap();
    scene.node(scene.find(scene.root(), names).unwrap())
}

/// Where the ruler puts `time` seconds of a clip `duration` long, in the window, on the row
/// whose middle is at `y`.
fn on_timeline(simulation: &Simulation<Editor>, time: f32, duration: f32, y: f32) -> [f32; 2] {
    let ruler = simulation.gui_rect("Timeline").unwrap();
    // As far in from its ends as the panel leaves room.
    let pad = 10.0;
    [ruler.x + pad + time / duration * (ruler.w - 2.0 * pad), y]
}

/// Clicks the field named `name` in the Inspector, and types `text` into it.
fn type_into(simulation: &mut Simulation<Editor>, name: &str, text: &str) {
    // The field after the label.
    let field = *simulation.gui_rects(name).last().unwrap();
    let [x, y] = middle(field);
    simulation.click(MouseButton::Left, x, y);
    simulation.step();
    control(simulation, Key::A);
    simulation.type_text(text);
    simulation.tap(Key::Enter);
    simulation.run_frames(2);
}

#[test]
fn scrubbing_and_playing_a_clip_poses_the_preview() {
    let mut simulation = picker("scrubbing", "walk");
    let rest = node(&simulation, "hips/body/head").rotation;
    let head = keys(&simulation, "walk", "hips/body/head", "rotation");
    let duration = clip(&simulation, "walk").duration;

    // Halfway along the ruler, snapped to its step.
    let ruler = simulation.gui_rect("Timeline").unwrap();
    let [x, y] = on_timeline(
        &simulation,
        duration / 2.0 + 0.01,
        duration,
        middle(ruler)[1],
    );
    simulation.click(MouseButton::Left, x, y);
    simulation.run_frames(2);
    let time = timeline(&simulation).time;
    assert_eq!(time, duration / 2.0);
    let Some(KeyValue::Number(posed)) = head.value_at(time) else {
        panic!("the head turns")
    };
    assert_eq!(live(&simulation, "hips/body/head").rotation(), posed);
    assert_ne!(posed, rest);
    // Only the preview is posed.
    assert_eq!(node(&simulation, "hips/body/head").rotation, rest);
    assert!(!simulation.game().document().unwrap().is_dirty());

    // Playing moves the playhead on from the next frame, and the walk loops. The editor wants
    // frames as long as it plays.
    simulation.click_gui("Play Clip");
    simulation.run_frames(30);
    assert!(simulation.game().animation().playing);
    let played = timeline(&simulation).time;
    let expected = (time + 30.0 / 60.0) % duration;
    assert!((played - expected).abs() < 0.001, "{played} {expected}");
    assert_eq!(simulation.next_frame_in(), Some(0.0));
    simulation.click_gui("Pause Clip");
    simulation.step();
    assert!(!simulation.game().animation().playing);
    settle(&mut simulation);

    // Another panel puts the preview back as the file has it.
    simulation.click_gui("Output");
    simulation.run_frames(2);
    assert_eq!(live(&simulation, "hips/body/head").rotation(), rest);
}

#[test]
fn keying_from_the_inspector_puts_keys_at_the_playhead() {
    // Blink changes only the face, so the rest of the picker is free to key.
    let mut simulation = picker("keying", "blink");
    select(&mut simulation, "hips/body/head");
    let rest = node(&simulation, "hips/body/head").clone();
    set_time(&mut simulation, 1.2);

    simulation.click_gui("Key x");
    simulation.run_frames(2);
    assert_eq!(
        keys(&simulation, "blink", "hips/body/head", "x"),
        Keys::Number {
            property: Property::X,
            interpolation: Interpolation::Linear,
            keys: vec![(1.2, rest.x)],
        }
    );

    // A field the clip keys changes its key at the playhead, or adds one there, not the node.
    type_into(&mut simulation, "x", "5");
    set_time(&mut simulation, 0.6);
    type_into(&mut simulation, "x", "9");
    assert_eq!(
        keys(&simulation, "blink", "hips/body/head", "x").times(),
        [0.6, 1.2]
    );
    assert_eq!(
        keys(&simulation, "blink", "hips/body/head", "x").value_at(1.2),
        Some(KeyValue::Number(5.0))
    );
    assert_eq!(node(&simulation, "hips/body/head").x, rest.x);
    assert_eq!(live(&simulation, "hips/body/head").position()[0], 9.0);

    // One it doesn't key changes the node.
    type_into(&mut simulation, "y", "-20");
    assert_eq!(node(&simulation, "hips/body/head").y, -20.0);
    assert!(clips::track(clip(&simulation, "blink"), "hips/body/head", "y").is_none());

    // Each was a change of its own.
    control(&mut simulation, Key::Z);
    control(&mut simulation, Key::Z);
    assert_eq!(
        keys(&simulation, "blink", "hips/body/head", "x").times(),
        [1.2]
    );

    // Keys belong to the clip on the timeline, so without it, there's nothing to key.
    simulation.click_gui("Output");
    simulation.run_frames(2);
    assert!(simulation.gui_rect("Key x").is_none());
}

#[test]
fn turning_a_keyed_node_in_the_viewport_keys_it() {
    let mut simulation = picker("turning", "walk");
    select(&mut simulation, "hips/body/head");
    set_time(&mut simulation, 0.2);
    let rest = node(&simulation, "hips/body/head").rotation;
    let Some(KeyValue::Number(before)) =
        keys(&simulation, "walk", "hips/body/head", "rotation").value_at(0.2)
    else {
        panic!("the head turns")
    };
    simulation.tap(Key::E);
    assert_eq!(simulation.game().tool(), Tool::Rotate);

    // An eighth of a turn about its origin, from below and right of it to straight below.
    let document = simulation.game().document().unwrap();
    let id = document
        .live_node(&edit::path_of(document.file(), "hips/body/head"))
        .unwrap();
    let origin = document.preview.as_ref().unwrap().transform(id).translation;
    let center = simulation.game().scene_to_window(origin).unwrap();
    drag(
        &mut simulation,
        [center[0] + 200.0, center[1] + 200.0],
        [center[0], center[1] + 280.0],
    );
    let turned = keys(&simulation, "walk", "hips/body/head", "rotation");
    let Some(KeyValue::Number(after)) = turned.value_at(0.2) else {
        panic!("the head turns")
    };
    let eighth = std::f32::consts::FRAC_PI_4;
    assert!((after - (before + eighth)).abs() < 1e-3, "{after} {before}");
    assert!(turned.times().contains(&0.2));
    assert_eq!(node(&simulation, "hips/body/head").rotation, rest);
}

#[test]
fn keys_move_and_go_on_the_timeline() {
    let mut simulation = picker("timeline", "blink");
    let times = |simulation: &Simulation<Editor>| {
        keys(simulation, "blink", "hips/body/head/face", "cell").times()
    };
    let start = times(&simulation);
    assert_eq!(start.len(), 3);
    let duration = clip(&simulation, "blink").duration;

    // Dragging the second key to 2 seconds in.
    let key = simulation.gui_rect("face:cell key 2").unwrap();
    let from = middle(key);
    let to = on_timeline(&simulation, 2.0, duration, from[1]);
    drag(&mut simulation, from, to);
    assert_eq!(times(&simulation), [start[0], 2.0, start[2]]);

    // Its menu takes it out.
    let [x, y] = middle(simulation.gui_rect("face:cell key 2").unwrap());
    simulation.click(MouseButton::Right, x, y);
    simulation.run_frames(2);
    simulation.click_gui("Delete Key");
    simulation.run_frames(2);
    assert_eq!(times(&simulation), [start[0], start[2]]);

    // Over the timeline, Delete takes out the selected key rather than the selected node.
    select(&mut simulation, "hips/body/head/face");
    simulation.click_gui("face:cell key 2");
    simulation.step();
    assert_eq!(timeline(&simulation).time, start[2]);
    simulation.tap(Key::Delete);
    simulation.run_frames(2);
    assert_eq!(times(&simulation), [start[0]]);
    assert!(
        edit::node(
            file(&simulation),
            &edit::path_of(file(&simulation), "hips/body/head/face")
        )
        .is_some()
    );

    // Each was a change of its own.
    for _ in 0..3 {
        control(&mut simulation, Key::Z);
    }
    assert_eq!(times(&simulation), start);
    assert!(!simulation.game().document().unwrap().is_dirty());
}

#[test]
fn a_clip_keeps_its_length_until_it_is_changed() {
    let folder = copy_of("orchard", "clip_length");
    // Longer than the editor writes lengths, and its last key at its end.
    fs::write(
        folder.join("wave.scene.luau"),
        r#"return {
            format = 1,
            children = { { name = "arm", kind = "node" } },
            clips = { wave = { duration = 0.12344, tracks = {
                { node = "arm", property = "x", keys = { { 0, 0 }, { 0.12344, 1 } } },
            } } },
        }"#,
    )
    .unwrap();
    let mut simulation = editor(Some(folder));
    simulation.game_mut().open_scene("wave.scene.luau").unwrap();
    simulation.run_frames(2);
    simulation.click_gui("Animation");
    simulation.run_frames(2);
    assert_eq!(timeline(&simulation).clip.as_deref(), Some("wave"));
    assert_eq!(clip(&simulation, "wave").duration, 0.12344);
    assert!(!simulation.game().has_unsaved_changes());

    type_into(&mut simulation, "duration", "0.5");
    assert_eq!(clip(&simulation, "wave").duration, 0.5);
}

#[test]
fn clips_are_made_named_lengthened_looped_and_saved() {
    let folder = copy_of("orchard", "clips");
    let mut simulation = editor(Some(folder.clone()));
    simulation
        .game_mut()
        .open_scene("orchard.scene.luau")
        .unwrap();
    simulation.run_frames(2);
    simulation.click_gui("Animation");
    simulation.run_frames(2);
    assert!(file(&simulation).clips.is_empty());

    simulation.click_gui("New Clip");
    simulation.run_frames(2);
    assert_eq!(timeline(&simulation).clip.as_deref(), Some("clip"));
    type_into(&mut simulation, "duration", "2.5");
    simulation.click_gui("Loop");
    simulation.run_frames(2);
    simulation.click_gui("Rename Clip");
    simulation.run_frames(2);
    control(&mut simulation, Key::A);
    simulation.type_text("sway");
    simulation.tap(Key::Enter);
    simulation.run_frames(2);
    assert_eq!(timeline(&simulation).clip.as_deref(), Some("sway"));
    let sway = clip(&simulation, "sway");
    assert_eq!((sway.duration, sway.looping), (2.5, true));

    // Keying the picker the scene places, at the end of the clip.
    select(&mut simulation, "picker");
    set_time(&mut simulation, 2.5);
    simulation.click_gui("Key rotation");
    simulation.run_frames(2);
    assert_eq!(
        keys(&simulation, "sway", "picker", "rotation").times(),
        [2.5]
    );

    // Saved in the canonical layout, it reads back the same.
    control(&mut simulation, Key::S);
    let saved = SceneFile::read(folder.join("orchard.scene.luau")).unwrap();
    assert_eq!(&saved, file(&simulation));
    assert_eq!(
        fs::read_to_string(folder.join("orchard.scene.luau")).unwrap(),
        saved.to_luau().unwrap()
    );

    simulation.click_gui("Delete Clip");
    simulation.run_frames(2);
    assert!(file(&simulation).clips.is_empty());
    assert_eq!(timeline(&simulation).clip, None);
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
fn what_fails_to_save_on_closing_stays_open() {
    let folder = copy_of("garden", "failing_save");
    let mut simulation = garden(folder.clone());
    let document = simulation.game_mut().document_mut().unwrap();
    edit::node_mut(document.edit(), &[0]).unwrap().x += 1.0;
    document.end_change();
    // A folder where the file was, which can't be written over.
    let path = folder.join("garden.scene.luau");
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();

    assert!(!simulation.request_close());
    simulation.run_frames(2);
    simulation.click_gui("Save");
    assert!(!simulation.has_quit());
    assert!(simulation.game().has_unsaved_changes());
    let lines = simulation.game().output().lines();
    assert!(
        lines
            .iter()
            .any(|line| line.level == Level::Error && line.text.contains("garden.scene.luau")),
        "{lines:?}"
    );
}

#[test]
fn opening_another_game_with_unsaved_changes_asks_first() {
    let mut simulation = garden(examples().join("garden"));
    let document = simulation.game_mut().document_mut().unwrap();
    edit::node_mut(document.edit(), &[0]).unwrap().x += 1.0;
    document.end_change();
    let project =
        |simulation: &Simulation<Editor>| simulation.game().project().unwrap().name.clone();

    simulation.game_mut().open_folder(examples().join("pong"));
    simulation.run_frames(2);
    assert!(simulation.gui_rect("Unsaved Changes").is_some());
    simulation.click_gui("Cancel");
    assert_eq!(project(&simulation), "garden");
    assert!(simulation.game().has_unsaved_changes());
    simulation.run_for(0.5);

    simulation.game_mut().open_folder(examples().join("pong"));
    simulation.run_frames(2);
    simulation.click_gui("Don't Save");
    assert_eq!(project(&simulation), "pong");
    assert!(simulation.game().documents().is_empty());
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
    // The game ending wakes the editor.
    wait_for(&mut simulation, "the game to end", |editor| {
        !editor.is_playing()
    });
    let lines = simulation.game().output().lines();
    assert!(
        lines.iter().any(|line| line.text == "The game ended."),
        "{lines:?}"
    );
}

#[test]
fn the_editor_runs_frames_only_when_something_changes() {
    let mut simulation = garden(examples().join("garden"));
    settle(&mut simulation);

    // The pointer moving over the viewport wants a frame, then the editor waits again.
    let [x, y] = simulation.game().scene_to_window([100.0, 100.0]).unwrap();
    simulation.move_mouse(x, y);
    assert_eq!(simulation.next_frame_in(), Some(0.0));
    settle(&mut simulation);
    assert_eq!(simulation.next_frame_in(), None);
}

#[test]
fn files_changed_elsewhere_wake_the_editor_to_read_them() {
    let folder = copy_of("pong", "watching");
    let mut simulation = editor(Some(folder.clone()));
    simulation.game_mut().scripts_mut().set_program(None);
    simulation.game_mut().open_script("opponent.luau").unwrap();
    settle(&mut simulation);

    fs::write(folder.join("opponent.luau"), "-- changed\n").unwrap();
    fs::write(folder.join("extra.luau"), "").unwrap();
    wait_for(&mut simulation, "the files to be read again", |editor| {
        script_is(editor, "-- changed\n")
    });
    simulation.step();
    assert!(simulation.gui_rect("extra.luau").is_some());
}

#[test]
fn scenes_changed_elsewhere_are_read_again_unless_changed_here() {
    let folder = copy_of("garden", "scene_changed");
    let mut simulation = garden(folder.clone());
    settle(&mut simulation);
    let path = folder.join("garden.scene.luau");
    let mut changed = SceneFile::read(&path).unwrap();
    changed.children[0].x += 10.0;
    let x = changed.children[0].x;
    changed.write(&path).unwrap();
    wait_for(&mut simulation, "the scene to be read again", |editor| {
        editor.document().unwrap().file().children[0].x == x
    });
    assert!(!simulation.game().has_unsaved_changes());

    // With changes of its own, it's kept as it is here, and Output says so.
    let document = simulation.game_mut().document_mut().unwrap();
    edit::node_mut(document.edit(), &[0]).unwrap().y += 1.0;
    document.end_change();
    changed.children[0].x += 10.0;
    changed.write(&path).unwrap();
    wait_for(
        &mut simulation,
        "the change on disk to be noticed",
        |editor| {
            editor.output().lines().iter().any(|line| {
                line.level == Level::Warning && line.text.contains("has unsaved changes here")
            })
        },
    );
    assert_eq!(file(&simulation).children[0].x, x);
    assert!(simulation.game().has_unsaved_changes());
}

fn script_is(editor: &Editor, text: &str) -> bool {
    editor
        .scripts()
        .script()
        .is_some_and(|script| script.text == text)
}

/// The text of the script shown.
fn script_text(simulation: &Simulation<Editor>) -> &str {
    &simulation.game().scripts().script().unwrap().text
}

/// Whether luau-lsp can be run, so tests that need it can say they're skipped if not. CI installs
/// it, and sets `WINGINE_LUAU_LSP` to it, which makes it a failure to be without it.
fn luau_lsp_runs() -> bool {
    let runs = Command::new(lsp::program())
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !runs {
        assert!(
            std::env::var_os("WINGINE_LUAU_LSP").is_none(),
            "WINGINE_LUAU_LSP is set, but doesn't run"
        );
        eprintln!("luau-lsp isn't installed, so this test checks nothing");
    }
    runs
}

#[test]
fn double_clicking_a_script_opens_it_to_edit_and_save() {
    let folder = copy_of("pong", "script-edit");
    let mut simulation = editor(Some(folder.clone()));
    simulation.game_mut().scripts_mut().set_program(None);
    double_click_gui(&mut simulation, "opponent.luau");
    simulation.run_frames(2);
    assert_eq!(simulation.game().screen(), MainScreen::Script);
    let on_disk = fs::read_to_string(folder.join("opponent.luau")).unwrap();
    assert_eq!(script_text(&simulation), on_disk.replace("\r\n", "\n"));

    // Typing at the end, where Enter keeps the indentation and indents blocks.
    let end = script_text(&simulation).chars().count();
    simulation.game_mut().scripts_mut().go_to(end);
    simulation.run_frames(2);
    simulation.type_text("if ready then");
    simulation.tap(Key::Enter);
    simulation.type_text("serve()");
    simulation.tap(Key::Enter);
    simulation.run_frames(2);
    assert!(
        script_text(&simulation).ends_with("\nif ready then\n    serve()\n    "),
        "{:?}",
        script_text(&simulation)
    );
    assert!(simulation.gui_rect("opponent.luau*").is_some());

    control(&mut simulation, Key::S);
    let saved = fs::read_to_string(folder.join("opponent.luau")).unwrap();
    assert_eq!(saved.replace("\r\n", "\n"), script_text(&simulation));
    assert!(!simulation.game().scripts().is_dirty());
    assert!(simulation.gui_rect("opponent.luau").is_some());

    // The 2D screen is a click away, and the script stays open.
    simulation.click_gui("🖼 2D");
    assert_eq!(simulation.game().screen(), MainScreen::TwoD);
    assert_eq!(simulation.game().scripts().scripts().len(), 1);
}

#[test]
fn without_luau_lsp_scripts_are_edited_unchecked() {
    let mut simulation = editor(Some(copy_of("pong", "script-unchecked")));
    simulation
        .game_mut()
        .scripts_mut()
        .set_program(Some("no-such-luau-lsp".into()));
    simulation.game_mut().open_script("main.luau").unwrap();
    simulation.run_frames(3);
    let lines = simulation.game().output().lines();
    assert!(
        lines
            .iter()
            .any(|line| line.level == Level::Warning && line.text.contains("isn't installed")),
        "{lines:?}"
    );
    simulation.game_mut().scripts_mut().go_to(0);
    simulation.run_frames(2);
    simulation.type_text("--");
    simulation.run_frames(2);
    assert!(script_text(&simulation).starts_with("---- Pong"));
}

#[test]
fn luau_lsp_checks_scripts_as_they_are_typed() {
    if !luau_lsp_runs() {
        return;
    }
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("editor/script-checked");
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    let main = "local font = graphics.default_font()\n\nfunction draw()\n    graphics.draw_text(font, \"hi\", 0, 0, \"#ffffff\")\nend\n";
    fs::write(folder.join("main.luau"), main).unwrap();
    let mut simulation = editor(Some(folder));
    simulation.game_mut().open_script("main.luau").unwrap();
    wait_for(&mut simulation, "luau-lsp to start", |editor| {
        editor.scripts().is_checking()
    });
    simulation.run_frames(10);
    assert!(
        simulation
            .game()
            .scripts()
            .script()
            .unwrap()
            .diagnostics
            .is_empty()
    );

    // Resting the pointer on a name shows its type and docs, from the engine's definitions.
    let at = main.find("default_font").unwrap() + 3;
    let [x, y] = simulation.game().scripts().screen_pos(at).unwrap();
    simulation.move_mouse(x, y);
    wait_for(&mut simulation, "the hover", |editor| {
        editor
            .scripts()
            .hover_text()
            .is_some_and(|text| text.contains("built-in pixel font"))
    });
    let hover = simulation.game().scripts().hover_text().unwrap();
    assert!(hover.contains("default_font(): Font"), "{hover}");
    simulation.move_mouse(1.0, 1.0);

    // Typing a field of the engine's tables offers them, narrowed as more is typed.
    let end = main.chars().count();
    simulation.game_mut().scripts_mut().go_to(end);
    simulation.run_frames(2);
    simulation.type_text("graphics.draw_t");
    wait_for(&mut simulation, "completions", |editor| {
        !editor.scripts().completions().is_empty()
    });
    let offered = simulation.game().scripts().completions();
    assert!(offered.contains(&"draw_text"), "{offered:?}");
    assert!(
        offered.iter().all(|label| label.contains("draw_t")),
        "{offered:?}"
    );
    simulation.type_text("e");
    simulation.run_frames(2);
    assert_eq!(
        simulation.game().scripts().completions(),
        ["draw_text", "draw_text_layout"]
    );
    simulation.tap(Key::Enter);
    simulation.run_frames(2);
    assert!(script_text(&simulation).ends_with("graphics.draw_text"));

    // Opening its call shows its parameters.
    simulation.type_text("(");
    wait_for(&mut simulation, "the signature", |editor| {
        editor.scripts().signature_text().is_some()
    });
    let signature = simulation.game().scripts().signature_text().unwrap();
    assert!(signature.contains("font: Font"), "{signature}");

    // A mistake is found, and shown where it is.
    simulation.type_text("1)");
    let wrong_font = |editor: &Editor| {
        let problems = &editor.scripts().script().unwrap().diagnostics;
        problems
            .iter()
            .find(|problem| problem.message.contains("Expected this to be 'Font'"))
            .cloned()
    };
    wait_for(&mut simulation, "the problem", |editor| {
        wrong_font(editor).is_some()
    });
    let problem = wrong_font(simulation.game()).unwrap();
    assert_eq!((problem.start.line, problem.start.character), (5, 19));
    assert_eq!(problem.severity, Severity::Error);
}

#[test]
fn the_examples_scripts_check_against_the_engines_definitions() {
    if !luau_lsp_runs() {
        return;
    }
    let (definitions, luaurc) = lsp::support_files().unwrap();
    for game in [
        "pong", "rpg", "shmup", "garden", "orchard", "cellar", "course",
    ] {
        let folder = examples().join(game);
        let scripts: Vec<String> = project_scripts(&folder, "");
        let output = Command::new(lsp::program())
            .arg("analyze")
            .arg("--platform=standard")
            .arg(format!("--definitions=@wingine={}", definitions.display()))
            .arg(format!("--base-luaurc={}", luaurc.display()))
            .args(&scripts)
            .current_dir(&folder)
            .output()
            .unwrap();
        let printed = String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{game}:\n{printed}");
    }
}

/// The scripts in the game in `folder`, apart from scene and tileset files, by their paths in it.
fn project_scripts(folder: &Path, path: &str) -> Vec<String> {
    let mut scripts = Vec::new();
    for entry in fs::read_dir(folder.join(path)).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        let inside = if path.is_empty() {
            name.clone()
        } else {
            format!("{path}/{name}")
        };
        if entry.file_type().unwrap().is_dir() {
            scripts.extend(project_scripts(folder, &inside));
        } else if name.ends_with(".luau")
            && !name.ends_with(".scene.luau")
            && !name.ends_with(".tileset.luau")
        {
            scripts.push(inside);
        }
    }
    scripts
}
