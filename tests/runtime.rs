//! Tests for the runtime that runs games written in Luau, with small games written to temporary
//! folders.

use std::{
    fs,
    path::{Path, PathBuf},
};

use wingine::{
    Error,
    graphics::Resolution,
    headless::{Image, Simulation},
    input::Key,
    lua::mlua::FromLuaMulti,
    runtime::{self, LibraryConfig, LuauGame},
};

/// Writes a game made of `files`, each a path in the game's folder and its contents, to a fresh
/// folder named after `test`.
fn game(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("runtime-tests")
        .join(test);
    if folder.exists() {
        fs::remove_dir_all(&folder).unwrap();
    }
    for (path, contents) in files {
        let path = folder.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    folder
}

/// A 20x20 canvas in a 20x20 window, so screenshots are canvas pixels.
const SMALL_WINDOW: &str =
    "return { width = 20, height = 20, resolution = { width = 20, height = 20 } }";

fn simulation(folder: &Path) -> Simulation<LuauGame> {
    let config = runtime::config(folder).expect("settings load");
    Simulation::new(&config.window, |ctx| LuauGame::load(ctx, folder)).expect("game starts")
}

/// The value of the Luau `expression` in the game.
fn eval<T: FromLuaMulti>(simulation: &Simulation<LuauGame>, expression: &str) -> T {
    let lua = simulation.game().scripts().lua();
    lua.load(format!("return {expression}"))
        .eval()
        .unwrap_or_else(|err| panic!("`{expression}`: {err}"))
}

/// The error that stopped the game, which a test expects there to be.
fn stopped_with(simulation: &Simulation<LuauGame>) -> String {
    let err = simulation.game().error().expect("the game stopped");
    err.to_string()
}

fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * image.width() + x) * 4) as usize;
    image.pixels()[offset..offset + 4].try_into().unwrap()
}

#[test]
fn settings_come_from_conf_luau() {
    let folder = game(
        "settings",
        &[(
            "conf.luau",
            "return {
                title = 'Settings',
                width = 640,
                height = 480,
                resizable = false,
                resolution = { width = 160, height = 120 },
                vsync = false,
                update_rate = 60,
                libraries = { 'physics', { name = 'particle_fx', optional = true } },
            }",
        )],
    );
    let config = runtime::config(&folder).unwrap();
    let window = &config.window;
    assert_eq!(window.title, "Settings");
    assert_eq!([window.width, window.height], [640, 480]);
    assert!(!window.resizable);
    assert!(!window.vsync);
    assert_eq!(window.update_rate, Some(60.0));
    assert_eq!(
        window.resolution,
        Resolution::Fixed {
            width: 160,
            height: 120
        }
    );
    let library = |name: &str, optional| LibraryConfig {
        name: name.into(),
        optional,
    };
    assert_eq!(
        config.libraries,
        [library("physics", false), library("particle_fx", true)]
    );
}

#[test]
fn without_settings_the_title_is_the_folders_name() {
    let folder = game("untitled", &[("main.luau", "")]);
    let config = runtime::config(&folder).unwrap();
    assert_eq!(config.window.title, "untitled");
    assert_eq!(config.window.resolution, Resolution::Window);
    assert!(config.window.vsync);
    assert_eq!(config.window.update_rate, None);
    assert!(config.libraries.is_empty());
}

#[test]
fn scripts_see_the_stats_of_the_frame_before() {
    let folder = game(
        "frame_stats",
        &[(
            "main.luau",
            "loading = graphics.frame_stats()
            seen = {}
            function update()
                table.insert(seen, graphics.frame_stats())
            end
            function draw()
                drawing = graphics.frame_stats()
                graphics.fill_rect(0, 0, 1, 1, '#ffffff')
                graphics.fill_rect(1, 1, 1, 1, '#ffffff')
            end",
        )],
    );
    let mut simulation = simulation(&folder);
    simulation.run_frames(2);
    assert_eq!(eval::<u32>(&simulation, "loading.quads"), 0);
    assert_eq!(eval::<f32>(&simulation, "seen[1].frame"), 0.0);
    assert_eq!(
        eval::<(u32, u32)>(&simulation, "seen[2].quads, seen[2].draw_calls"),
        (2, 1)
    );
    assert!(eval::<f32>(&simulation, "seen[2].frame") > 0.0);
    assert_eq!(eval::<u32>(&simulation, "drawing.quads"), 2);
}

