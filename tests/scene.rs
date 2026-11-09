//! Tests for scenes loaded to play: finding nodes, where they are, changing them, tilemaps,
//! instances, and adding and removing nodes. Reading and writing scene files is tested in
//! `src/scene`, and drawing scenes in `tests/screenshots.rs`.

use std::{collections::HashMap, f32::consts::FRAC_PI_2, fs, path::Path, rc::Rc};

use wingine::{
    Context, Error, Result,
    graphics::{Color, Font, Mesh, Model, Texture},
    headless::Headless,
    math::Rect,
    scene::{Assets, Kind, Scene, SceneFile, Shape, TilesetFile, Value},
};

const TILESET: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/kenney_1-bit-pack_transparent-packed.png"
);

/// The examples' folder, which scenes' model and font files are in.
const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples");

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
    files: HashMap<&'a str, &'a str>,
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

    fn font(&mut self, path: &str) -> Result<Font> {
        self.ctx.load_font(Path::new(EXAMPLES).join(path))
    }

    fn model(&mut self, path: &str) -> Result<Model> {
        wingine::scene::load_model(self.ctx, &Path::new(EXAMPLES).join(path))
    }

    fn mesh(&mut self, shape: &Shape) -> Result<Mesh> {
        self.ctx.create_mesh(&shape.mesh_data())
    }
}

/// The scene `path` among `files`, loaded.
fn load(headless: &mut Headless, files: &[(&str, &str)], path: &str) -> Result<Scene> {
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

/// A screen: a bar along the bottom, inset by 4, with a score at its right end, and a menu in the
/// middle, with a group in it and a node in the group anchored to the menu's corner.
const SCREEN: &str = r##"return {
    format = 1,
    children = {
        {
            name = "bar", kind = "rect", y = -4, anchor = { 0, 1 }, width = -8, height = 12,
            stretch = { 1, 0 }, pivot = { -4, 12 },
            children = {
                {
                    name = "score", kind = "text", x = -2, anchor = { 1, 0.5 }, pivot = { 0, 5 },
                    text = "120", align = "right",
                },
            },
        },
        {
            name = "menu", kind = "area", anchor = { 0.5, 0.5 }, width = 80, height = 40,
            pivot = { 40, 20 },
            children = {
                {
                    name = "group", kind = "node", x = 10,
                    children = { { name = "corner", kind = "node", anchor = { 1, 1 } } },
                },
            },
        },
    },
}"##;

#[test]
fn nodes_anchor_and_stretch_to_their_parents() {
    let mut headless = headless();
    let mut scene = load(
        &mut headless,
        &[("screen.scene.luau", SCREEN)],
        "screen.scene.luau",
    )
    .unwrap();
    let root = scene.root();
    let [bar, score, menu, corner] = ["bar", "bar/score", "menu", "menu/group/corner"]
        .map(|path| scene.find(root, path).unwrap());
    assert_eq!(scene.size(), [0.0, 0.0], "until it's drawn");
    scene.set_size([200.0, 100.0]);

    assert_eq!(scene.bounds(bar), Some(Rect::new(4.0, 84.0, 192.0, 12.0)));
    assert_eq!(
        scene.node(bar).size(),
        Some([-8.0, 12.0]),
        "before stretching"
    );
    assert_eq!(scene.bounds(menu), Some(Rect::new(60.0, 30.0, 80.0, 40.0)));
    // The group passes the menu's rectangle on, carried along with it.
    assert_near(scene.transform(corner).apply([0.0, 0.0]), [150.0, 70.0]);
    // Right-aligned text ends at its origin, 2 in from the bar's right end, which is at 196.
    let [width, _] = scene.node(score).size().unwrap();
    assert_eq!(
        scene.rect(score),
        Some(Rect::new(-width, -5.0, width, 10.0))
    );
    assert_eq!(
        scene.bounds(score),
        Some(Rect::new(194.0 - width, 85.0, width, 10.0))
    );
    assert!(scene.contains(score, [193.0, 90.0]));

    // On a smaller canvas, they follow its edges and middle.
    let frame = headless.render(100, 60, |canvas| scene.draw(canvas));
    frame.unwrap();
    assert_eq!(scene.size(), [100.0, 60.0], "drawing fits it to the canvas");
    assert_eq!(scene.bounds(bar), Some(Rect::new(4.0, 44.0, 92.0, 12.0)));
    assert_eq!(scene.bounds(menu), Some(Rect::new(10.0, 10.0, 80.0, 40.0)));

    // Without its anchor, it's from the bar's origin, its bottom-left corner.
    scene.node_mut(score).set_anchor(None);
    assert_near(scene.transform(score).apply([0.0, 0.0]), [-2.0, 56.0]);
    // Moving the root moves everything, anchors and all.
    scene.node_mut(root).set_position([5.0, 0.0]);
    assert_eq!(scene.bounds(bar), Some(Rect::new(9.0, 44.0, 92.0, 12.0)));
}

