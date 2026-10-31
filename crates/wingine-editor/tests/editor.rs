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
    scene::{KindDef, SceneFile, TileLayerDef, Value},
};
use wingine_editor::{
    Editor,
    docks::{output::Level, tiles::TileTool},
    document::View,
    edit,
    editor::BottomTab,
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

/// Adds a 16 by 16 rect named `box` at the top of the open scene, at `x`, `y`, and selects it.
fn add_box(simulation: &mut Simulation<Editor>, x: f32, y: f32) {
    let document = simulation.game_mut().document_mut().unwrap();
    let mut node = edit::new_node("rect", "", "", "");
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
