//! What physics costs, from Rust and from Luau. Run with `cargo bench --bench physics`.
//!
//! The Luau cases play a level of their own seen from the side, a tile map 120 by 60 tiles with
//! floors across it, crates resting on them, posts nested in groups and characters walking, or a
//! 3D view of boxes, and report the median time `update` took over [`FRAMES`] updates, once the
//! crates have come to rest. Each does something else beside stepping the world: nothing, setting a
//! tile that isn't solid, casting rays and overlapping boxes, or walking characters. The Rust cases
//! step a world of tumbling boxes by a sixtieth of a second, and by a 59.94 Hz display's frame.

use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use wingine::{
    FrameStats, Game, WindowConfig,
    headless::Simulation,
    math::{Affine, Rect},
    physics::{Body, BodyKind, Material, Pose, Shape, Solid, World},
    runtime::LuauGame,
};

/// Updates run before measuring, so the crates have come to rest and caches have warmed up.
const WARMUP: u32 = 300;
const FRAMES: u32 = 120;

const COLUMNS: usize = 120;
const ROWS: usize = 60;
/// Rows apart the floors are.
const FLOORS: usize = 10;
const CRATES_PER_FLOOR: usize = 30;
/// Groups of posts, each of [`POSTS`] posts, nested two deep.
const GROUPS: usize = 30;
const POSTS: usize = 10;
const WALKERS: usize = 20;
/// Boxes of the 3D view, in groups of ten.
const BOXES: usize = 300;
/// Boxes tumbling in the Rust cases.
const TUMBLING: usize = 300;

fn main() {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("physics-bench");
    fs::create_dir_all(&folder).unwrap();
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/quarry");

    println!("Luau, the median of {FRAMES} updates, each stepping the world:");
    let level_2d = level_2d();
    for (name, case, work) in [
        ("step", "nothing else", ""),
        ("tile", "setting a tile that isn't solid", SET_TILE),
        ("queries", "100 rays and 20 overlapping boxes", QUERIES),
        ("walks", "walking 20 characters", WALKS),
    ] {
        let game = folder.join(name);
        fs::create_dir_all(&game).unwrap();
        for file in ["quarry.tileset.luau", "kenney_1-bit-pack.png"] {
            fs::copy(examples.join(file), game.join(file)).unwrap();
        }
        fs::write(game.join("bench.scene.luau"), &level_2d).unwrap();
        fs::write(game.join("main.luau"), main_2d(work)).unwrap();
        report(case, update_time(&game));
    }
    let game = folder.join("3d");
    fs::create_dir_all(&game).unwrap();
    fs::write(game.join("bench.scene.luau"), level_3d()).unwrap();
    fs::write(game.join("main.luau"), MAIN_3D).unwrap();
    report("3D: 100 rays among 300 boxes", update_time(&game));

    println!("Rust, {TUMBLING} boxes tumbling, the mean of {FRAMES} steps:");
    for (case, dt) in [
        ("World::step(1 / 60)", 1.0 / 60.0),
        ("World::step(1 / 59.94), a 59.94 Hz display", 1.0 / 59.94),
        ("World::step(1.02 / 60), a frame of jitter", 1.02 / 60.0),
    ] {
        report(case, step_time(dt));
    }
    if cfg!(debug_assertions) {
        println!("(a debug build: run `cargo bench` for one built for speed)");
    }
}

fn report(case: &str, seconds: f32) {
    println!("  {case:<58}{:>8.3} ms", seconds * 1000.0);
}

/// The median time the Luau game in `game` took to update, once it's warmed up.
fn update_time(game: &Path) -> f32 {
    let config = WindowConfig {
        width: 320,
        height: 180,
        ..Default::default()
    };
    let mut simulation = Simulation::new(&config, |ctx| LuauGame::load(ctx, game))
        .expect("a GPU adapter is available");
    let time = median(&mut simulation, |stats| stats.update);
    if let Some(err) = simulation.game().error() {
        panic!("{}: {err}", game.display());
    }
    time
}