/// A menu stacking its items down, and a bar along the bottom-right corner stacking across.
const STACKS: &str = r##"return {
    format = 3,
    children = {
        {
            name = "menu", kind = "stack", x = 10, y = 20, width = 100, gap = 2,
            padding = { 4, 3, 4, 5 },
            children = {
                { name = "a", kind = "rect", width = 20, height = 10 },
                { name = "b", kind = "rect", x = 1, anchor = { 1, 0 }, width = 30, height = 6 },
                { name = "hidden", kind = "rect", width = 50, height = 50, visible = false },
                {
                    name = "c", kind = "rect", width = 0, height = 4, stretch = { 1, 0 },
                    pivot = { 0, 4 },
                },
                { name = "d", kind = "area", anchor = { 0.5, 0 }, width = 10, height = 10 },
            },
        },
        {
            name = "bar", kind = "stack", x = -2, y = -2, anchor = { 1, 1 }, direction = "row",
            gap = 3, padding = 1,
            children = {
                { name = "icon", kind = "rect", anchor = { 0, 0.5 }, width = 8, height = 8 },
                {
                    name = "label", kind = "node",
                    children = {
                        { name = "text", kind = "text", text = "Hi" },
                        { name = "badge", kind = "rect", x = 2, y = 12, width = 4, height = 2 },
                    },
                },
                { name = "tall", kind = "area", width = 5, height = 20 },
            },
        },
    },
}"##;

#[test]
fn stacks_lay_out_their_children_one_after_another() {
    let mut headless = headless();
    let mut scene = load(
        &mut headless,
        &[("stacks.scene.luau", STACKS)],
        "stacks.scene.luau",
    )
    .unwrap();
    scene.set_size([200.0, 100.0]);
    let root = scene.root();
    let [menu, a, b, hidden, c, d, bar, icon, text, tall] = [
        "menu",
        "menu/a",
        "menu/b",
        "menu/hidden",
        "menu/c",
        "menu/d",
        "bar",
        "bar/icon",
        "bar/label/text",
        "bar/tall",
    ]
    .map(|path| scene.find(root, path).unwrap());
    // Down from inside the padding, the gap between each, each lined up across by its anchor:
    // `b` at the right, and moved 1 by its `x`; `c` stretched across, its pivot not moving it;
    // `d` in the middle. The hidden one takes no space.
    assert_eq!(scene.bounds(a), Some(Rect::new(14.0, 23.0, 20.0, 10.0)));
    assert_eq!(scene.bounds(b), Some(Rect::new(77.0, 35.0, 30.0, 6.0)));
    assert_eq!(scene.bounds(c), Some(Rect::new(14.0, 43.0, 92.0, 4.0)));
    assert_eq!(scene.bounds(d), Some(Rect::new(55.0, 49.0, 10.0, 10.0)));
    // As wide as it says, and as tall as what it holds and its padding.
    assert_eq!(scene.bounds(menu), Some(Rect::new(10.0, 20.0, 100.0, 44.0)));
    assert_eq!(scene.size_of(menu), Some([100.0, 44.0]));
    assert!(scene.node(menu).size().is_none(), "the scene measures it");

    // Hiding one closes up the space it took.
    scene.node_mut(b).set_visible(false);
    assert_eq!(scene.bounds(c), Some(Rect::new(14.0, 35.0, 92.0, 4.0)));
    assert_eq!(scene.bounds(d), Some(Rect::new(55.0, 41.0, 10.0, 10.0)));
    assert_eq!(scene.bounds(menu), Some(Rect::new(10.0, 20.0, 100.0, 36.0)));
    scene.node_mut(hidden).set_visible(true);
    assert_eq!(
        scene.bounds(hidden),
        Some(Rect::new(14.0, 35.0, 50.0, 50.0))
    );
    assert_eq!(scene.bounds(menu).unwrap().h, 88.0);

    // Across: the bar fits what it holds, and its anchor keeps its bottom-right corner 2 in from
    // the canvas's, however wide that is. The plain node takes the space its children cover.
    let [text_width, _] = scene.size_of(text).unwrap();
    let label = text_width.max(6.0);
    let width = 1.0 + 8.0 + 3.0 + label + 3.0 + 5.0 + 1.0;
    assert_eq!(
        scene.bounds(bar),
        Some(Rect::new(198.0 - width, 76.0, width, 22.0))
    );
    // The icon in the middle of the bar's height, the label's text from the top.
    assert_eq!(
        scene.bounds(icon),
        Some(Rect::new(199.0 - width, 83.0, 8.0, 8.0))
    );
    assert_near(
        scene.transform(text).apply([0.0, 0.0]),
        [210.0 - width, 77.0],
    );
    assert_eq!(scene.bounds(tall), Some(Rect::new(192.0, 77.0, 5.0, 20.0)));
    assert!(scene.contains(tall, [193.0, 90.0]));
}

