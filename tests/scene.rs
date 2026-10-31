//! Tests for scenes loaded to play: finding nodes, where they are, changing them, tilemaps,
//! instances, and adding and removing nodes. Reading and writing scene files is tested in
//! `src/scene`, and drawing scenes in `tests/screenshots.rs`.

use std::{collections::HashMap, f32::consts::FRAC_PI_2, fs, path::Path, rc::Rc};

use wingine::{
    Context, Error, Result,
    graphics::{Color, Texture},
    headless::Headless,
    math::Rect,
    scene::{Assets, Kind, Scene, SceneFile, TilesetFile, Value},
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

/// Scene and tileset files held in memory, by path, with every image the tileset's.
struct Files<'a> {
    ctx: &'a mut Context,
    files: HashMap<&'static str, &'static str>,
}

impl Assets for Files<'_> {
    fn texture(&mut self, _: &str) -> Result<Texture> {
        self.ctx.load_texture(TILESET)
    }

    fn scene(&mut self, path: &str) -> Result<Rc<SceneFile>> {
        let source = self.files.get(path).unwrap_or_else(|| panic!("no {path}"));
        SceneFile::parse(source, path).map(Rc::new)
    }

    fn tileset(&mut self, path: &str) -> Result<Rc<TilesetFile>> {
        let source = self.files.get(path).unwrap_or_else(|| panic!("no {path}"));
        TilesetFile::parse(source, path).map(Rc::new)
    }
}

/// The scene `path` among `files`, loaded.
fn load(
    headless: &mut Headless,
    files: &[(&'static str, &'static str)],
    path: &str,
) -> Result<Scene> {
    let ctx = headless.context();
    let font = ctx.default_font().clone();
    let mut assets = Files {
        ctx,
        files: files.iter().copied().collect(),
    };
    let file = assets.scene(path)?;
    Scene::new(&file, path, &font, &mut assets)
}

fn garden(headless: &mut Headless) -> Scene {
    load(
        headless,
        &[("garden.scene.luau", GARDEN)],
        "garden.scene.luau",
    )
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

/// Grass, and trees, which are solid.
const WORLD_TILESET: &str = r##"return {
    format = 1, tile_width = 16, tile_height = 16,
    sheets = { tiles = { path = "tiles.png", tile_width = 16, tile_height = 16 } },
    tiles = {
        grass = { sheet = "tiles", cell = { 5, 0 }, tint = "#3e6b3f" },
        tree = { sheet = "tiles", cell = { 0, 1 }, tint = "#6aa84f", solid = true, props = { text = "An oak." } },
    },
}"##;

/// A 3x2 map at (32, 16): grass, people, then trees on top.
const LEVEL: &str = r##"return {
    format = 1,
    children = {
        {
            name = "map", kind = "tilemap", x = 32, y = 16, tileset = "world.tileset.luau",
            columns = 3, rows = 2,
            children = {
                { name = "ground", kind = "tile_layer", legend = { ["."] = "grass" }, tiles = { "...", ".. " } },
                { name = "people", kind = "node" },
                { name = "trees", kind = "tile_layer", legend = { T = "tree" }, tiles = { "T  ", "   " } },
            },
        },
    },
}"##;

fn level(headless: &mut Headless, level: &'static str) -> Result<Scene> {
    let files = [
        ("world.tileset.luau", WORLD_TILESET),
        ("level.scene.luau", level),
    ];
    load(headless, &files, "level.scene.luau")
}

#[test]
fn tilemaps_hold_tiles_in_layers() {
    let mut scene = level(&mut headless(), LEVEL).unwrap();
    let root = scene.root();
    let map = scene.find(root, "map").unwrap();
    let ground = scene.find(root, "map/ground").unwrap();
    let trees = scene.find(root, "map/trees").unwrap();
    assert_eq!(scene.tile(ground, [0, 0]), Some("grass"));
    assert_eq!(scene.tile(ground, [2, 1]), None, "a space is an empty cell");
    assert_eq!(scene.tile(trees, [0, 0]), Some("tree"));
    assert_eq!(scene.tile(trees, [9, 9]), None, "outside the map");

    let (found, tilemap) = scene.tilemap(trees).unwrap();
    assert_eq!(found, map);
    assert!(
        tilemap.map.is_solid([0, 0]),
        "a tree on a layer above grass"
    );
    assert!(!tilemap.map.is_solid([1, 0]));
    assert!(tilemap.map.is_solid([3, 0]), "outside the map");
    assert_eq!(
        tilemap.tile_props("tree").unwrap()["text"],
        Value::String("An oak.".into())
    );
    assert_eq!(tilemap.tile_names(), ["grass", "tree"]);

    assert_eq!(scene.bounds(map), Some(Rect::new(32.0, 16.0, 48.0, 32.0)));
    assert!(scene.contains(trees, [33.0, 17.0]));
    assert!(!scene.contains(trees, [31.0, 17.0]));

    scene.set_tile(trees, [1, 1], Some("tree")).unwrap();
    assert_eq!(scene.tile(trees, [1, 1]), Some("tree"));
    scene.set_tile(trees, [0, 0], None).unwrap();
    assert_eq!(scene.tile(trees, [0, 0]), None);
    let err = scene.set_tile(trees, [0, 0], Some("rock")).unwrap_err();
    assert!(err.contains("there's no tile `rock`"), "{err}");
    let err = scene.set_tile(trees, [3, 0], Some("tree")).unwrap_err();
    assert!(err.contains("outside a 3x2 map"), "{err}");
}