/// The median of `stat` over [`FRAMES`] frames, after warming up.
fn median<G: Game>(simulation: &mut Simulation<G>, stat: fn(&FrameStats) -> f32) -> f32 {
    simulation.run_frames(WARMUP);
    let mut values: Vec<f32> = (0..FRAMES)
        .map(|_| {
            simulation.step();
            stat(&simulation.context().frame_stats())
        })
        .collect();
    values.sort_by(f32::total_cmp);
    values[values.len() / 2]
}

/// The mean time a step of `dt` seconds took, of a world of boxes falling into a heap.
fn step_time(dt: f32) -> f32 {
    let mut world = World::new();
    world.set_length_unit(16.0);
    world.set_gravity([0.0, 160.0]);
    let ground = Rect::new(-400.0, 0.0, 800.0, 16.0);
    world.add(Affine::IDENTITY, ground, Solid::Whole);
    for i in 0..TUMBLING {
        let at = [
            (i % 30) as f32 * 18.0 - 270.0,
            -20.0 - (i / 30) as f32 * 20.0,
        ];
        world
            .add_body(&Body {
                kind: BodyKind::Dynamic,
                shape: Shape::Box(Rect::new(-8.0, -8.0, 16.0, 16.0)),
                pose: Pose::at(at),
                material: Material::default(),
                sensor: false,
            })
            .unwrap();
    }
    // A moment in, the boxes are landing on each other, and go on tumbling for the steps timed.
    for _ in 0..20 {
        world.step(dt);
    }
    let start = Instant::now();
    for _ in 0..FRAMES {
        world.step(dt);
    }
    start.elapsed().as_secs_f32() / FRAMES as f32
}

/// The 2D level: a tile map of floors with walls at its sides and rubble behind, crates on the
/// floors, posts in groups, and walkers.
fn level_2d() -> String {
    let rows = |cell: &dyn Fn(usize, usize) -> char| -> String {
        let mut rows = String::new();
        for row in 0..ROWS {
            let line: String = (0..COLUMNS).map(|column| cell(row, column)).collect();
            writeln!(rows, "                \"{line}\",").unwrap();
        }
        rows
    };
    // Floors every [`FLOORS`] rows, with gaps, and walls down the sides.
    let ground = rows(&|row, column| {
        let floor = row % FLOORS == FLOORS - 1 && column % 20 >= 2;
        if floor || column == 0 || column == COLUMNS - 1 {
            'B'
        } else {
            ' '
        }
    });
    let back = rows(&|row, column| {
        if (row * 7 + column * 3) % 11 == 0 {
            'r'
        } else {
            ' '
        }
    });
    let mut nodes = String::new();
    for floor in 0..ROWS / FLOORS {
        let top = ((floor + 1) * FLOORS - 1) as f32 * 16.0;
        for i in 0..CRATES_PER_FLOOR {
            let x = 40.0 + i as f32 * 60.0;
            let y = top - 12.0 - (i % 3) as f32 * 14.0;
            writeln!(
                nodes,
                "        {{ name = \"crate{floor}_{i}\", kind = \"rect\", x = {x}, y = {y}, \
                 width = 12, height = 12, color = \"#a07040\", tags = {{ \"crate\" }} }},"
            )
            .unwrap();
        }
    }
    for group in 0..GROUPS {
        let [x, y] = [
            (group % 10) as f32 * 180.0 + 30.0,
            (group / 10) as f32 * 300.0 + 40.0,
        ];
        let mut posts = String::new();
        for post in 0..POSTS {
            writeln!(
                posts,
                "                {{ name = \"post{post}\", kind = \"rect\", x = {}, width = 6, \
                 height = 6, color = \"#606060\", tags = {{ \"solid\" }} }},",
                post as f32 * 10.0
            )
            .unwrap();
        }
        writeln!(
            nodes,
            "        {{ name = \"group{group}\", kind = \"node\", x = {x}, y = {y}, children = {{\n\
             {{ name = \"inner\", kind = \"node\", x = 2, children = {{\n{posts}}} }},\n}} }},"
        )
        .unwrap();
    }
    for walker in 0..WALKERS {
        let floor = walker % (ROWS / FLOORS);
        let y = ((floor + 1) * FLOORS - 1) as f32 * 16.0;
        let x = 70.0 + (walker / (ROWS / FLOORS)) as f32 * 400.0;
        writeln!(
            nodes,
            "        {{ name = \"walker{walker}\", kind = \"node\", x = {x}, y = {y}, \
             tags = {{ \"walker\" }} }},"
        )
        .unwrap();
    }
    format!(
        "return {{
    format = 4,
    children = {{
        {{
            name = \"level\", kind = \"tilemap\", tileset = \"quarry.tileset.luau\",
            columns = {COLUMNS}, rows = {ROWS},
            children = {{
                {{ name = \"back\", kind = \"tile_layer\", legend = {{ r = \"rubble\" }}, tiles = {{
{back}                }} }},
                {{ name = \"ground\", kind = \"tile_layer\", legend = {{ B = \"brick\" }}, tiles = {{
{ground}                }} }},
            }},
        }},
{nodes}    }},
}}
"
    )
}