#[test]
fn stacks_measure_transformed_groups_as_their_contents_are_drawn() {
    let source = r#"return { format = 3, children = {
        { name = 'list', kind = 'stack', children = {
            { name = 'row', kind = 'node', PLACEMENT children = {
                { name = 'group', kind = 'node', x = 3, y = 4, children = {
                    { name = 'item', CONTENT },
                    { name = 'hidden', kind = 'rect', width = 1000, height = 1000,
                      visible = false },
                } },
            } },
            { name = 'next', kind = 'rect', width = 4, height = 2 },
        } },
    } }"#;
    let mut headless = headless();
    for placement in [
        "",
        "scale_y = 0.5,",
        "scale_y = 2,",
        "rotation = 1.5707964,",
    ] {
        for content in [
            "kind = 'text', text = 'Hello'",
            "kind = 'rect', width = 20, height = 10",
        ] {
            let source = source
                .replace("PLACEMENT", placement)
                .replace("CONTENT", content);
            let scene = load(
                &mut headless,
                &[("group.scene.luau", &source)],
                "group.scene.luau",
            )
            .unwrap();
            let find = |path| scene.find(scene.root(), path).unwrap();
            let row = scene.extent(find("list/row")).unwrap();
            let next = scene.bounds(find("list/next")).unwrap();
            assert!(
                (next.y - row.bottom()).abs() < 1e-4,
                "{placement} {content}: row={row:?}, next={next:?}"
            );
            let list = scene.bounds(find("list")).unwrap();
            assert!((list.h - next.bottom()).abs() < 1e-4, "{list:?}, {next:?}");
        }
    }
}

/// A panel fitting a column of rows, each a plain node with a name and a value at its right, and
/// a panel showing a sprite, sliced.
const PANELS: &str = r##"return {
    format = 3,
    sheets = { tiles = { path = "tiles.png", tile_width = 16, tile_height = 16 } },
    children = {
        {
            name = "stats", kind = "panel", x = 4, y = 4, padding = { 6, 5, 6, 5 },
            color = "#14182c", border = 1, border_color = "#f2d3ab",
            children = {
                {
                    name = "rows", kind = "stack", width = 60,
                    children = {
                        {
                            name = "hp", kind = "node",
                            children = {
                                { name = "name", kind = "text", text = "HP" },
                                { name = "value", kind = "text", x = 52, text = "10" },
                            },
                        },
                        {
                            name = "coins", kind = "node",
                            children = {
                                { name = "name", kind = "text", text = "Coins" },
                                {
                                    name = "value", kind = "text", anchor = { 1, 0 },
                                    text = "3", align = "right",
                                },
                            },
                        },
                    },
                },
            },
        },
        {
            name = "frame", kind = "panel", sheet = "tiles", cell = { 0, 0 }, slice = 4,
            width = 40, height = 30, anchor = { 0.5, 0.5 },
        },
    },
}"##;

#[test]
fn panels_fit_what_they_hold() {
    let mut headless = headless();
    let mut scene = load(
        &mut headless,
        &[("panels.scene.luau", PANELS)],
        "panels.scene.luau",
    )
    .unwrap();
    scene.set_size([200.0, 100.0]);
    let root = scene.root();
    let [stats, hp, coins, coins_value, frame] = [
        "stats",
        "stats/rows/hp/name",
        "stats/rows/coins/name",
        "stats/rows/coins/value",
        "frame",
    ]
    .map(|path| scene.find(root, path).unwrap());
    // Two rows of text, each a line tall, inside the padding.
    assert_eq!(scene.bounds(stats), Some(Rect::new(4.0, 4.0, 72.0, 30.0)));
    assert_near(scene.transform(hp).apply([0.0, 0.0]), [10.0, 9.0]);
    assert_near(scene.transform(coins).apply([0.0, 0.0]), [10.0, 19.0]);
    // A row's children anchor across the space the stack gives it.
    assert_eq!(scene.bounds(coins_value).unwrap().right(), 70.0);
    let Kind::Panel(panel) = scene.node(stats).kind() else {
        panic!("a panel");
    };
    assert_eq!(panel.stack.padding, [6.0, 5.0, 6.0, 5.0]);
    // Its own size, lined up by its anchor in the middle of the canvas.
    assert_eq!(scene.bounds(frame), Some(Rect::new(80.0, 35.0, 40.0, 30.0)));

    let frame_image = headless
        .render(200, 100, |canvas| scene.draw(canvas))
        .unwrap();
    let pixel = |x: u32, y: u32| frame_image.pixel(x, y);
    assert_eq!(pixel(4, 4), [0xf2, 0xd3, 0xab, 0xff], "the border");
    assert_eq!(pixel(5, 5), [0x14, 0x18, 0x2c, 0xff], "the fill");
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
fn text_is_in_the_font_its_file_names_through_instances_too() {
    let label = r#"return { format = 3, children = {
        { name = "label", kind = "text", text = "Hi" },
    } }"#;
    let screen = r#"return { format = 3, children = {
        { name = "title", kind = "text", text = "Hi", font = "rpg/title.font.luau" },
        { name = "plain", kind = "instance", scene = "label.scene.luau" },
        {
            name = "fancy", kind = "instance", scene = "label.scene.luau",
            overrides = { label = { font = "rpg/title.font.luau", font_scale = 2 } },
        },
    } }"#;
    let mut headless = headless();
    let files = [("label.scene.luau", label), ("screen.scene.luau", screen)];
    let scene = load(&mut headless, &files, "screen.scene.luau").unwrap();
    let height = |path: &str| {
        let id = scene.find(scene.root(), path).unwrap();
        scene.bounds(id).unwrap().h
    };
    // The RPG title's font's lines are 15 apart, and the built-in font's 10.
    assert_eq!(height("title"), 15.0);
    assert_eq!(height("plain/label"), 10.0);
    assert_eq!(height("fancy/label"), 30.0, "overridden, and scaled");

    let missing = r#"return { format = 3, children = {
        { name = "title", kind = "text", text = "Hi", font = "rpg/missing.font.luau" },
    } }"#;
    let err = load(
        &mut headless,
        &[("missing.scene.luau", missing)],
        "missing.scene.luau",
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("missing.font.luau"), "{err}");
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

