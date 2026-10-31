//! Tests for scenes loaded to play: finding nodes, where they are, and changing them. Reading and
//! writing scene files is tested in `src/scene`, and drawing scenes in `tests/screenshots.rs`.

use std::{f32::consts::FRAC_PI_2, fs, path::Path};

use wingine::{
    Error,
    graphics::Color,
    headless::Headless,
    math::Rect,
    scene::{Kind, Scene, SceneFile, Value},
};

const TILESET: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/kenney_1-bit-pack_transparent-packed.png"
);

/// A garden: a mill whose sails are inside it, a hidden flower bed with a flower, and areas.
const GARDEN: &str = r##"return {
    format = 1,
    sheets = { tiles = { path = "tiles.png", tile_width = 16, tile_height = 16 } },
    props = { title = "The garden" },
    children = {
        {
            name = "mill", kind = "sprite", x = 100, y = 50, sheet = "tiles", cell = { 0, 1 },
            pivot = { 8, 16 }, tags = { "solid" },
            children = {
                {
                    name = "sails", kind = "node", y = -12, rotation = 1.5707964,
                    children = {
                        { name = "sail", kind = "rect", width = 2, height = 10, pivot = { 1, 10 } },
                    },
                },
            },
        },
        {
            name = "bed", kind = "node", x = 10, y = 10, visible = false, tags = { "solid" },
            children = {
                { name = "flower", kind = "sprite", sheet = "tiles", cell = { 15, 6 }, tags = { "solid" } },
            },
        },
        {
            name = "gate", kind = "area", x = 40, y = 40, width = 20, height = 10, pivot = { 10, 5 },
            rotation = 1.5707964, props = { to = "town", arrive = { 3, 4 } },
        },
        { name = "label", kind = "text", text = "Hi", x = 0, y = 100 },
    },
}"##;

fn headless() -> Headless {
    Headless::new().expect("scene tests need a GPU adapter, as screenshot tests do")
}

fn garden(headless: &mut Headless) -> Scene {
    let file = SceneFile::parse(GARDEN, "garden.scene.luau").unwrap();
    let ctx = headless.context();
    let font = ctx.default_font().clone();
    Scene::new(&file, "garden.scene.luau", &font, |_| {
        ctx.load_texture(TILESET)
    })
    .unwrap()
}

fn assert_near(actual: [f32; 2], expected: [f32; 2]) {
    assert!(
        (actual[0] - expected[0]).abs() < 1e-4 && (actual[1] - expected[1]).abs() < 1e-4,
        "{actual:?} is not {expected:?}"
    );
}

#[test]
fn nodes_are_found_by_path_and_tag() {
    let scene = garden(&mut headless());
    let root = scene.root();
    assert_eq!(
        scene.node(root).props()["title"],
        Value::String("The garden".into())
    );
    let sails = scene.find(root, "mill/sails").unwrap();
    assert_eq!(scene.path(sails), "mill/sails");
    assert_eq!(scene.node(sails).name(), "sails");
    let mill = scene.node(sails).parent().unwrap();
    assert_eq!(scene.find(mill, "sails"), Some(sails));
    assert_eq!(scene.find(root, "mill/sail"), None);
    assert_eq!(scene.find(root, ""), None);
    assert_eq!(scene.path(root), "");

    let solid: Vec<String> = scene
        .tagged(root, "solid")
        .into_iter()
        .map(|id| scene.path(id))
        .collect();
    assert_eq!(
        solid,
        ["mill", "bed", "bed/flower"],
        "in the order they draw"
    );
    let bed = scene.find(root, "bed").unwrap();
    assert_eq!(
        scene.tagged(bed, "solid").len(),
        2,
        "the node itself, and inside it"
    );
    let order: Vec<String> = scene
        .descendants(mill)
        .into_iter()
        .map(|id| scene.path(id))
        .collect();
    assert_eq!(order, ["mill", "mill/sails", "mill/sails/sail"]);
}