/// The 2D game, which steps the world each update after doing `work`.
fn main_2d(work: &str) -> String {
    format!(
        "local level = scene.load('bench.scene.luau')
        local world = physics.world()
        world:add(level:find('level'))
        for _, node in level:tagged('solid') do
            world:add(node)
        end
        for _, crate in level:tagged('crate') do
            world:add(crate, {{ body = 'dynamic', friction = 0.6 }})
        end
        local walkers = level:tagged('walker')
        for _, walker in walkers do
            world:add(walker, {{ character = {{ width = 8, height = 14, step = 4 }} }})
        end
        local back = level:find('level/back')
        local flip = false
        local time = 0

        function update(dt)
            time += dt
            {work}
            world:step(dt)
        end"
    )
}

/// Sets a tile of the layer behind, which isn't solid, and empties it again the next update.
const SET_TILE: &str = "
    flip = not flip
    back:set_tile(3, 3, if flip then 'rubble' else nil)";

/// Rays down from the top, and boxes over the level.
const QUERIES: &str = "
    for i = 0, 99 do
        world:ray((i * 37) % 1900 + 10, 5, 0, 1)
    end
    for i = 0, 19 do
        world:overlap_box({ x = (i * 97) % 1800, y = (i * 53) % 900, width = 64, height = 64 })
    end";

/// Each walker walking one way for a second, then the other, falling onto its floor.
const WALKS: &str = "
    for i, walker in walkers do
        local across = if math.floor(time + i) % 2 == 0 then 1 else -1
        world:walk(walker, across * 60 * dt, 4)
    end";

/// A 3D view of boxes in groups, each group turned.
fn level_3d() -> String {
    let mut groups = String::new();
    for group in 0..BOXES / 10 {
        let [x, z] = [
            (group % 6) as f32 * 8.0 - 20.0,
            (group / 6) as f32 * 8.0 - 20.0,
        ];
        let mut boxes = String::new();
        for i in 0..10 {
            writeln!(
                boxes,
                "                {{ name = \"box{i}\", kind = \"mesh\", shape = \"cuboid\", \
                 x = {}, z = {}, tags = {{ \"solid\" }} }},",
                (i % 5) as f32 * 1.5,
                (i / 5) as f32 * 1.5
            )
            .unwrap();
        }
        writeln!(
            groups,
            "            {{ name = \"group{group}\", kind = \"node\", x = {x}, z = {z}, \
             yaw = {}, children = {{\n{boxes}            }} }},",
            group as f32 * 0.3
        )
        .unwrap();
    }
    format!(
        "return {{
    format = 4,
    children = {{
        {{
            name = \"view\", kind = \"view3d\", width = 320, height = 180,
            camera = {{ position = {{ 0, 30, 40 }}, target = {{ 0, 0, 0 }} }},
            children = {{
{groups}            }},
        }},
    }},
}}
"
    )
}

/// The 3D game, casting rays down onto the boxes each update before stepping.
const MAIN_3D: &str = "
    local level = scene.load('bench.scene.luau')
    local world = physics.world_3d()
    for _, node in level:tagged('solid') do
        world:add(node)
    end
    local down = vector.create(0, -1, 0)

    function update(dt)
        for i = 0, 99 do
            world:ray(vector.create((i % 10) * 4 - 20, 10, math.floor(i / 10) * 4 - 20), down)
        end
        world:step(dt)
    end";