/// The source of a scene file holding a chain of `count` plain nodes nested one inside the
/// next, named n1, n2 and so on, with `inner`, the source of a node, inside the deepest.
fn chain(count: usize, inner: &str) -> String {
    let mut node = inner.to_owned();
    for i in (0..count).rev() {
        node = format!(
            "{{ name = \"n{}\", kind = \"node\", children = {{ {node} }} }}",
            i + 1
        );
    }
    format!("return {{ format = 1, children = {{ {node} }} }}")
}

#[test]
fn instances_do_not_nest_deeper_than_scenes_hold() {
    let mut headless = headless();
    // Three files of 80 nodes each, each placing the next: each alone is well within what a
    // scene holds, but placed one inside the next they nest 240 deep. (A file's own nesting is
    // held to about half that by the limit on how deeply its tables nest, so it takes three.)
    let placed =
        |scene: &str| format!(r#"{{ name = "placed", kind = "instance", scene = "{scene}" }}"#);
    let tail: &str = chain(80, "").leak();
    let middle: &str = chain(79, &placed("tail.scene.luau")).leak();
    let head: &str = chain(79, &placed("middle.scene.luau")).leak();
    let files = [
        ("tail.scene.luau", tail),
        ("middle.scene.luau", middle),
        ("head.scene.luau", head),
    ];
    load(&mut headless, &files, "tail.scene.luau").unwrap();
    let err = load(&mut headless, &files, "head.scene.luau")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("nodes nest more than 200 deep, too deep to place"),
        "{err}"
    );
}

#[test]
fn long_instance_chains_obey_the_node_limit_without_overflowing() {
    let mut headless = headless();
    // One file per node needs the most file-to-file expansion steps. Parsing the last file
    // must still have room on the stack, and 201 nodes must return an error rather than abort.
    for count in [199, 200, 201] {
        let files = (0..count)
            .map(|i| {
                let path = format!("part{i}.scene.luau");
                let source = if i + 1 == count {
                    r#"return { format = 1, children = { { name = "leaf", kind = "node" } } }"#
                        .to_owned()
                } else {
                    format!(
                        r#"return {{ format = 1, children = {{ {{ name = "placed", kind = "instance", scene = "part{}.scene.luau" }} }} }}"#,
                        i + 1
                    )
                };
                (path, source)
            })
            .collect::<Vec<_>>();
        let files = files
            .iter()
            .map(|(path, source)| (path.as_str(), source.as_str()))
            .collect::<Vec<_>>();
        let result = load(&mut headless, &files, "part0.scene.luau");
        if count <= 200 {
            let scene = result.unwrap();
            assert_eq!(scene.descendants(scene.root()).len(), count + 1);
        } else {
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("nodes nest more than 200 deep, too deep to place"),
                "{err}"
            );
        }
    }
}

#[test]
fn nodes_are_added_only_as_deep_as_scenes_hold() {
    let mut headless = headless();
    // A chain grows one node at a time, each inside the last, until scenes hold no more.
    let mut growing = load(
        &mut headless,
        &[("chain.scene.luau", chain(1, "").leak())],
        "chain.scene.luau",
    )
    .unwrap();
    let single = load(
        &mut headless,
        &[(
            "single.scene.luau",
            r#"return { format = 1, children = { { name = "single", kind = "node" } } }"#,
        )],
        "single.scene.luau",
    )
    .unwrap();
    let lone = single.find(single.root(), "single").unwrap();

    let root = growing.root();
    let mut deepest = growing.find(root, "n1").unwrap();
    let mut depth = 1;
    loop {
        match growing.add(deepest, &single, lone, Some("deeper")) {
            Ok(inside) => {
                deepest = inside;
                depth += 1;
            }
            Err(err) => {
                assert!(err.contains("too deep to add"), "{err}");
                break;
            }
        }
    }
    assert_eq!(depth, 200, "the chain grew as deep as scenes hold");
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

/// A puppet: a body with an arm and a face, and clips that pose them.
const PUPPET: &str = r##"return {
    format = 1,
    sheets = { tiles = { path = "tiles.png", tile_width = 16, tile_height = 16 } },
    children = {
        {
            name = "body", kind = "rect", width = 4, height = 10, pivot = { 2, 10 },
            children = {
                { name = "arm", kind = "rect", y = -8, width = 2, height = 6, pivot = { 1, 0 } },
                { name = "face", kind = "sprite", y = -16, sheet = "tiles", cell = { 0, 0 }, pivot = { 8, 8 } },
            },
        },
        { name = "label", kind = "text", text = "Hi" },
    },
    clips = {
        wave = {
            duration = 1,
            loop = true,
            tracks = {
                { node = "body/arm", property = "rotation", keys = { { 0, 0 }, { 0.5, 1 }, { 1, 0 } } },
                { node = "body", property = "y", interpolation = "step", keys = { { 0, 0 }, { 0.5, -2 } } },
                { node = "body/face", property = "cell", keys = { { 0, { 0, 0 } }, { 0.75, { 1, 0 } } } },
                { node = "body/face", property = "visible", keys = { { 0, true }, { 0.9, false } } },
            },
        },
        rest = {
            duration = 1,
            tracks = { { node = "body/arm", property = "rotation", keys = { { 0, -0.5 } } } },
        },
        broken = {
            duration = 1,
            tracks = {
                { node = "body/arm", property = "rotation", keys = { { 0, 2 } } },
                { node = "body/leg", property = "x", keys = { { 0, 1 } } },
            },
        },
        talk = {
            duration = 1,
            tracks = { { node = "label", property = "cell", keys = { { 0, { 0, 0 } } } } },
        },
        stare = {
            duration = 1,
            tracks = { { node = "body/face", property = "cell", keys = { { 0, { 999, 0 } } } } },
        },
    },
}"##;