#[test]
fn unknown_or_invalid_settings_are_errors() {
    let folder = game(
        "misspelled_setting",
        &[("conf.luau", "return { titel = 'x' }")],
    );
    let err = runtime::config(&folder).unwrap_err().to_string();
    assert!(
        err.contains("`titel`: runtime error: unknown setting"),
        "{err}"
    );

    let folder = game(
        "empty_resolution",
        &[(
            "conf.luau",
            "return { resolution = { width = 0, height = 10 } }",
        )],
    );
    let err = runtime::config(&folder).unwrap_err().to_string();
    assert!(err.contains("must be at least 1"), "{err}");

    for (test, conf, message) in [
        (
            "library_path",
            "return { libraries = { '../physics' } }",
            "`../physics` is not a library name",
        ),
        (
            "library_setting",
            "return { libraries = { { name = 'physics', optinal = true } } }",
            "not `optinal`",
        ),
        (
            "zero_update_rate",
            "return { update_rate = 0 }",
            "`update_rate`: runtime error: must be above 0",
        ),
    ] {
        let folder = game(test, &[("conf.luau", conf)]);
        let err = runtime::config(&folder).unwrap_err().to_string();
        assert!(err.contains(message), "{err}");
    }
}

#[test]
fn an_update_rate_updates_by_the_same_dt() {
    let folder = game(
        "update_rate",
        &[
            ("conf.luau", "return { update_rate = 30 }"),
            (
                "main.luau",
                "updates, draws, dts = 0, 0, {}
                function update(dt)
                    updates += 1
                    dts[dt] = true
                end
                function draw() draws += 1 end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    simulation.run_frames(4);
    assert_eq!(eval::<(u32, u32)>(&simulation, "updates, draws"), (2, 4));
    let dt: f32 = eval(&simulation, "next(dts)");
    assert!((dt - 1.0 / 30.0).abs() < 1e-6, "{dt}");
    assert!(eval::<bool>(&simulation, "next(dts, next(dts)) == nil"));
}

#[test]
fn update_and_tasks_run_each_frame_and_draw_draws() {
    let folder = game(
        "frames",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "frames = 0
                ticks = 0
                jumped = false
                input.bind('jump', { 'Space' })
                spawn(function()
                    while true do
                        ticks += 1
                        wait()
                    end
                end)

                function update(dt)
                    frames += 1
                    if input.pressed('jump') then
                        jumped = true
                    end
                end

                function draw()
                    graphics.clear('#ff0000')
                    graphics.fill_rect(0, 0, 10, 10, { r = 0, g = 0, b = 1 })
                end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    simulation.run_frames(3);
    assert!(!eval::<bool>(&simulation, "jumped"));
    simulation.tap(Key::Space);
    assert!(eval::<bool>(&simulation, "jumped"));
    assert_eq!(eval::<u32>(&simulation, "frames"), 4);
    assert_eq!(
        eval::<u32>(&simulation, "ticks"),
        5,
        "once when spawned, then after each update"
    );

    let image = simulation.screenshot().unwrap();
    assert_eq!(pixel(&image, 5, 5), [0, 0, 255, 255]);
    assert_eq!(pixel(&image, 15, 15), [255, 0, 0, 255]);
}

#[test]
fn tasks_started_in_update_resume_on_the_next_frame() {
    let folder = game(
        "spawn_in_update",
        &[(
            "main.luau",
            "frames = 0
            resumed_on = nil
            function update()
                frames += 1
                if frames == 2 then
                    spawn(function()
                        wait()
                        resumed_on = frames
                    end)
                end
            end",
        )],
    );
    let mut simulation = simulation(&folder);
    simulation.run_frames(2);
    assert_eq!(eval::<Option<u32>>(&simulation, "resumed_on"), None);
    simulation.step();
    assert_eq!(eval::<Option<u32>>(&simulation, "resumed_on"), Some(3));
}

#[test]
fn a_task_failing_as_it_starts_does_not_stop_the_game() {
    let folder = game(
        "spawn_failing",
        &[(
            "main.luau",
            "frames = 0
            function update()
                frames += 1
                if frames == 1 then
                    spawn(function() error('lost the map') end)
                end
            end",
        )],
    );
    let mut simulation = simulation(&folder);
    simulation.run_frames(3);
    assert!(simulation.game().error().is_none());
    assert_eq!(eval::<u32>(&simulation, "frames"), 3);
}

#[test]
fn an_error_stops_the_game_and_shows_it() {
    let folder = game(
        "failing_update",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "frames = 0
                function update()
                    frames += 1
                    if frames == 2 then
                        error('out of cheese')
                    end
                end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    simulation.run_frames(5);
    assert_eq!(
        eval::<u32>(&simulation, "frames"),
        2,
        "stopped at the error"
    );
    let err = stopped_with(&simulation);
    assert!(err.contains("main.luau:5: out of cheese"), "{err}");

    let image = simulation.screenshot().unwrap();
    assert_eq!(
        pixel(&image, 19, 19),
        [0x30, 0x10, 0x18, 255],
        "the error's background"
    );
}

#[test]
fn drawing_only_works_in_draw() {
    let folder = game(
        "drawing_in_update",
        &[(
            "main.luau",
            "function update() graphics.fill_rect(0, 0, 1, 1, '#ffffff') end",
        )],
    );
    let mut simulation = simulation(&folder);
    simulation.step();
    let err = stopped_with(&simulation);
    assert!(err.contains("drawing only works in `draw`"), "{err}");
}

#[test]
fn input_and_audio_do_not_work_in_draw() {
    let folder = game(
        "input_in_draw",
        &[("main.luau", "function draw() input.held('jump') end")],
    );
    let mut simulation = simulation(&folder);
    simulation.step();
    let err = stopped_with(&simulation);
    assert!(err.contains("not in `draw`"), "{err}");
}

#[test]
fn scripts_require_modules_relative_to_themselves() {
    let folder = game(
        "modules",
        &[
            (
                "main.luau",
                "local numbers = require('./lib/numbers')
                doubled = numbers.double(21)",
            ),
            (
                "lib/numbers.luau",
                "-- Modules see the engine's tables too.
                input.bind('confirm', { 'Enter' })
                return { double = function(n) return n * 2 end }",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    assert_eq!(eval::<u32>(&simulation, "doubled"), 42);
    simulation.tap(Key::Enter);
    assert!(simulation.context().input().released("confirm"));
}

#[test]
fn sounds_load_from_the_game_folder() {
    let folder = game(
        "sounds",
        &[(
            "main.luau",
            "local blip = audio.load('sounds/blip.wav')
            length = blip:duration()
            function update()
                if input.key_pressed('Space') then
                    audio.play(blip, { volume = 0.5 })
                end
            end",
        )],
    );
    let sounds = folder.join("sounds");
    fs::create_dir_all(&sounds).unwrap();
    let blip = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/sounds/blip.wav");
    fs::copy(blip, sounds.join("blip.wav")).unwrap();

    let mut simulation = simulation(&folder);
    assert!(eval::<f32>(&simulation, "length") > 0.0);
    simulation.tap(Key::Space);
    assert_eq!(simulation.context().audio().sounds_playing(), 1);
}

#[test]
fn fonts_measure_and_scale() {
    let folder = game(
        "fonts",
        &[(
            "main.luau",
            "local font = graphics.default_font()
            width, height = font:measure('Hi')
            big_width = font:scaled(2):measure('Hi')
            line_height = font:line_height()
            zero_scale_fails = not pcall(function() font:scaled(0) end)",
        )],
    );
    let simulation = simulation(&folder);
    let (width, height): (f32, f32) = eval(&simulation, "width, height");
    assert!(width > 0.0);
    assert_eq!(height, eval::<f32>(&simulation, "line_height"));
    assert_eq!(eval::<f32>(&simulation, "big_width"), width * 2.0);
    assert!(eval::<bool>(&simulation, "zero_scale_fails"));
}

/// Writes a PNG `width` by `height` pixels to `path`, with `pixels` row by row.
fn write_png(path: &Path, width: u32, height: u32, pixels: &[[u8; 4]]) {
    let image = image::RgbaImage::from_raw(width, height, pixels.concat()).unwrap();
    image.save(path).unwrap();
}

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

#[test]
fn sprites_load_from_sheets_and_draw() {
    let folder = game(
        "sprites",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "local sheet = graphics.load_sprite_sheet('art/sheet.png', 1, 1)
                columns, rows = sheet:size()
                local red, blue = sheet:sprite(0, 0), sheet:sprite(1, 0)
                width, height = blue:size()
                outside_fails = not pcall(function() sheet:sprite(2, 0) end)
                empty_tiles_fail = not pcall(function()
                    graphics.load_sprite_sheet('art/sheet.png', 0, 1)
                end)

                function draw()
                    graphics.clear('#000000')
                    -- Stretched, then at its own size, mirrored and tinted.
                    graphics.draw_sprite(blue, 0, 0, nil, 10, 20)
                    graphics.draw_sprite(red:flipped_x(), 12, 0, '#ffffff')
                end",
            ),
        ],
    );
    fs::create_dir_all(folder.join("art")).unwrap();
    write_png(&folder.join("art/sheet.png"), 2, 1, &[RED, BLUE]);

    let mut simulation = simulation(&folder);
    assert_eq!(eval::<(u32, u32)>(&simulation, "columns, rows"), (2, 1));
    assert_eq!(eval::<(f32, f32)>(&simulation, "width, height"), (1.0, 1.0));
    assert!(eval::<bool>(&simulation, "outside_fails"));
    assert!(eval::<bool>(&simulation, "empty_tiles_fail"));

    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(pixel(&image, 9, 19), BLUE);
    assert_eq!(pixel(&image, 10, 19), BLACK);
    assert_eq!(pixel(&image, 12, 0), RED);
    assert_eq!(pixel(&image, 13, 0), BLACK);
}

#[test]
fn sprites_draw_rotated_about_their_centers() {
    let folder = game(
        "rotated",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "local sheet = graphics.load_sprite_sheet('sheet.png', 1, 1)
                local red, blue = sheet:sprite(0, 0), sheet:sprite(1, 0)
                function draw()
                    graphics.clear('#000000')
                    -- 8x2, standing up as 2x8 about its center, (4, 5).
                    graphics.draw_sprite_rotated(blue, 0, 4, math.pi / 2, nil, 8, 2)
                    graphics.draw_sprite_rotated(red, 12, 0, 0, '#ffffff')
                end",
            ),
        ],
    );
    write_png(&folder.join("sheet.png"), 2, 1, &[RED, BLUE]);

    let mut simulation = simulation(&folder);
    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(pixel(&image, 3, 1), BLUE);
    assert_eq!(pixel(&image, 4, 8), BLUE);
    assert_eq!(pixel(&image, 0, 4), BLACK);
    assert_eq!(pixel(&image, 5, 5), BLACK);
    assert_eq!(pixel(&image, 12, 0), RED);
    assert_eq!(pixel(&image, 13, 0), BLACK);
}

#[test]
fn animations_show_their_frames_in_turn() {
    let folder = game(
        "animations",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "local sheet = graphics.load_sprite_sheet('sheet.png', 1, 1)
                local red, blue = sheet:sprite(0, 0), sheet:sprite(1, 0)
                blink = graphics.new_animation({ red, blue, red }, 0.25)
                once = blink:once()
                mirrored = blink:flipped_x():flipped_y()
                no_frames_fail = not pcall(graphics.new_animation, {}, 0.25)
                no_time_fails = not pcall(graphics.new_animation, { red }, 0)
                local time = 0

                function update(dt)
                    time += dt
                end

                function draw()
                    graphics.draw_sprite(blink:frame(time), 0, 0)
                end",
            ),
        ],
    );
    write_png(&folder.join("sheet.png"), 2, 1, &[RED, BLUE]);

    let mut simulation = simulation(&folder);
    let indices = "blink:frame_index(0), blink:frame_index(0.3), blink:frame_index(0.6), \
                   blink:frame_index(0.8)";
    assert_eq!(
        eval::<(u32, u32, u32, u32)>(&simulation, indices),
        (1, 2, 3, 1)
    );
    assert_eq!(eval::<u32>(&simulation, "once:frame_index(10)"), 3);
    assert_eq!(
        eval::<(u32, f32)>(&simulation, "blink:frame_count(), blink:duration()"),
        (3, 0.75)
    );
    assert_eq!(eval::<u32>(&simulation, "mirrored:frame_count()"), 3);
    assert!(eval::<bool>(
        &simulation,
        "once:is_finished(0.75) and not once:is_finished(0.7) and not blink:is_finished(100)"
    ));
    assert!(eval::<bool>(
        &simulation,
        "no_frames_fail and no_time_fails"
    ));

    simulation.step();
    assert_eq!(pixel(&simulation.screenshot().unwrap(), 0, 0), RED);
    simulation.run_for(0.3);
    assert_eq!(pixel(&simulation.screenshot().unwrap(), 0, 0), BLUE);
}

#[test]
fn sprite_sheets_only_load_while_the_game_loads_or_updates() {
    let folder = game(
        "sprites_in_draw",
        &[(
            "main.luau",
            "function draw() graphics.load_sprite_sheet('sheet.png', 1, 1) end",
        )],
    );
    let mut simulation = simulation(&folder);
    simulation.step();
    assert!(stopped_with(&simulation).contains("not in `draw`"));
}

#[test]
fn tilemaps_hold_tiles_in_layers_and_draw_them() {
    let folder = game(
        "tilemaps",
        &[
            (
                "conf.luau",
                "return { width = 30, height = 20, resolution = { width = 30, height = 20 } }",
            ),
            (
                "main.luau",
                "local white = graphics.load_sprite_sheet('white.png', 1, 1):sprite(0, 0)
                local tileset = tilemap.new_tileset(10, 10)
                local grass = tileset:add({ sprite = white, tint = '#ff0000' })
                local wall = tileset:add({ sprite = white, tint = '#0000ff', solid = true })
                map = tilemap.new(tileset, 3, 2)
                ground = map:add_layer()
                walls = map:add_layer()
                for row = 0, 1 do
                    for column = 0, 2 do
                        map:set(ground, column, row, grass)
                    end
                end
                map:set(walls, 1, 0, wall)
                map:set(walls, 2, 1, wall)
                map:set(walls, 2, 1, nil)

                function fails(f)
                    local ok, err = pcall(f)
                    return if ok then nil else tostring(err)
                end
                outside = fails(function() map:set(ground, 3, 0, grass) end)
                no_tile = fails(function() map:set(ground, 0, 0, 7) end)
                no_layer = fails(function() map:get(2, 0, 0) end)

                only_ground = false
                function draw()
                    graphics.clear('#000000')
                    graphics.draw_tilemap(map, if only_ground then ground else nil)
                end",
            ),
        ],
    );
    write_png(&folder.join("white.png"), 1, 1, &[WHITE]);

    let mut simulation = simulation(&folder);
    let check = |expression: &str| eval::<bool>(&simulation, expression);
    assert!(check(
        "map:get(walls, 1, 0) ~= nil and map:get(walls, 2, 1) == nil"
    ));
    assert!(check("map:is_solid(1, 0) and not map:is_solid(2, 1)"));
    assert!(check("map:is_solid(-1, 0) and not map:contains(-1, 0)"));
    assert_eq!(eval::<(u32, u32)>(&simulation, "map:size()"), (3, 2));
    assert_eq!(eval::<usize>(&simulation, "map:layer_count()"), 2);
    let bounds: (f32, f32, f32, f32) = eval(
        &simulation,
        "map:bounds().x, map:bounds().y, map:bounds().width, map:bounds().height",
    );
    assert_eq!(bounds, (0.0, 0.0, 30.0, 20.0));
    assert_eq!(
        eval::<(i32, i32)>(&simulation, "map:world_to_tile(15, 19.5)"),
        (1, 1)
    );
    assert_eq!(
        eval::<(f32, f32)>(&simulation, "map:tile_to_world(1.5, 1)"),
        (15.0, 10.0)
    );
    for (name, message) in [
        ("outside", "tile (3, 0) is outside a 3x2 map"),
        ("no_tile", "the map's tileset has no tile 7"),
        ("no_layer", "there's no layer 2 in a map of 2 layers"),
    ] {
        let err: String = eval(&simulation, name);
        assert!(err.contains(message), "{name}: {err}");
    }

    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(pixel(&image, 15, 5), BLUE, "the wall, over the ground");
    assert_eq!(pixel(&image, 25, 15), RED);
    simulation
        .game()
        .scripts()
        .load_source("test", "only_ground = true")
        .unwrap();
    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(pixel(&image, 15, 5), RED);
}

#[test]
fn grid_movers_walk_from_tile_to_tile() {
    let folder = game(
        "grid_movers",
        &[(
            "main.luau",
            "local mover = grid.new_mover(2, 2, 0.5)
            started = mover:step('right')
            column, row = mover:tile()
            mover:update(0.25)
            x, y = mover:position()
            moving = mover:is_moving()
            mover:update(0.25)
            arrived = not mover:is_moving()

            -- The blocked function sees where the mover would go, and may look at the mover.
            blocked_step = mover:step('up', function(c, r)
                checked = { c, r }
                local mc, mr = mover:tile()
                from = { mc, mr }
                return true
            end)
            facing = mover:facing()
            neighbor = { grid.neighbor('left', 0, 0) }
            opposite = grid.opposite('up')
            zero_step_time_fails = not pcall(function() grid.new_mover(0, 0, 0) end)
            bad_frame_times_fail = not pcall(function() mover:update(0 / 0) end)
                and not pcall(function() mover:update(-0.1) end)",
        )],
    );
    let simulation = simulation(&folder);
    assert!(eval::<bool>(&simulation, "started"));
    assert_eq!(eval::<(i32, i32)>(&simulation, "column, row"), (3, 2));
    assert_eq!(eval::<(f32, f32)>(&simulation, "x, y"), (2.5, 2.0));
    assert!(eval::<bool>(&simulation, "moving and arrived"));
    assert!(!eval::<bool>(&simulation, "blocked_step"));
    assert_eq!(
        eval::<(i32, i32, i32, i32)>(&simulation, "checked[1], checked[2], from[1], from[2]"),
        (3, 1, 3, 2)
    );
    assert_eq!(eval::<String>(&simulation, "facing"), "up", "turned");
    assert_eq!(
        eval::<(i32, i32)>(&simulation, "neighbor[1], neighbor[2]"),
        (-1, 0)
    );
    assert_eq!(eval::<String>(&simulation, "opposite"), "down");
    assert!(eval::<bool>(&simulation, "zero_step_time_fails"));
    assert!(eval::<bool>(&simulation, "bad_frame_times_fail"));
}

#[test]
fn tilemaps_too_big_to_hold_are_errors() {
    let folder = game(
        "huge_tilemaps",
        &[(
            "main.luau",
            "local tileset = tilemap.new_tileset(1, 1)
            local ok, err = pcall(function() tilemap.new(tileset, 100000, 100000) end)
            too_big = tostring(err)
            -- One layer of this fits, but not two.
            local map = tilemap.new(tileset, 4096, 2049)
            map:add_layer()
            ok, err = pcall(function() map:add_layer() end)
            second_layer = tostring(err)
            layers = map:layer_count()",
        )],
    );
    let simulation = simulation(&folder);
    let too_big: String = eval(&simulation, "too_big");
    assert!(too_big.contains("more than the 16777216"), "{too_big}");
    let second_layer: String = eval(&simulation, "second_layer");
    assert!(second_layer.contains("with 2 layers"), "{second_layer}");
    assert_eq!(eval::<usize>(&simulation, "layers"), 1);
}

#[test]
fn each_image_is_loaded_once() {
    let folder = game(
        "cached_images",
        &[(
            "main.luau",
            "graphics.load_sprite_sheet('sheet.png', 1, 1)
            function update()
                if reload then
                    reload = false
                    -- The same image, divided differently.
                    again = graphics.load_sprite_sheet('sheet.png', 2, 1)
                end
            end",
        )],
    );
    let sheet = folder.join("sheet.png");
    write_png(&sheet, 2, 1, &[RED, BLUE]);
    let mut simulation = simulation(&folder);
    // Gone from disk, it loads all the same.
    fs::remove_file(&sheet).unwrap();
    simulation
        .game()
        .scripts()
        .load_source("test", "reload = true")
        .unwrap();
    simulation.step();
    assert!(simulation.game().error().is_none());
    assert_eq!(eval::<(u32, u32)>(&simulation, "again:size()"), (1, 1));
}

#[test]
fn text_layouts_reveal_their_glyphs() {
    let folder = game(
        "text_layouts",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "local font = graphics.default_font()
                -- Just too narrow for the text on one line.
                max_width = font:measure('Hi there') - 1
                layout = font:layout('Hi there', max_width)
                lines = layout:line_count()
                glyphs = layout:glyph_count()
                width, height = layout:size()
                line_height = font:line_height()
                shown = 0

                function draw()
                    graphics.clear('#000000')
                    graphics.draw_text_layout(layout, 0, 0, '#ffffff', shown)
                end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    assert_eq!(eval::<usize>(&simulation, "lines"), 2, "wrapped");
    assert_eq!(
        eval::<usize>(&simulation, "glyphs"),
        7,
        "spaces don't count"
    );
    let (width, height): (f32, f32) = eval(&simulation, "width, height");
    assert!(width <= eval::<f32>(&simulation, "max_width"));
    assert_eq!(height, 2.0 * eval::<f32>(&simulation, "line_height"));

    let lit = |simulation: &mut Simulation<LuauGame>| {
        simulation.step();
        let image = simulation.screenshot().unwrap();
        image.pixels().chunks(4).filter(|p| p[0] > 0).count()
    };
    assert_eq!(lit(&mut simulation), 0, "none shown");
    simulation
        .game()
        .scripts()
        .load_source("test", "shown = 1.9")
        .unwrap();
    let one = lit(&mut simulation);
    simulation
        .game()
        .scripts()
        .load_source("test", "shown = nil")
        .unwrap();
    let all = lit(&mut simulation);
    assert!(0 < one && one < all, "{one} of {all}");
}

#[test]
fn the_camera_stays_inside_its_bounds() {
    let folder = game(
        "camera_bounds",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "function draw()
                    graphics.clear('#000000')
                    -- Centered on the world's corner, the view would show half off the world.
                    graphics.set_camera(0, 0, { x = 0, y = 0, width = 40, height = 40 })
                    graphics.fill_rect(0, 0, 2, 2, '#ff0000')
                end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(pixel(&image, 1, 1), RED);
    assert_eq!(pixel(&image, 11, 11), BLACK);
}

#[test]
fn games_that_fail_to_load_are_errors() {
    let config = wingine::WindowConfig::default();
    let missing = game("no_main", &[]);
    let result = Simulation::new(&config, |ctx| LuauGame::load(ctx, &missing));
    assert!(matches!(result, Err(Error::ReadFile { .. })));

    let failing = game("failing_main", &[("main.luau", "audio.load('nope.wav')")]);
    let result = Simulation::new(&config, |ctx| LuauGame::load(ctx, &failing));
    let err = result.err().expect("loading fails").to_string();
    assert!(err.contains("nope.wav"), "{err}");
}