#[test]
fn nodes_are_placed_within_their_parents() {
    let mut scene = garden(&mut headless());
    let root = scene.root();
    let sail = scene.find(root, "mill/sails/sail").unwrap();
    // The hub is 12 above the mill's origin, and the sail points up from it, turned a quarter
    // clockwise to point right.
    assert_near(scene.transform(sail).apply([0.0, 0.0]), [100.0, 38.0]);
    let bounds = scene.bounds(sail).unwrap();
    assert_near([bounds.x, bounds.y], [100.0, 37.0]);
    assert_near([bounds.w, bounds.h], [10.0, 2.0]);

    let mill = scene.find(root, "mill").unwrap();
    scene.node_mut(mill).set_position([0.0, 0.0]);
    scene.node_mut(mill).set_scale([2.0, 2.0]);
    assert_near(scene.transform(sail).apply([0.0, 0.0]), [0.0, -24.0]);
    assert_eq!(
        scene.bounds(mill),
        Some(Rect::new(-16.0, -32.0, 32.0, 32.0)),
        "stretched about its pivot"
    );
    assert_eq!(scene.bounds(root), None, "plain nodes cover nothing");
}

#[test]
fn points_are_inside_nodes_as_they_are_turned() {
    let scene = garden(&mut headless());
    let gate = scene.find(scene.root(), "gate").unwrap();
    // 20 wide and 10 tall about its middle at (40, 40), turned to stand 10 wide and 20 tall.
    assert!(scene.contains(gate, [40.0, 40.0]));
    assert!(scene.contains(gate, [36.0, 49.0]));
    assert!(!scene.contains(gate, [49.0, 40.0]), "outside once turned");
    let bounds = scene.bounds(gate).unwrap();
    assert_near([bounds.x, bounds.y], [35.0, 30.0]);
    assert_near([bounds.w, bounds.h], [10.0, 20.0]);
    assert!(!scene.contains(scene.root(), [0.0, 0.0]));
}

#[test]
fn props_and_kinds_come_from_the_file() {
    let mut scene = garden(&mut headless());
    let root = scene.root();
    let gate = scene.find(root, "gate").unwrap();
    assert_eq!(
        scene.node(gate).props()["arrive"],
        Value::List(vec![Value::Number(3.0), Value::Number(4.0)])
    );
    assert_eq!(scene.node(gate).kind().name(), "area");
    assert_eq!(scene.node(gate).rotation(), FRAC_PI_2);

    let mill = scene.find(root, "mill").unwrap();
    let Kind::Sprite(sprite) = scene.node(mill).kind() else {
        panic!("the mill is a sprite");
    };
    assert_eq!(sprite.size, [16.0, 16.0], "its own size");
    assert_eq!(sprite.tint, Color::WHITE);

    let label = scene.find(root, "label").unwrap();
    let Kind::Text(text) = scene.node_mut(label).kind_mut() else {
        panic!("the label is text");
    };
    let short = text.layout().size()[0];
    text.set_text("Hello there");
    assert!(text.layout().size()[0] > short, "laid out again");
    assert_eq!(text.text(), "Hello there");
}

#[test]
fn hidden_nodes_and_what_they_hold_do_not_draw() {
    let mut headless = headless();
    let mut scene = garden(&mut headless);
    let root = scene.root();
    let bed = scene.find(root, "bed").unwrap();
    let flower = scene.find(root, "bed/flower").unwrap();
    // Only the flower, drawn alone: nothing, since the bed it's in is hidden.
    let frame = headless
        .render(64, 64, |canvas| scene.draw_node(canvas, flower))
        .unwrap();
    assert!(frame.pixels().chunks(4).all(|p| p[..3] == [0, 0, 0]));
    scene.node_mut(bed).set_visible(true);
    let frame = headless
        .render(64, 64, |canvas| scene.draw_node(canvas, flower))
        .unwrap();
    assert!(frame.pixels().chunks(4).any(|p| p[..3] != [0, 0, 0]));
}

#[test]
fn scenes_load_from_a_folder() {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("scene-tests/load");
    fs::create_dir_all(&folder).unwrap();
    fs::copy(TILESET, folder.join("tiles.png")).unwrap();
    fs::write(folder.join("garden.scene.luau"), GARDEN).unwrap();
    let mut headless = headless();
    let scene = Scene::load(headless.context(), &folder, "garden.scene.luau").unwrap();
    assert!(scene.find(scene.root(), "mill/sails").is_some());

    fs::write(
        folder.join("broken.scene.luau"),
        GARDEN.replace("cell = { 0, 1 }", "cell = { 99, 1 }"),
    )
    .unwrap();
    let err = Scene::load(headless.context(), &folder, "broken.scene.luau").unwrap_err();
    assert!(matches!(err, Error::Scene { .. }), "{err}");
    let message = err.to_string();
    assert!(message.contains("broken.scene.luau"), "{message}");
    assert!(
        message.contains("node `mill`: sprite (99, 1) is outside"),
        "{message}"
    );
}