/// Two puppets, instances of the puppet's file.
const SHOW: &str = r##"return {
    format = 1,
    children = {
        { name = "left", kind = "instance", x = 20, scene = "puppet.scene.luau" },
        { name = "right", kind = "instance", x = 60, scale_x = -1, scene = "puppet.scene.luau" },
    },
}"##;

#[test]
fn clips_pose_nodes() {
    let mut headless = headless();
    let mut scene = load(
        &mut headless,
        &[("puppet.scene.luau", PUPPET)],
        "puppet.scene.luau",
    )
    .unwrap();
    let clips = SceneFile::parse(PUPPET, "puppet.scene.luau").unwrap().clips;
    let root = scene.root();
    let body = scene.find(root, "body").unwrap();
    let arm = scene.find(root, "body/arm").unwrap();
    let face = scene.find(root, "body/face").unwrap();
    let face_x = |scene: &Scene| sprite(scene, "body/face").0.region().x;

    scene.pose(root, &clips["wave"], 0.25, 1.0).unwrap();
    assert_eq!(scene.node(arm).rotation(), 0.5);
    assert_eq!(scene.node(body).position(), [0.0, 0.0], "a step");
    assert_eq!(face_x(&scene), 0.0);
    scene.pose(root, &clips["wave"], 0.8, 1.0).unwrap();
    assert!((scene.node(arm).rotation() - 0.4).abs() < 1e-6);
    assert_eq!(scene.node(body).position(), [0.0, -2.0]);
    assert_eq!(face_x(&scene), 16.0, "the next cell");
    assert!(scene.node(face).is_visible());
    scene.pose(root, &clips["wave"], 1.95, 1.0).unwrap();
    assert!(!scene.node(face).is_visible(), "looped");

    // Weighted, numbers go part of the way, and the rest changes from a weight of a half.
    scene.pose(root, &clips["rest"], 0.0, 1.0).unwrap();
    scene.node_mut(body).set_position([0.0, 0.0]);
    scene.pose(root, &clips["wave"], 0.5, 0.25).unwrap();
    assert_eq!(scene.node(arm).rotation(), -0.125);
    assert_eq!(scene.node(body).position(), [0.0, -0.5]);
    assert!(!scene.node(face).is_visible());
    scene.pose(root, &clips["wave"], 0.8, 0.5).unwrap();
    assert!(scene.node(face).is_visible());
    assert_eq!(scene.node(body).position(), [0.0, -1.25]);
    scene.pose(root, &clips["wave"], 0.0, 0.0).unwrap();
    assert_eq!(scene.node(body).position(), [0.0, -1.25], "nothing at all");

    // Clips that don't fit change nothing.
    let rotation = scene.node(arm).rotation();
    let err = scene.pose(root, &clips["broken"], 0.0, 1.0).unwrap_err();
    assert!(
        err.contains("there's no node `body/leg` in the scene's root for the clip to pose"),
        "{err}"
    );
    assert_eq!(scene.node(arm).rotation(), rotation);
    let err = scene.pose(body, &clips["wave"], 0.0, 1.0).unwrap_err();
    assert!(
        err.contains("no node `body/arm` in node `body`"),
        "paths are from the node posed: {err}"
    );
    let err = scene.pose(root, &clips["talk"], 0.0, 1.0).unwrap_err();
    assert!(
        err.contains("node `label` is a text node, which has no cell"),
        "{err}"
    );
    let err = scene.pose(root, &clips["stare"], 0.0, 1.0).unwrap_err();
    assert!(
        err.contains("node `body/face`: sprite (999, 0) is outside"),
        "{err}"
    );
    let Kind::Sprite(sprite) = scene.node_mut(face).kind_mut() else {
        panic!("the face is a sprite");
    };
    sprite.set_cell([2, 0]).unwrap();
    assert_eq!(face_x(&scene), 32.0);
}