#[test]
fn layers_name_tiles_their_tileset_has() {
    let broken = LEVEL.replace("T = \"tree\"", "T = \"rock\"").leak();
    let err = level(&mut headless(), broken).unwrap_err().to_string();
    assert!(
        err.contains("node `map/trees`: there's no tile `rock` in the tilemap's tileset"),
        "{err}"
    );
}

/// A villager: a body with a hat, and a shadow.
const VILLAGER: &str = r##"return {
    format = 1,
    sheets = { people = { path = "tiles.png", tile_width = 16, tile_height = 16 } },
    props = { talk = "hello", species = "human" },
    children = {
        {
            name = "body", kind = "sprite", sheet = "people", cell = { 25, 0 }, pivot = { 8, 16 },
            children = {
                {
                    name = "hat", kind = "rect", y = -18, width = 6, height = 2, pivot = { 3, 0 },
                    props = { brim = 1, style = "straw" },
                },
            },
        },
        { name = "shadow", kind = "node" },
    },
}"##;

/// A family: a mother, whose hat is off.
const FAMILY: &str = r##"return {
    format = 1,
    children = {
        {
            name = "mother", kind = "instance", scene = "villager.scene.luau",
            overrides = { ["body/hat"] = { visible = false } },
        },
    },
}"##;

/// A town of villagers, changed by overrides, and a family through an instance of its own.
const TOWN: &str = r##"return {
    format = 1,
    children = {
        {
            name = "elder", kind = "instance", x = 40, y = 30, scene = "villager.scene.luau",
            tags = { "talks" }, props = { talk = "elder" },
            overrides = {
                body = { tint = "#c8b8e8", cell = { 24, 0 } },
                ["body/hat"] = { color = "#ff0000", props = { brim = 3 } },
            },
            children = { { name = "lamp", kind = "node", x = 4 } },
        },
        {
            name = "child", kind = "instance", x = 80, scene = "villager.scene.luau",
            overrides = { body = { scale_x = 0.5, scale_y = 0.5 } },
        },
        {
            name = "family", kind = "instance", scene = "family.scene.luau",
            overrides = { ["mother/body"] = { tint = "#00ff00" } },
        },
    },
}"##;

fn town(headless: &mut Headless, town: &'static str) -> Result<Scene> {
    let files = [
        ("villager.scene.luau", VILLAGER),
        ("family.scene.luau", FAMILY),
        ("town.scene.luau", town),
    ];
    load(headless, &files, "town.scene.luau")
}

fn sprite(scene: &Scene, path: &str) -> (wingine::graphics::Sprite, Color) {
    let Kind::Sprite(sprite) = scene.node(scene.find(scene.root(), path).unwrap()).kind() else {
        panic!("{path} is a sprite");
    };
    (sprite.sprite.clone(), sprite.tint)
}

#[test]
fn instances_place_scenes_with_overrides() {
    let scene = town(&mut headless(), TOWN).unwrap();
    let root = scene.root();
    let elder = scene.find(root, "elder").unwrap();
    let node = scene.node(elder);
    assert_eq!(node.kind().name(), "node");
    assert_eq!(
        node.props()["talk"],
        Value::String("elder".into()),
        "its own props win"
    );
    assert_eq!(node.props()["species"], Value::String("human".into()));
    assert_eq!(node.tags(), ["talks"]);
    let children: Vec<&str> = node
        .children()
        .iter()
        .map(|&id| scene.node(id).name())
        .collect();
    assert_eq!(
        children,
        ["body", "shadow", "lamp"],
        "its own children last"
    );

    let (body, tint) = sprite(&scene, "elder/body");
    assert_eq!(tint, Color::rgb8(0xc8, 0xb8, 0xe8));
    assert_eq!(body.region().x, 24.0 * 16.0, "the cell overridden");
    let hat = scene.find(root, "elder/body/hat").unwrap();
    let Kind::Rect(rect) = scene.node(hat).kind() else {
        panic!("the hat is a rect");
    };
    assert_eq!(rect.color, Color::rgb8(0xff, 0, 0));
    let props = scene.node(hat).props();
    assert_eq!(props["brim"], Value::Number(3.0), "props change one by one");
    assert_eq!(props["style"], Value::String("straw".into()));
    assert_eq!(scene.transform(hat).apply([0.0, 0.0]), [40.0, 12.0]);

    let child_body = scene.find(root, "child/body").unwrap();
    assert_eq!(scene.node(child_body).scale(), [0.5, 0.5]);
    let (_, tint) = sprite(&scene, "child/body");
    assert_eq!(tint, Color::WHITE, "each instance changes only itself");

    let (_, tint) = sprite(&scene, "family/mother/body");
    assert_eq!(
        tint,
        Color::rgb8(0, 0xff, 0),
        "through an instance in an instance"
    );
    let mother_hat = scene.find(root, "family/mother/body/hat").unwrap();
    assert!(!scene.node(mother_hat).is_visible());
}