#[test]
fn clips_pose_every_instance_of_their_scene() {
    let mut headless = headless();
    let files = [("puppet.scene.luau", PUPPET), ("show.scene.luau", SHOW)];
    let mut scene = load(&mut headless, &files, "show.scene.luau").unwrap();
    let wave = &SceneFile::parse(PUPPET, "puppet.scene.luau").unwrap().clips["wave"];
    let root = scene.root();
    let left = scene.find(root, "left").unwrap();
    let right = scene.find(root, "right").unwrap();
    scene.pose(left, wave, 0.5, 1.0).unwrap();
    scene.pose(right, wave, 0.25, 1.0).unwrap();
    let rotation = |path| scene.node(scene.find(root, path).unwrap()).rotation();
    assert_eq!(rotation("left/body/arm"), 1.0);
    assert_eq!(rotation("right/body/arm"), 0.5);
    // The right one is mirrored, so its arm turns the other way on the canvas.
    let hand = |path| {
        scene
            .transform(scene.find(root, path).unwrap())
            .apply([0.0, 6.0])
    };
    let [left_x, _] = hand("left/body/arm");
    let [right_x, _] = hand("right/body/arm");
    assert!(left_x < 20.0 && right_x > 60.0, "{left_x}, {right_x}");
}

/// A cellar seen from straight above through an orthographic camera, so a world unit is 10
/// canvas pixels, `x` to the right and `z` down the canvas, with the world's origin at the
/// middle of the view, (70, 35) on the canvas: a floor, a crate in a stack turned a quarter turn,
/// the hero as a billboard, and a chair placed by an instance, after a rect of the 2D scene.
const CELLAR: &str = r##"return {
    format = 2,
    sheets = { tiles = { path = "tiles.png", tile_width = 16, tile_height = 16 } },
    children = {
        { name = "title", kind = "rect", width = 10, height = 10 },
        {
            name = "view", kind = "view3d", x = 20, y = 10, width = 100, height = 50,
            camera = { position = { 0, 10, 0 }, target = { 0, 0, 0 }, height = 5 },
            children = {
                { name = "floor", kind = "mesh", shape = "plane", size = { 10, 5 } },
                {
                    name = "stack", kind = "node", x = 2, yaw = 1.5707964,
                    children = {
                        { name = "crate", kind = "mesh", shape = "cuboid", x = 1, y = 0.5 },
                    },
                },
                { name = "hero", kind = "billboard", sheet = "tiles", cell = { 27, 0 }, x = -2 },
                { name = "chair", kind = "instance", scene = "chair.scene.luau", x = -3, z = 1 },
            },
        },
    },
    clips = {
        spin = {
            duration = 1,
            tracks = {
                { node = "view/stack", property = "yaw", keys = { { 0, 0 }, { 1, 3.1415927 } } },
                { node = "view/stack/crate", property = "scale_z", keys = { { 0, 2 } } },
                { node = "view/stack/crate", property = "y", keys = { { 0, 1 } } },
            },
        },
        tilt = { duration = 1, tracks = { { node = "view/stack", property = "rotation", keys = { { 0, 1 } } } } },
        lift = { duration = 1, tracks = { { node = "title", property = "z", keys = { { 0, 1 } } } } },
    },
}"##;

/// A chair, a file of its own for placing in views: a seat, and a plain node holding its back,
/// placed in 3D only once an instance places it in a view.
const CHAIR: &str = r##"return {
    format = 2,
    children = {
        { name = "seat", kind = "mesh", shape = "cuboid", y = 0.25, size = { 0.5, 0.5, 0.5 } },
        {
            name = "frame", kind = "node",
            children = { { name = "back", kind = "mesh", shape = "cuboid", y = 0.75, z = -0.2, size = { 0.5, 0.5, 0.1 } } },
        },
    },
}"##;

fn cellar(headless: &mut Headless) -> Scene {
    let files = [("chair.scene.luau", CHAIR), ("cellar.scene.luau", CELLAR)];
    let mut scene = load(headless, &files, "cellar.scene.luau").unwrap();
    scene.set_size([160.0, 90.0]);
    scene
}

fn assert_near3(actual: [f32; 3], expected: [f32; 3]) {
    assert!(
        (0..3).all(|i| (actual[i] - expected[i]).abs() < 1e-4),
        "{actual:?} is not {expected:?}"
    );
}

#[test]
fn nodes_in_3d_views_compose_their_placement() {
    let scene = cellar(&mut headless());
    let root = scene.root();
    let find = |path| scene.find(root, path).unwrap();
    let view = find("view");
    let crate_ = find("view/stack/crate");
    assert_eq!(
        scene.node(view).placement3(),
        None,
        "a view is in the 2D scene"
    );
    assert_eq!(scene.transform_3d(view), None);
    assert_eq!(scene.view_of(crate_), Some(view));
    assert_eq!(scene.view_of(find("title")), None);
    assert!(scene.node(find("view/floor")).placement3().is_some());

    // The stack turns the crate a quarter turn about it, from `+x` to `-z`.
    let at = |path| scene.transform_3d(find(path)).unwrap().translation;
    assert_near3(at("view/stack/crate"), [2.0, 0.5, -1.0]);
    let bounds = scene.bounds_3d(crate_).unwrap();
    assert_near3(bounds.min, [1.5, 0.0, -1.5]);
    assert_near3(bounds.max, [2.5, 1.0, -0.5]);
    assert_eq!(
        scene.bounds_3d(find("view/stack")),
        None,
        "a plain node shows nothing"
    );

    // A billboard stands in a flat box facing the camera, which looks straight down here, so
    // its front is toward the view's bottom edge, `+z`.
    let hero = scene.bounds_3d(find("view/hero")).unwrap();
    assert_near3(hero.min, [-2.5, 0.0, 0.0]);
    assert_near3(hero.max, [-1.5, 1.0, 0.0]);

    // The chair's nodes are placed in 3D in the view, the plain node holding its back too.
    assert_near3(at("view/chair/seat"), [-3.0, 0.25, 1.0]);
    assert_near3(at("view/chair/frame/back"), [-3.0, 0.75, 0.8]);
    assert!(scene.node(find("view/chair/frame")).placement3().is_some());

    // Nodes in 3D views cover nothing in the 2D scene, and the view what an area would.
    assert_eq!(scene.bounds(crate_), None);
    assert_eq!(scene.bounds(view), Some(Rect::new(20.0, 10.0, 100.0, 50.0)));
}

#[test]
fn views_pick_their_nodes_nearest_first() {
    let mut scene = cellar(&mut headless());
    let root = scene.root();
    let find = |scene: &Scene, path| scene.find(root, path).unwrap();
    let view = find(&scene, "view");
    let paths = |scene: &Scene, picked: Vec<_>| -> Vec<String> {
        picked.into_iter().map(|id| scene.path(id)).collect()
    };
    // The crate's top is nearer the camera than the floor under it.
    let on_crate = [70.0 + 20.0, 35.0 - 10.0];
    assert_eq!(
        paths(&scene, scene.pick(view, on_crate)),
        ["view/stack/crate", "view/floor"]
    );
    assert_eq!(
        paths(&scene, scene.pick(view, [75.0, 40.0])),
        ["view/floor"]
    );
    assert_eq!(
        paths(&scene, scene.pick(view, [70.0 - 30.0, 35.0 + 10.0])),
        ["view/chair/seat", "view/floor"]
    );
    assert_eq!(scene.pick(view, [5.0, 5.0]), [], "outside the view");
    assert_eq!(
        scene.pick(find(&scene, "title"), [5.0, 5.0]),
        [],
        "not a view"
    );
    let ray = scene.ray(view, on_crate).unwrap();
    assert_near3(ray.direction, [0.0, -1.0, 0.0]);
    assert_near3(
        ray.hit_plane([0.0; 3], [0.0, 1.0, 0.0]).unwrap(),
        [2.0, 0.0, -1.0],
    );

    // What doesn't show isn't picked.
    let stack = find(&scene, "view/stack");
    scene.node_mut(stack).set_visible(false);
    assert_eq!(paths(&scene, scene.pick(view, on_crate)), ["view/floor"]);
    scene.node_mut(view).set_visible(false);
    assert_eq!(scene.pick(view, on_crate), []);
}

#[test]
fn clips_pose_nodes_in_3d() {
    let mut scene = cellar(&mut headless());
    let clips = SceneFile::parse(CELLAR, "cellar.scene.luau").unwrap().clips;
    let root = scene.root();
    scene.pose(root, &clips["spin"], 0.5, 1.0).unwrap();
    let crate_ = scene.find(root, "view/stack/crate").unwrap();
    let placement = scene.node(crate_).placement3().unwrap();
    assert_eq!(placement.scale_z, 2.0);
    assert_eq!(scene.node(crate_).position(), [1.0, 1.0]);
    let stack = scene.find(root, "view/stack").unwrap();
    assert!((scene.node(stack).placement3().unwrap().yaw - 1.5707964).abs() < 1e-6);
    assert_near3(
        scene.transform_3d(crate_).unwrap().translation,
        [2.0, 1.0, -1.0],
    );

    let err = scene.pose(root, &clips["tilt"], 0.0, 1.0).unwrap_err();
    assert!(
        err.contains("node `view/stack` is placed in 3D, where it turns by yaw, pitch and roll"),
        "{err}"
    );
    let err = scene.pose(root, &clips["lift"], 0.0, 1.0).unwrap_err();
    assert!(
        err.contains("node `title` isn't in a 3D view, so it has no z"),
        "{err}"
    );
}