#[test]
fn instances_that_cannot_be_made_are_errors() {
    let mut headless = headless();
    let cases = [
        (
            TOWN.replace("[\"body/hat\"]", "[\"body/cap\"]"),
            "node `elder`: overriding `body/cap`: there's no node `body/cap` to override",
        ),
        (
            TOWN.replace("color = \"#ff0000\"", "tint = \"#ff0000\""),
            "a rect node has no field `tint`",
        ),
        (
            TOWN.replace("name = \"lamp\"", "name = \"shadow\""),
            "has a node named `shadow` already",
        ),
        (
            TOWN.replace("family.scene.luau", "town.scene.luau"),
            "`town.scene.luau` places itself",
        ),
    ];
    for (source, expected) in cases {
        let err = town(&mut headless, source.leak()).unwrap_err().to_string();
        assert!(err.contains(expected), "{err}");
    }
}

#[test]
fn nodes_are_added_and_removed() {
    let mut headless = headless();
    let mut scene = garden(&mut headless);
    let other = garden(&mut headless);
    let root = scene.root();

    let err = scene.add(root, &other, other.root(), None).unwrap_err();
    assert!(err.contains("takes a name"), "{err}");
    let copy = scene.add(root, &other, other.root(), Some("copy")).unwrap();
    assert_eq!(scene.path(copy), "copy");
    assert!(scene.find(root, "copy/mill/sails/sail").is_some());
    assert_eq!(
        scene.node(copy).props()["title"],
        Value::String("The garden".into()),
        "a scene's props come with its root"
    );
    let other_mill = other.find(other.root(), "mill").unwrap();
    let err = scene.add(root, &other, other_mill, None).unwrap_err();
    assert!(err.contains("a node named `mill` there already"), "{err}");

    // Part of a scene, copied into itself.
    let mill = scene.find(root, "mill").unwrap();
    let bed = scene.find(root, "bed").unwrap();
    let (part, from) = scene.copy_of(mill);
    let copied = scene.add(bed, &part, from, None).unwrap();
    assert_eq!(scene.path(copied), "bed/mill");
    assert!(scene.find(root, "bed/mill/sails/sail").is_some());

    let sails = scene.find(root, "mill/sails").unwrap();
    let sail = scene.find(root, "mill/sails/sail").unwrap();
    scene.remove(sails).unwrap();
    assert!(scene.get(sails).is_none());
    assert!(scene.get(sail).is_none(), "and what was inside it");
    assert!(scene.find(root, "mill/sails").is_none());
    assert!(scene.node(mill).children().is_empty());
    // New nodes take the removed ones' places, but the removed IDs still lead nowhere.
    let again = scene.add(mill, &other, other_mill, Some("again")).unwrap();
    assert!(scene.get(sails).is_none() && scene.get(sail).is_none());
    assert_eq!(scene.path(again), "mill/again");
    let err = scene.remove(root).unwrap_err();
    assert!(err.contains("root can't be removed"), "{err}");
}

#[test]
fn tile_layers_are_copied_only_with_their_tilemaps() {
    let mut headless = headless();
    let mut scene = level(&mut headless, LEVEL).unwrap();
    let source = level(&mut headless, LEVEL).unwrap();
    let root = scene.root();
    let layer = source.find(source.root(), "map/trees").unwrap();
    let err = scene.add(root, &source, layer, None).unwrap_err();
    assert!(err.contains("only be copied with its tilemap"), "{err}");
    let map = source.find(source.root(), "map").unwrap();
    scene.add(root, &source, map, Some("second")).unwrap();
    let trees = scene.find(root, "second/trees").unwrap();
    assert_eq!(scene.tile(trees, [0, 0]), Some("tree"));
    scene.set_tile(trees, [0, 0], None).unwrap();
    let first = scene.find(root, "map/trees").unwrap();
    assert_eq!(
        scene.tile(first, [0, 0]),
        Some("tree"),
        "the copy has a map of its own"
    );
}