#[test]
fn only_views_hold_nodes_in_3d() {
    let mut headless = headless();
    let scene = |children: &str| {
        format!(
            "return {{ format = 2, sheets = {{ tiles = {{ path = 'tiles.png', tile_width = 16, \
             tile_height = 16 }} }}, children = {{ {children} }} }}"
        )
    };
    let view = |children: &str| {
        scene(&format!(
            "{{ name = 'view', kind = 'view3d', width = 10, height = 10, camera = {{ position = \
             {{ 0, 1, 2 }}, target = {{ 0, 0, 0 }} }}, children = {{ {children} }} }}"
        ))
    };
    let cases = [
        (
            scene("{ name = 'box', kind = 'mesh', shape = 'cuboid' }"),
            "node `box`: a mesh node is only in a 3D view: put it inside a view3d node",
        ),
        (
            scene("{ name = 'chair', kind = 'instance', scene = 'chair.scene.luau' }"),
            "node `chair/seat`: a mesh node is only in a 3D view",
        ),
        (
            view("{ name = 'sign', kind = 'rect', width = 1, height = 1 }"),
            "node `view/sign`: a rect node can't be in a 3D view",
        ),
        (
            view(
                "{ name = 'inner', kind = 'view3d', width = 1, height = 1, camera = { position = { 0, 1, 2 }, target = { 0, 0, 0 } } }",
            ),
            "node `view/inner`: a view3d node can't be in a 3D view",
        ),
        (
            view("{ name = 'lamp', kind = 'instance', scene = 'lamp.scene.luau' }"),
            "node `view/lamp/post`: a node in a 3D view turns by `yaw`, `pitch` and `roll`, \
             and has no `rotation` or `anchor`",
        ),
        (
            scene("{ name = 'turned', kind = 'node', yaw = 1 }"),
            "node `turned`: only nodes in 3D views are placed in 3D",
        ),
    ];
    let lamp =
        "return { format = 2, children = { { name = 'post', kind = 'node', rotation = 1 } } }";
    for (source, expected) in cases {
        let files = [
            ("chair.scene.luau", CHAIR),
            ("lamp.scene.luau", lamp),
            ("test.scene.luau", &source),
        ];
        let err = load(&mut headless, &files, "test.scene.luau")
            .unwrap_err()
            .to_string();
        assert!(err.contains(expected), "{source}\n{err}");
    }
}

#[test]
fn nodes_in_3d_are_added_only_to_views() {
    let mut scene = cellar(&mut headless());
    let root = scene.root();
    let view = scene.find(root, "view").unwrap();
    let crate_ = scene.find(root, "view/stack/crate").unwrap();
    let (copy, from) = scene.copy_of(crate_);
    let added = scene.add(view, &copy, from, Some("spare")).unwrap();
    assert!(scene.node(added).placement3().is_some());
    assert_eq!(scene.view_of(added), Some(view));
    let err = scene.add(root, &copy, from, Some("spare")).unwrap_err();
    assert!(
        err.contains("a mesh node placed in 3D can only be copied into a 3D view"),
        "{err}"
    );
    let title = scene.find(root, "title").unwrap();
    let (copy, from) = scene.copy_of(title);
    let err = scene.add(view, &copy, from, None).unwrap_err();
    assert!(
        err.contains("a rect node of the 2D scene can't be copied into a 3D view"),
        "{err}"
    );
}

/// An override giving a mesh showing no sprite a sheet names a sheet of the file the mesh is from,
/// even through an instance inside the file the instance places.
#[test]
fn overridden_meshes_take_sheets_of_their_own_files() {
    let mut headless = headless();
    let jar = "return { format = 2, \
               sheets = { glass = { path = 'glass.png', tile_width = 16, tile_height = 16 } }, \
               children = { { name = 'jar', kind = 'mesh', shape = 'sphere' } } }";
    // The shelf has a sheet of the same name, of other cells, which the jar's isn't.
    let shelf = "return { format = 2, \
                 sheets = { glass = { path = 'glass.png', tile_width = 8, tile_height = 8 } }, \
                 children = { { name = 'jar', kind = 'instance', scene = 'jar.scene.luau' } } }";
    let room = "return { format = 2, children = { { name = 'view', kind = 'view3d', width = 10, \
                height = 10, camera = { position = { 0, 1, 2 }, target = { 0, 0, 0 } }, \
                children = { { name = 'shelf', kind = 'instance', scene = 'shelf.scene.luau', \
                overrides = { ['jar/jar'] = { sheet = 'glass', cell = { 1, 0 } } } } } } } }";
    let files = [
        ("jar.scene.luau", jar),
        ("shelf.scene.luau", shelf),
        ("room.scene.luau", room),
    ];
    let scene = load(&mut headless, &files, "room.scene.luau").unwrap();
    let jar = scene.find(scene.root(), "view/shelf/jar/jar").unwrap();
    let Kind::Mesh(mesh) = scene.node(jar).kind() else {
        panic!("the jar is a mesh");
    };
    let sprite = mesh
        .sprite
        .as_ref()
        .expect("the override gives it a sprite");
    assert_eq!((sprite.width(), sprite.height()), (16.0, 16.0));
}

#[test]
#[cfg(feature = "models")]
fn model_nodes_show_their_files() {
    let mut headless = headless();
    let source = "return { format = 2, children = { { name = 'view', kind = 'view3d', width = 10, \
                  height = 10, camera = { position = { 0, 1, 2 }, target = { 0, 0, 0 } }, \
                  children = { { name = 'barrel', kind = 'model', model = 'models/barrel.glb', \
                  x = 3 } } } } }";
    let scene = load(
        &mut headless,
        &[("test.scene.luau", source)],
        "test.scene.luau",
    )
    .unwrap();
    let barrel = scene.find(scene.root(), "view/barrel").unwrap();
    let Kind::Model(model) = scene.node(barrel).kind() else {
        panic!("the barrel is a model");
    };
    let own = model.model.bounds().unwrap();
    let placed = scene.bounds_3d(barrel).unwrap();
    assert_near3(placed.min, [own.min[0] + 3.0, own.min[1], own.min[2]]);
    assert_near3(placed.max, [own.max[0] + 3.0, own.max[1], own.max[2]]);
}
