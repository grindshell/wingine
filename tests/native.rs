//! Tests for native libraries in the Luau runtime. Most link small libraries into the test, and
//! one builds the `native-particles` example and loads it from a game's `native` folder.

use std::{
    cell::RefCell,
    env::consts::{DLL_PREFIX, DLL_SUFFIX},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

use wingine::{
    Error,
    headless::Simulation,
    input::Key,
    runtime::{self, LibraryConfig, LuauGame},
};
use wingine_native::{
    Affine3, BillboardDraw, Blend, Buffer, Call, Camera3d, Class, Color, Context, Library,
    Lighting, Mesh, MeshDraw, MeshVertex, Rect, Registry, Result, Size, Sprite, SpriteDraw, Vec3,
    View3d, abi,
};

/// Writes a game made of `files`, each a path in the game's folder and its contents, to a fresh
/// folder named after `test`.
fn game(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("native-tests")
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

/// Settings for a game that loads `libraries`, in a 20x20 window whose screenshots are canvas
/// pixels.
fn conf(libraries: &[&str]) -> String {
    let libraries = libraries
        .iter()
        .map(|name| format!("'{name}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "return {{
            width = 20,
            height = 20,
            resolution = {{ width = 20, height = 20 }},
            libraries = {{ {libraries} }},
        }}"
    )
}

fn start(folder: &Path) -> Result<Simulation<LuauGame>, Error> {
    let config = runtime::config(folder).expect("settings load");
    Simulation::new(&config.window, |ctx| {
        LuauGame::load_with(ctx, folder, &config)
    })
}

fn simulation(folder: &Path) -> Simulation<LuauGame> {
    start(folder).expect("game starts")
}

/// A library that records what happens to it, for tests to look at through Luau.
struct Probe {
    log: Vec<String>,
    canvas_size: [f32; 2],
    /// The error its hook after `update` returns, if it fails.
    failure: Option<String>,
    /// A white pixel.
    white: Sprite,
}

/// A number scripts hold as an object.
struct Counter(i64);

impl Class for Counter {
    const NAME: &str = "Counter";
}

impl Library for Probe {
    const NAME: &str = "probe";

    fn new(ctx: &mut Context) -> Result<Self> {
        Ok(Self {
            log: Vec::new(),
            canvas_size: ctx.canvas_size(),
            failure: None,
            white: ctx.create_sprite(1, 1, &[255; 4])?,
        })
    }

    fn register(registry: &mut Registry<Self>) {
        registry
            .function("add", |_, _, (a, b): (f64, f64)| Ok(a + b))
            .function("byte", |_, _, byte: u8| Ok(byte))
            .function(
                "describe",
                |_, _, (flag, text, color, rect, missing): (bool, String, Color, Rect, Option<f64>)| {
                    Ok(format!(
                        "{flag} {text} {} {} {:?}",
                        color.g,
                        rect.w,
                        missing
                    ))
                },
            )
            .function("pair", |_, _, ()| Ok(("one", 2)))
            .function("canvas_size", |probe, _, ()| {
                Ok((probe.canvas_size[0], probe.canvas_size[1]))
            })
            .function("fail", |_, _, message: String| -> Result<()> {
                Err(wingine_native::Error::new(message))
            })
            .function("panic", |_, _, ()| -> Result<()> { panic!("on purpose") })
            .function("fail_after_update", |probe, _, message: String| {
                probe.failure = Some(message);
                Ok(())
            })
            .function("mark", |probe, _, name: String| {
                probe.log.push(name);
                Ok(())
            })
            .function("log", |probe, _, ()| Ok(probe.log.join(" ")))
            .function("held", |_, call, action: String| {
                Ok(call.context()?.held(&action))
            })
            .function("fill", |_, call, color: Color| {
                call.canvas()?.fill_rect(Rect::new(0.0, 0.0, 1.0, 1.0), color);
                Ok(())
            })
            .function("blend", |_, call, mode: u32| {
                call.canvas()?.set_blend(Blend(mode))
            })
            .function("stamp", |probe, call, ()| {
                let draws = [
                    SpriteDraw {
                        tint: Color::rgb(1.0, 0.0, 0.0),
                        ..SpriteDraw::new(Rect::new(0.0, 0.0, 2.0, 2.0))
                    },
                    // 8x2, standing up as 2x8 about its center, (14, 5).
                    SpriteDraw {
                        angle: std::f32::consts::FRAC_PI_2,
                        ..SpriteDraw::new(Rect::new(10.0, 4.0, 8.0, 2.0))
                    },
                ];
                call.canvas()?.draw_sprites(&probe.white, &draws);
                Ok(())
            })
            // A square a unit across facing +z, centered on its origin, colored `color`.
            .function("square", |_, call, color: Color| {
                let normal = Vec3::new(0.0, 0.0, 1.0);
                let vertices = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]].map(|[x, y]| {
                    MeshVertex {
                        color,
                        ..MeshVertex::new(Vec3::new(x, y, 0.0), normal)
                    }
                });
                call.context()?
                    .create_mesh(&vertices, &[0, 1, 2, 0, 2, 3])
            })
            .function("bad_mesh", |_, call, ()| {
                let vertex = MeshVertex::new(Vec3::ZERO, Vec3::Y);
                call.context()?.create_mesh(&[vertex], &[0, 1, 2])
            })
            .function("midpoint", |_, _, (a, b): (Vec3, Vec3)| {
                Ok(Vec3::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0, (a.z + b.z) / 2.0))
            })
            // A view of the middle of the world, 2 units across, as `FLAT_VIEW` in the runtime's
            // tests, over the whole 20x20 canvas.
            .function("view", |_, call, ()| {
                let camera = Camera3d::orthographic(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, 2.0);
                let view = View3d {
                    lighting: Lighting::UNLIT,
                    ..View3d::new(camera, Rect::new(0.0, 0.0, 20.0, 20.0))
                };
                call.canvas()?.begin_3d(&view)
            })
            .function("bad_view", |_, call, ()| {
                let camera = Camera3d {
                    near: 0.0,
                    ..Camera3d::perspective(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, 0.8)
                };
                let view = View3d::new(camera, Rect::new(0.0, 0.0, 20.0, 20.0));
                call.canvas()?.begin_3d(&view)
            })
            // In the view begun last: `mesh` red on the left and green on the right, and a blue
            // billboard below them, in front.
            .function("scene", |probe, call, mesh: Mesh| {
                let at = |x| Affine3::from_translation(Vec3::new(x, 0.0, 0.0));
                let meshes = [
                    MeshDraw {
                        tint: Color::rgb(1.0, 0.0, 0.0),
                        ..MeshDraw::new(at(-0.5))
                    },
                    MeshDraw {
                        tint: Color::rgb(0.0, 1.0, 0.0),
                        ..MeshDraw::new(at(0.5))
                    },
                ];
                let billboard = BillboardDraw {
                    tint: Color::rgb(0.0, 0.0, 1.0),
                    ..BillboardDraw::new(Vec3::new(0.0, -1.0, 1.0), Size::new(0.4, 0.4))
                };
                let canvas = call.canvas()?;
                canvas.draw_meshes(&mesh, None, &meshes)?;
                canvas.draw_billboards(&probe.white, &[billboard])
            })
            // In the view begun last: `mesh` red on the left, and a blue billboard below it,
            // each glowing `glow`, and `mesh` green on the right, which doesn't.
            .function("glowing_scene", |probe, call, (mesh, glow): (Mesh, Color)| {
                let at = |x| Affine3::from_translation(Vec3::new(x, 0.0, 0.0));
                let red = MeshDraw {
                    tint: Color::rgb(1.0, 0.0, 0.0),
                    ..MeshDraw::new(at(-0.5))
                };
                let green = MeshDraw {
                    tint: Color::rgb(0.0, 1.0, 0.0),
                    ..MeshDraw::new(at(0.5))
                };
                let billboard = BillboardDraw {
                    tint: Color::rgb(0.0, 0.0, 1.0),
                    ..BillboardDraw::new(Vec3::new(0.0, -1.0, 1.0), Size::new(0.4, 0.4))
                };
                let canvas = call.canvas()?;
                canvas.draw_meshes_glowing(&mesh, None, &[red], glow)?;
                canvas.draw_billboards_glowing(&probe.white, &[billboard], glow)?;
                canvas.draw_meshes(&mesh, None, &[green])
            })
            .function("reversed", |_, _, bytes: Buffer| {
                Ok(Buffer(bytes.0.into_iter().rev().collect()))
            })
            .function("byte_sum", |_, call, ()| {
                let bytes = call.buffer(0)?;
                Ok(bytes.iter().map(|&byte| u32::from(byte)).sum::<u32>())
            })
            // probe.fill_bytes(buffer, offset, byte, count)
            .function("fill_bytes", |_, call, ()| {
                let (offset, byte, count): (usize, u8, usize) =
                    (call.arg(1)?, call.arg(2)?, call.arg(3)?);
                call.write_buffer(0, offset, &vec![byte; count])
            })
            .function("doubled", |_, _, numbers: Vec<f64>| {
                Ok(numbers.into_iter().map(|n| n * 2.0).collect::<Vec<_>>())
            })
            .function("words", |_, _, text: String| {
                Ok(text.split(' ').map(str::to_owned).collect::<Vec<_>>())
            })
            // How many words each row has, and the argument after them, which may be missing.
            .function(
                "lengths",
                |_, _, (rows, after): (Vec<Vec<String>>, Option<u32>)| {
                    Ok((rows.iter().map(Vec::len).collect::<Vec<_>>(), after))
                },
            )
            .function("grid", |_, _, (rows, columns): (u32, u32)| {
                Ok((0..rows)
                    .map(|row| (0..columns).map(|column| row * 10 + column).collect())
                    .collect::<Vec<Vec<u32>>>())
            })
            .function("new_counter", |_, _, start: i64| Ok(Counter(start)))
            .function("total", |_, call, ()| {
                let (a, b) = (call.object::<Counter>(0)?, call.object::<Counter>(1)?);
                Ok(a.0 + b.0)
            })
            .function("reset", |_, call, ()| {
                call.object_mut::<Counter>(0)?.0 = 0;
                Ok(())
            })
            .before_update(|probe, _, _| {
                probe.log.push("before_update".into());
                Ok(())
            })
            .after_update(|probe, _, _| {
                probe.log.push("after_update".into());
                match &probe.failure {
                    Some(message) => Err(wingine_native::Error::new(message)),
                    None => Ok(()),
                }
            })
            .before_draw(|probe, _| {
                probe.log.push("before_draw".into());
                Ok(())
            })
            .after_draw(|probe, canvas| {
                probe.log.push("after_draw".into());
                // In canvas pixels, whatever camera the script left.
                canvas.fill_rect(Rect::new(19.0, 19.0, 1.0, 1.0), Color::WHITE);
                Ok(())
            })
            .luau(
                "wait",
                "local wg = require('@wingine')
                local probe = ...
                function probe.wait_marked(name)
                    wg.wait_until(function() return string.find(probe.log(), name, 1, true) ~= nil end)
                end
                local lent = pcall(probe.held, 'jump')
                function probe.lent_while_loading() return lent end",
            );
        registry
            .class::<Counter>()
            .method("add", |_, counter, _, n: i64| {
                counter.0 += n;
                Ok(())
            })
            .method("value", |_, counter, _, ()| Ok(counter.0))
            .method("absorb", |_, counter, call, ()| {
                counter.0 += call.object::<Counter>(0)?.0;
                Ok(())
            });
    }
}

/// A game in a folder named after `test` that loads `Probe`, linked as a library named `test`,
/// and runs `main`.
fn probe_game(test: &str, main: &str) -> Simulation<LuauGame> {
    runtime::link_library(test, wingine_native::library::<Probe>());
    let folder = game(test, &[("conf.luau", &conf(&[test])), ("main.luau", main)]);
    simulation(&folder)
}

#[test]
fn functions_convert_their_arguments_and_results() {
    let mut simulation = probe_game(
        "conversions",
        "sum = probe.add(2, 3.5)
        described = probe.describe(1, 'hi', { r = 0, g = 0.5, b = 0 }, { x = 0, y = 0, width = 3, height = 4 })
        word, number = probe.pair()
        largest = probe.byte(255)
        local _, err = pcall(probe.byte, 256)
        too_big = tostring(err)
        _, err = pcall(probe.byte, 1.5)
        fraction = tostring(err)
        _, err = pcall(probe.add, 1, {})
        not_a_number = tostring(err)
        _, err = pcall(probe.describe, true, 'hi', 'red')
        not_a_color = tostring(err)",
    );
    assert_eq!(simulation.eval::<f64>("sum"), 5.5);
    assert_eq!(simulation.eval::<String>("described"), "true hi 0.5 3 None");
    assert_eq!(
        simulation.eval::<(String, i64)>("word, number"),
        ("one".into(), 2)
    );
    assert_eq!(simulation.eval::<u8>("largest"), 255);
    for (name, message) in [
        ("too_big", "bad argument #1 to `probe.byte`"),
        ("too_big", "256 is out of range: expected 0 to 255"),
        ("fraction", "bad argument #1 to `probe.byte`"),
        ("not_a_number", "bad argument #2 to `probe.add`"),
        ("not_a_color", "bad argument #3 to `probe.describe`"),
    ] {
        let err: String = simulation.eval(name);
        assert!(err.contains(message), "{name}: {err}");
    }
    assert_eq!(
        simulation.eval::<(f32, f32)>("probe.canvas_size()"),
        (20.0, 20.0),
        "`new` had the context"
    );
}

#[test]
fn buffers_cross_both_ways_and_are_written_in_place() {
    let mut simulation = probe_game(
        "buffers",
        "local bytes = buffer.create(4)
        buffer.writeu32(bytes, 0, 0x04030201)
        reversed = buffer.readu32(probe.reversed(bytes), 0)
        kept = buffer.readu32(bytes, 0)
        empty = buffer.len(probe.reversed(buffer.create(0)))
        probe.fill_bytes(bytes, 1, 9, 2)
        filled = buffer.readu32(bytes, 0)
        sum = probe.byte_sum(bytes)
        probe.fill_bytes(bytes, 4, 9, 0)
        local _, err = pcall(probe.fill_bytes, bytes, 3, 9, 2)
        past_the_end = tostring(err)
        unchanged = buffer.readu32(bytes, 0)
        _, err = pcall(probe.reversed, 'text')
        not_a_buffer = tostring(err)",
    );
    assert_eq!(simulation.eval::<u32>("reversed"), 0x0102_0304);
    assert_eq!(
        simulation.eval::<u32>("kept"),
        0x0403_0201,
        "an argument is a copy"
    );
    assert_eq!(simulation.eval::<u32>("empty"), 0);
    assert_eq!(simulation.eval::<u32>("filled"), 0x0409_0901);
    assert_eq!(simulation.eval::<u32>("sum"), 1 + 9 + 9 + 4);
    assert_eq!(simulation.eval::<u32>("unchanged"), 0x0409_0901);
    for (name, message) in [
        ("past_the_end", "bad argument #1 to `probe.fill_bytes`"),
        (
            "past_the_end",
            "2 bytes from byte 3 reach past the end of a buffer of 4",
        ),
        ("not_a_buffer", "bad argument #1 to `probe.reversed`"),
    ] {
        let err: String = simulation.eval(name);
        assert!(err.contains(message), "{name}: {err}");
    }
}

#[test]
fn arrays_cross_both_ways() {
    let mut simulation = probe_game(
        "arrays",
        "doubled = table.concat(probe.doubled({ 1, 2.5, 3 }), ' ')
        none = #probe.doubled({})
        words = table.concat(probe.words('a bc d'), ',')
        local counts, after = probe.lengths({ { 'a' }, {}, { 'b', 'c' } })
        lengths = table.concat(counts, ' ')
        nothing_after = after == nil
        counts, after = probe.lengths({}, 7)
        seven_after = after
        local grid = probe.grid(2, 3)
        cells = `{#grid} {#grid[1]} {grid[2][3]}`
        local _, err = pcall(probe.doubled, { 1, true, 3 })
        not_a_number = tostring(err)
        _, err = pcall(probe.lengths, { { 'a' }, { 'b', {} } }, 7)
        nested = tostring(err)
        _, err = pcall(probe.doubled, 5)
        not_an_array = tostring(err)",
    );
    assert_eq!(simulation.eval::<String>("doubled"), "2 5 6");
    assert_eq!(simulation.eval::<u32>("none"), 0);
    assert_eq!(simulation.eval::<String>("words"), "a,bc,d");
    assert_eq!(simulation.eval::<String>("lengths"), "1 0 2");
    assert!(
        simulation.eval::<bool>("nothing_after"),
        "an array's values don't stand in for an argument left out"
    );
    assert_eq!(simulation.eval::<u32>("seven_after"), 7);
    assert_eq!(simulation.eval::<String>("cells"), "2 3 12");
    for (name, message) in [
        ("not_a_number", "bad argument #1 to `probe.doubled`"),
        ("not_a_number", "`probe.doubled`: item 2: error converting"),
        ("nested", "bad argument #1 to `probe.lengths`"),
        (
            "nested",
            "`probe.lengths`: item 2: item 2: error converting",
        ),
        ("not_an_array", "bad argument #1 to `probe.doubled`"),
    ] {
        let err: String = simulation.eval(name);
        assert!(err.contains(message), "{name}: {err}");
    }
}

#[test]
fn errors_and_panics_are_luau_errors() {
    let mut simulation = probe_game(
        "errors",
        "local _, err = pcall(probe.fail, 'no luck')
        failed = tostring(err)
        _, err = pcall(probe.panic)
        panicked = tostring(err)",
    );
    assert!(simulation.eval::<String>("failed").contains("no luck"));
    let panicked: String = simulation.eval("panicked");
    assert!(panicked.contains("panicked: on purpose"), "{panicked}");
}

#[test]
fn the_libraries_table_is_read_only() {
    let mut simulation = probe_game(
        "read_only",
        "replaced = pcall(function() probe.add = nil end)",
    );
    assert!(!simulation.eval::<bool>("replaced"));
}

#[test]
fn hooks_run_around_update_and_draw() {
    let mut simulation = probe_game(
        "hooks",
        "local wg = require('@wingine')
        function update() probe.mark('update') end
        function draw()
            wg.graphics.clear('#000000')
            -- The hook after draw draws in canvas pixels anyway.
            wg.graphics.set_camera(100, 100)
            probe.mark('draw')
        end",
    );
    simulation.step();
    assert_eq!(
        simulation.eval::<String>("probe.log()"),
        "before_update update after_update before_draw draw after_draw"
    );
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(19, 19), [255, 255, 255, 255]);
}

#[test]
fn a_failing_hook_stops_the_game() {
    let mut simulation = probe_game("failing_hook", "probe.fail_after_update('jammed')");
    simulation.step();
    simulation.step();
    let err = simulation.game().error().expect("the game stopped");
    assert!(
        err.to_string()
            .contains("native library `failing_hook`: jammed"),
        "{err}"
    );
    assert_eq!(
        simulation.eval::<String>("probe.log()"),
        "before_update after_update",
        "nothing ran after the hook failed"
    );
}

#[test]
fn functions_get_the_context_or_canvas_as_scripts_do() {
    let mut simulation = probe_game(
        "lent",
        "local wg = require('@wingine')
        wg.input.bind('jump', { 'Space' })
        jumped = false
        function update()
            jumped = jumped or probe.held('jump')
            _, fill_in_update = pcall(probe.fill, '#ff0000')
        end
        function draw()
            wg.graphics.clear('#000000')
            probe.fill('#0000ff')
            _, held_in_draw = pcall(probe.held, 'jump')
        end",
    );
    assert!(
        simulation.eval::<bool>("probe.lent_while_loading()"),
        "the library's Luau had the context"
    );
    simulation.tap(Key::Space);
    assert!(simulation.eval::<bool>("jumped"));
    let err: String = simulation.eval("tostring(fill_in_update)");
    assert!(err.contains("drawing only works in `draw`"), "{err}");
    let err: String = simulation.eval("tostring(held_in_draw)");
    assert!(err.contains("not in `draw`"), "{err}");
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(0, 0), [0, 0, 255, 255]);
}

#[test]
fn libraries_set_blend_modes_until_the_game_has_drawn() {
    let mut simulation = probe_game(
        "blend",
        "local wg = require('@wingine')
            function draw()
            wg.graphics.clear('#ff0000')
            probe.blend(1)
            probe.fill('#0000ff')
            wg.graphics.fill_rect(19, 18, 1, 1, '#00ff00')
            _, unknown = pcall(probe.blend, 9)
            -- Left multiplying, which the hooks after the game's draw don't: multiplying by
            -- their white would leave red as it is.
            probe.blend(2)
        end",
    );
    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(0, 0), [255, 0, 255, 255], "blue added to red");
    assert_eq!(
        image.pixel(19, 18),
        [255, 255, 0, 255],
        "green added to red"
    );
    assert_eq!(image.pixel(19, 19), [255, 255, 255, 255], "white over red");
    let err: String = simulation.eval("tostring(unknown)");
    assert!(err.contains("there's no blend mode 9"), "{err}");
}

#[test]
fn one_call_draws_many_sprites() {
    let mut simulation = probe_game(
        "batch",
        "local wg = require('@wingine')
            function draw()
            wg.graphics.clear('#000000')
            probe.stamp()
        end",
    );
    simulation.step();
    assert_eq!(simulation.context().frame_stats().quads, 3);
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(1, 1), [255, 0, 0, 255]);
    assert_eq!(image.pixel(13, 1), [255, 255, 255, 255]);
    assert_eq!(image.pixel(14, 8), [255, 255, 255, 255]);
    assert_eq!(image.pixel(10, 4), [0, 0, 0, 255]);
    assert_eq!(image.pixel(15, 5), [0, 0, 0, 255]);
}

#[test]
fn libraries_make_meshes_and_draw_in_3d_views() {
    let mut simulation = probe_game(
        "three_d",
        "local wg = require('@wingine')
        square = probe.square('#ffffff')
        middle = probe.midpoint(vector.zero, vector.create(2, 4, 6))
        _, bad_mesh = pcall(probe.bad_mesh)
        _, not_drawing = pcall(probe.scene, square)
        mode = 'library'
        function draw()
            wg.graphics.clear('#000000')
            if mode == 'library' then
                probe.view()
                _, bad_view = pcall(probe.bad_view)
                -- The bad view began nothing, so the good one is still open.
                probe.scene(square)
            elseif mode == 'script' then
                wg.graphics.begin_3d({
                    camera = { position = vector.create(0, 0, 5), target = vector.zero, height = 2 },
                    lighting = { color = '#000000', ambient = '#ffffff' },
                })
                probe.scene(square)
            else
                probe.scene(square)
            end
        end",
    );
    assert_eq!(simulation.eval::<u32>("square:triangle_count()"), 2);
    assert_eq!(
        simulation.eval::<(f32, f32, f32)>("middle.x, middle.y, middle.z"),
        (1.0, 2.0, 3.0)
    );
    let bad_mesh: String = simulation.eval("tostring(bad_mesh)");
    assert!(bad_mesh.contains("past the last"), "{bad_mesh}");
    let not_drawing: String = simulation.eval("tostring(not_drawing)");
    assert!(
        not_drawing.contains("only works in `draw`"),
        "{not_drawing}"
    );

    let picture = |simulation: &mut Simulation<LuauGame>| {
        simulation.step();
        assert_eq!(simulation.game().error().map(ToString::to_string), None);
        let stats = simulation.context().frame_stats();
        // Two squares, of one mesh, in a draw call, and a billboard, then the pixel the hook
        // after `draw` fills.
        assert_eq!((stats.triangles, stats.draw_calls), (6, 3));
        let image = simulation.screenshot().unwrap();
        assert_eq!(image.pixel(5, 10), [255, 0, 0, 255], "the left square");
        assert_eq!(image.pixel(15, 10), [0, 255, 0, 255], "the right square");
        assert_eq!(image.pixel(10, 18), [0, 0, 255, 255], "the billboard");
        assert_eq!(image.pixel(10, 2), [0, 0, 0, 255], "above them");
    };
    picture(&mut simulation);
    let bad_view: String = simulation.eval("tostring(bad_view)");
    assert!(bad_view.contains("near"), "{bad_view}");

    // In a view the script began.
    simulation.exec("mode = 'script'");
    picture(&mut simulation);

    // With no view open.
    simulation.exec("mode = 'none'");
    simulation.step();
    let err = simulation
        .game()
        .error()
        .expect("the game stopped")
        .to_string();
    assert!(err.contains("begin one first"), "{err}");
}

#[test]
fn a_librarys_draws_glow_a_batch_at_a_time() {
    let mut simulation = probe_game(
        "glow",
        "local wg = require('@wingine')
        square = probe.square('#ffffff')
        glow = '#000000'
        function draw()
            wg.graphics.clear('#000000')
            -- A view with no light in it, where only what glows shows.
            wg.graphics.begin_3d({
                camera = { position = vector.create(0, 0, 5), target = vector.zero, height = 2 },
                lighting = { color = '#000000', ambient = '#000000' },
            })
            -- A script's glow before the library draws is none of the library's.
            wg.graphics.draw_mesh(square, vector.create(0, 0, -1), { scale = 0.1, glow = '#ffffff' })
            probe.glowing_scene(square, glow)
        end",
    );
    simulation.step();
    assert_eq!(simulation.game().error().map(ToString::to_string), None);
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(5, 10), [0, 0, 0, 255], "unlit, and not glowing");
    assert_eq!(image.pixel(10, 18), [0, 0, 0, 255]);

    simulation.exec("glow = '#ffffff'");
    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(5, 10), [255, 0, 0, 255], "the mesh glows");
    assert_eq!(image.pixel(10, 18), [0, 0, 255, 255], "and the billboard");
    assert_eq!(
        image.pixel(15, 10),
        [0, 0, 0, 255],
        "what's drawn after them, without, doesn't"
    );

    // With no view open, as the calls without glow fail.
    simulation.exec("function draw() probe.glowing_scene(square, glow) end");
    simulation.step();
    let err = simulation
        .game()
        .error()
        .expect("the game stopped")
        .to_string();
    assert!(
        err.contains("draw_meshes_glowing draws in a 3D view"),
        "{err}"
    );
}

#[test]
fn objects_have_methods_and_borrow_safely() {
    let mut simulation = probe_game(
        "objects",
        "local counter = probe.new_counter(2)
        counter:add(3)
        value = counter:value()
        name = tostring(counter)
        local other = probe.new_counter(10)
        total = probe.total(counter, other)
        counter:absorb(other)
        absorbed = counter:value()
        probe.reset(other)
        reset = other:value()

        function fails(f)
            local ok, err = pcall(f)
            return if ok then nil else tostring(err)
        end
        with_dot = fails(function() counter.add(3) end)
        not_a_counter = fails(function() probe.total(counter, 1) end)
        -- A method has its object borrowed exclusively, so it can't read it too.
        itself = fails(function() counter:absorb(counter) end)
        missing = counter.subtract",
    );
    assert_eq!(simulation.eval::<i64>("value"), 5);
    assert_eq!(simulation.eval::<String>("name"), "Counter");
    assert_eq!(simulation.eval::<i64>("total"), 15);
    assert_eq!(simulation.eval::<i64>("absorbed"), 15);
    assert_eq!(simulation.eval::<i64>("reset"), 0);
    for (name, message) in [
        ("with_dot", "call methods with `:`"),
        ("not_a_counter", "bad argument #2 to `probe.total`"),
        ("not_a_counter", "error converting Lua number to Counter"),
        ("itself", "bad argument #2 to `Counter:absorb`"),
    ] {
        let err: String = simulation.eval(name);
        assert!(err.contains(message), "{name}: {err}");
    }
    assert!(simulation.eval::<bool>("missing == nil"));
}

#[test]
fn objects_work_in_games_that_loaded_earlier_registrations() {
    let mut first = probe_game("registered_first", "kept = probe.new_counter(1)");
    // Registers `Probe` again, under another name.
    let mut second = probe_game("registered_second", "kept = probe.new_counter(2)");
    for (simulation, kept) in [(&mut first, 1), (&mut second, 2)] {
        assert_eq!(
            simulation.eval::<i64>("probe.total(kept, probe.new_counter(10))"),
            kept + 10
        );
        assert_eq!(simulation.eval::<i64>("probe.new_counter(3):value()"), 3);
    }
}

#[test]
fn luau_from_the_library_can_wait() {
    let mut simulation = probe_game(
        "luau",
        "local wg = require('@wingine')
        done = false
        wg.spawn(function()
            probe.wait_marked('go')
            done = true
        end)",
    );
    simulation.step();
    assert!(!simulation.eval::<bool>("done"));
    simulation.exec("probe.mark('go')");
    simulation.step();
    assert!(simulation.eval::<bool>("done"));
}

/// How many games have created and destroyed `Lifecycle`'s state, and freed its objects.
static CREATED: AtomicUsize = AtomicUsize::new(0);
static DESTROYED: AtomicUsize = AtomicUsize::new(0);
static FREED: AtomicUsize = AtomicUsize::new(0);

struct Lifecycle;

impl Drop for Lifecycle {
    fn drop(&mut self) {
        DESTROYED.fetch_add(1, Ordering::SeqCst);
    }
}

struct Token;

impl Class for Token {
    const NAME: &str = "Token";
}

impl Drop for Token {
    fn drop(&mut self) {
        FREED.fetch_add(1, Ordering::SeqCst);
    }
}

impl Library for Lifecycle {
    const NAME: &str = "lifecycle";

    fn new(_: &mut Context) -> Result<Self> {
        CREATED.fetch_add(1, Ordering::SeqCst);
        Ok(Self)
    }

    fn register(registry: &mut Registry<Self>) {
        registry.function("token", |_, _, ()| Ok(Token));
        registry.class::<Token>();
    }
}

#[test]
fn each_game_has_its_own_state_until_it_ends() {
    runtime::link_library("lifecycle", wingine_native::library::<Lifecycle>());
    let folder = game(
        "lifecycle",
        &[
            ("conf.luau", &conf(&["lifecycle"])),
            ("main.luau", "kept = lifecycle.token() lifecycle.token()"),
        ],
    );
    let first = simulation(&folder);
    let second = simulation(&folder);
    assert_eq!(CREATED.load(Ordering::SeqCst), 2);

    first.game().scripts().lua().gc_collect().unwrap();
    assert_eq!(FREED.load(Ordering::SeqCst), 1, "the token not kept");
    drop(first);
    assert_eq!(DESTROYED.load(Ordering::SeqCst), 1);
    assert_eq!(FREED.load(Ordering::SeqCst), 2);
    drop(second);
    assert_eq!(DESTROYED.load(Ordering::SeqCst), 2);
    assert_eq!(FREED.load(Ordering::SeqCst), 4);
}

thread_local! {
    /// What `Tracer`s did on this thread, in order.
    static TRACE: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// A library that traces its state's life and its hooks to `TRACE`, under its table's name:
/// `north` or `south`.
struct Tracer<const N: usize>;

impl<const N: usize> Tracer<N> {
    /// Traces `event`, which always succeeds, so hooks can return it.
    fn trace(event: &str) -> Result<()> {
        let event = format!("{}:{event}", Self::NAME);
        TRACE.with_borrow_mut(|trace| trace.push(event));
        Ok(())
    }
}

impl<const N: usize> Drop for Tracer<N> {
    fn drop(&mut self) {
        let _ = Self::trace("drop");
    }
}

impl<const N: usize> Library for Tracer<N> {
    const NAME: &str = ["north", "south"][N];

    fn new(_: &mut Context) -> Result<Self> {
        Self::trace("new")?;
        Ok(Self)
    }

    fn register(registry: &mut Registry<Self>) {
        registry
            .before_update(|_, _, _| Self::trace("before_update"))
            .after_update(|_, _, _| Self::trace("after_update"))
            .before_draw(|_, _| Self::trace("before_draw"))
            .after_draw(|_, _| Self::trace("after_draw"));
    }
}

#[test]
fn libraries_run_in_the_order_games_list_them() {
    runtime::link_library("north", wingine_native::library::<Tracer<0>>());
    runtime::link_library("south", wingine_native::library::<Tracer<1>>());
    let folder = game(
        "order",
        &[("conf.luau", &conf(&["south", "north"])), ("main.luau", "")],
    );
    TRACE.take();
    let mut simulation = simulation(&folder);
    simulation.step();
    drop(simulation);
    assert_eq!(
        TRACE.take().join(" "),
        "south:new north:new \
         south:before_update north:before_update south:after_update north:after_update \
         south:before_draw north:before_draw south:after_draw north:after_draw \
         north:drop south:drop"
    );
}

#[test]
fn reloading_a_game_gives_its_libraries_new_state() {
    runtime::link_library("north", wingine_native::library::<Tracer<0>>());
    let folder = game("reload", &[("conf.luau", &conf(&[])), ("main.luau", "")]);
    TRACE.take();
    let mut simulation = simulation(&folder);

    // A library listed by the time the game reloads loads.
    fs::write(folder.join("conf.luau"), conf(&["north"])).unwrap();
    simulation.reload();
    assert!(simulation.game().error().is_none());
    assert!(simulation.eval::<bool>("north ~= nil"));
    assert_eq!(TRACE.take().join(" "), "north:new");

    // The new game's state is made before the old game's goes.
    simulation.reload();
    assert_eq!(TRACE.take().join(" "), "north:new north:drop");
    drop(simulation);
    assert_eq!(TRACE.take().join(" "), "north:drop");
}

/// A library that fails as a game loads it.
struct Failing;

impl Library for Failing {
    const NAME: &str = "failing";

    fn new(_: &mut Context) -> Result<Self> {
        Err(wingine_native::Error::new("out of fuel"))
    }

    fn register(_: &mut Registry<Self>) {}
}

/// A library that registers a function twice.
struct Twice;

impl Library for Twice {
    const NAME: &str = "twice";

    fn new(_: &mut Context) -> Result<Self> {
        Ok(Self)
    }

    fn register(registry: &mut Registry<Self>) {
        registry
            .function("go", |_, _: &mut Call, ()| Ok(()))
            .function("go", |_, _, ()| Ok(()));
    }
}

/// A library that takes its time: 2 milliseconds for its hook after each update, and for each
/// call.
struct Slow;

const NAP: std::time::Duration = std::time::Duration::from_millis(2);

impl Library for Slow {
    const NAME: &str = "slow";

    fn new(_: &mut Context) -> Result<Self> {
        Ok(Self)
    }

    fn register(registry: &mut Registry<Self>) {
        registry
            .function("nap", |_, _, ()| {
                std::thread::sleep(NAP);
                Ok(())
            })
            .after_update(|_, _, _| {
                std::thread::sleep(NAP);
                Ok(())
            });
    }
}

#[test]
fn frame_stats_say_what_each_library_cost_the_frame_before() {
    runtime::link_library("slow", wingine_native::library::<Slow>());
    runtime::link_library("slow_probe", wingine_native::library::<Probe>());
    let folder = game(
        "library_stats",
        &[
            ("conf.luau", &conf(&["slow", "slow_probe"])),
            (
                "main.luau",
                "local wg = require('@wingine')
                -- While the game loads, which is no frame.
                slow.nap()
                seen = {}
                function update()
                    table.insert(seen, wg.graphics.frame_stats())
                    slow.nap()
                    probe.add(1, 2)
                    -- From the frame after this one.
                    if #seen == 2 then
                        wg.graphics.time_library_calls(true)
                    elseif #seen == 4 then
                        wg.graphics.time_library_calls(false)
                    end
                end
                function draw()
                    slow.nap()
                    slow.nap()
                end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    simulation.run_frames(6);
    let nap = NAP.as_secs_f32();

    // Before the first frame has finished, nothing, though the game called a library as it
    // loaded.
    assert_eq!(
        simulation.eval::<(u32, u32, f32)>(
            "seen[1].libraries.slow.update.calls, seen[1].libraries.slow.draw.calls, \
             seen[1].libraries.slow.update.hooks"
        ),
        (0, 0, 0.0)
    );
    // Each library's calls are counted by when they were made, and its hooks timed, within
    // what the frame's updates and drawing took.
    for frame in 2..=6 {
        let slow = format!("seen[{frame}].libraries.slow");
        assert_eq!(
            simulation.eval::<(u32, u32)>(&format!("{slow}.update.calls, {slow}.draw.calls")),
            (1, 2)
        );
        let (hooks, update): (f32, f32) =
            simulation.eval(&format!("{slow}.update.hooks, seen[{frame}].update"));
        assert!(nap <= hooks && hooks <= update, "{nap} {hooks} {update}");
        assert_eq!(simulation.eval::<f32>(&format!("{slow}.draw.hooks")), 0.0);
        assert_eq!(
            simulation.eval::<u32>(&format!("seen[{frame}].libraries.probe.update.calls")),
            1,
            "a library is named by its table"
        );
    }
    // Calls are timed only once a game asks, and for the frames they were timed all through.
    for frame in [2, 3, 5, 6] {
        let timed: bool = simulation.eval(&format!(
            "seen[{frame}].libraries.slow.update.call_time ~= nil \
             or seen[{frame}].libraries.slow.draw.call_time ~= nil"
        ));
        assert!(!timed, "frame {frame}");
    }
    let (update_calls, draw_calls, update, draw): (f32, f32, f32, f32) = simulation.eval(
        "seen[4].libraries.slow.update.call_time, seen[4].libraries.slow.draw.call_time, \
         seen[4].update, seen[4].draw",
    );
    assert!(
        nap <= update_calls && update_calls <= update,
        "{update_calls} {update}"
    );
    assert!(
        2.0 * nap <= draw_calls && draw_calls <= draw,
        "{draw_calls} {draw}"
    );

    // Asked before a frame's first call, as between frames, that frame's calls are all timed.
    fs::write(
        folder.join("main.luau"),
        "local wg = require('@wingine')
        times = {}
        function update()
            wg.graphics.time_library_calls(true)
            slow.nap()
            -- False for a frame whose calls weren't timed.
            table.insert(times, wg.graphics.frame_stats().libraries.slow.update.call_time or false)
        end",
    )
    .unwrap();
    let mut simulation = self::simulation(&folder);
    simulation.run_frames(2);
    assert!(
        simulation.eval::<bool>("times[1] == false"),
        "no frame came before the first"
    );
    assert!(simulation.eval::<f32>("times[2]") >= nap);
}

/// A library that says what it registers, for its definitions.
struct Told;

/// A count scripts hold.
struct Tally(u32);

impl Class for Tally {
    const NAME: &str = "Tally";
}

/// Arguments read by hand, whose types say nothing of what scripts pass.
struct Anything;

impl wingine_native::FromArgs for Anything {
    fn from_args(_: &Call) -> Result<Self> {
        Ok(Self)
    }
}

impl Library for Told {
    const NAME: &str = "told";

    fn new(_: &mut Context) -> Result<Self> {
        Ok(Self)
    }

    fn register(registry: &mut Registry<Self>) {
        registry
            .declare_type(
                "Point",
                "{ x: number, y: number }",
                "A place on the canvas.",
            )
            .function("add", |_, _, (a, b): (f64, Option<f64>)| {
                Ok(a + b.unwrap_or(1.0))
            })
            .named(&["a", "b"])
            .doc(
                "Adds `b` to `a`, or 1 without it. What it does takes more words to say than a \
                 line of a definitions file holds, so they go on to the next.\nAnd a line \
                 written apart stays apart.",
            )
            .function("plain", |_, _, (_, _): (String, Vec<u8>)| {
                Ok(("one", 2, None::<Buffer>))
            })
            .function("fill", |_, call, ()| call.write_buffer(0, 0, &[1]))
            .takes("into: buffer")
            .function("tally", |_, _, start: u32| Ok(Tally(start)))
            .named(&["start"])
            .function("raw", |_, _, _: Anything| Ok(()))
            .function("counted", |_, _, _: Anything| Ok(()))
            .named(&["first", "second"])
            .returns("number?")
            .luau(
                "middle",
                "local told = ...
                function told.middle(a, b)
                    return { x = (a.x + b.x) / 2, y = (a.y + b.y) / 2 }
                end",
            )
            .declare(
                "middle",
                "a: Point, b: Point",
                "Point",
                "The point halfway between two.",
            );
        registry
            .class::<Tally>()
            .method("add", |_, tally, _, n: u32| {
                tally.0 += n;
                Ok(())
            })
            .named(&["n"])
            .doc("Adds `n` to the tally.")
            .method("total", |_, tally, _, ()| Ok(tally.0))
            .method("raw", |_, _, _, _: Anything| Ok(()));
    }
}

const TOLD: &str = "\
--- The `told` native library, for luau-lsp: what the library says of itself as it registers.
--- `wingine definitions` wrote this file: change the library, then have it write the file again.

--- A place on the canvas.
export type Point = { x: number, y: number }

declare extern type Tally with
    --- Adds `n` to the tally.
    function add(self, n: number): ()
    function total(self): number
    function raw(self, ...: any): ()
end

declare told: {
    --- Adds `b` to `a`, or 1 without it. What it does takes more words to say than a line of a
    --- definitions file holds, so they go on to the next.
    --- And a line written apart stays apart.
    add: (a: number, b: number?) -> number,
    plain: (arg1: string, arg2: { number }) -> (string, number, buffer?),
    fill: (into: buffer) -> (),
    tally: (start: number) -> Tally,
    raw: (...any) -> (),
    counted: (first: any, second: any) -> number?,
    --- The point halfway between two.
    middle: (a: Point, b: Point) -> Point,
}
";

#[test]
fn a_librarys_definitions_say_what_it_registers() {
    runtime::link_library("told", wingine_native::library::<Told>());
    let folder = game(
        "definitions",
        &[(
            "conf.luau",
            "return { libraries = { 'told', { name = 'not_there', optional = true } } }",
        )],
    );
    // Without a window: the library registers, and no game loads.
    let definitions = runtime::definitions(&folder).expect("the library registers");
    assert_eq!(
        definitions.len(),
        1,
        "an optional library that isn't there is left out"
    );
    assert_eq!(definitions[0].library, "told");
    assert_eq!(definitions[0].text, TOLD);

    // What its Luau adds is there as it's declared.
    fs::write(folder.join("conf.luau"), conf(&["told"])).unwrap();
    fs::write(
        folder.join("main.luau"),
        "local middle = told.middle({ x = 0, y = 2 }, { x = 4, y = 4 })
        halfway = `{middle.x} {middle.y}`",
    )
    .unwrap();
    let mut simulation = simulation(&folder);
    assert_eq!(simulation.eval::<String>("halfway"), "2 3");

    fs::write(folder.join("conf.luau"), conf(&["nowhere"])).unwrap();
    let err = runtime::definitions(&folder).unwrap_err().to_string();
    assert!(
        err.contains("native library `nowhere`: couldn't find"),
        "{err}"
    );
}

/// A library that names one of a function's two parameters.
struct Misnamed;

impl Library for Misnamed {
    const NAME: &str = "misnamed";

    fn new(_: &mut Context) -> Result<Self> {
        Ok(Self)
    }

    fn register(registry: &mut Registry<Self>) {
        registry
            .function("add", |_, _, (a, b): (f64, f64)| Ok(a + b))
            .named(&["a"]);
    }
}

/// A library that describes a function before registering one.
struct Undescribed;

impl Library for Undescribed {
    const NAME: &str = "undescribed";

    fn new(_: &mut Context) -> Result<Self> {
        Ok(Self)
    }

    fn register(registry: &mut Registry<Self>) {
        registry
            .doc("Does nothing.")
            .function("go", |_, _, ()| Ok(()));
    }
}

#[test]
fn a_library_that_describes_itself_wrongly_fails_to_load() {
    runtime::link_library("misnamed", wingine_native::library::<Misnamed>());
    let err = load_error(&["misnamed"]);
    assert!(
        err.contains("`add` takes 2 arguments, and 1 are named"),
        "{err}"
    );
    runtime::link_library("undescribed", wingine_native::library::<Undescribed>());
    let err = load_error(&["undescribed"]);
    assert!(
        err.contains("describe the function or method registered just before them"),
        "{err}"
    );
}

/// A library whose table would replace one of Luau's.
struct Impostor;

impl Library for Impostor {
    const NAME: &str = "print";

    fn new(_: &mut Context) -> Result<Self> {
        Ok(Self)
    }

    fn register(_: &mut Registry<Self>) {}
}

/// The error loading a game that loads `libraries` fails with.
fn load_error(libraries: &[&str]) -> String {
    let folder = game(
        &format!("load_{}", libraries.join("_")),
        &[("conf.luau", &conf(libraries)), ("main.luau", "")],
    );
    let err = start(&folder).err().expect("loading fails");
    assert!(matches!(err, Error::NativeLibrary { .. }), "{err}");
    err.to_string()
}

#[test]
fn libraries_that_fail_to_load_are_errors() {
    let err = load_error(&["nowhere"]);
    assert!(
        err.contains("native library `nowhere`: couldn't find"),
        "{err}"
    );

    runtime::link_library("failing", wingine_native::library::<Failing>());
    assert!(load_error(&["failing"]).contains("out of fuel"));

    runtime::link_library("twice", wingine_native::library::<Twice>());
    let err = load_error(&["twice"]);
    assert!(err.contains("registered `go` twice"), "{err}");

    runtime::link_library("impostor", wingine_native::library::<Impostor>());
    let err = load_error(&["impostor"]);
    assert!(err.contains("its table `print` is taken"), "{err}");

    // Two libraries with the same table.
    runtime::link_library("north_one", wingine_native::library::<Tracer<0>>());
    runtime::link_library("north_two", wingine_native::library::<Tracer<0>>());
    let err = load_error(&["north_one", "north_two"]);
    assert!(
        err.contains("native library `north_two`: its table `north` is taken"),
        "{err}"
    );

    let newer = abi::Library {
        abi_minor: abi::ABI_MINOR + 1,
        ..wingine_native::library::<Probe>()
    };
    runtime::link_library("newer", newer);
    let err = load_error(&["newer"]);
    assert!(err.contains("can't load"), "{err}");
}

#[test]
fn optional_libraries_load_only_if_theyre_there() {
    runtime::link_library("optional_probe", wingine_native::library::<Probe>());
    let folder = game(
        "optional",
        &[
            (
                "conf.luau",
                "return { libraries = {
                    { name = 'nowhere', optional = true },
                    { name = 'optional_probe', optional = true },
                } }",
            ),
            ("main.luau", "has_probe = probe ~= nil"),
        ],
    );
    assert!(simulation(&folder).eval::<bool>("has_probe"));

    // One that's there but doesn't load is an error all the same.
    let folder = game(
        "optional_broken",
        &[
            (
                "conf.luau",
                "return { libraries = { { name = 'broken', optional = true } } }",
            ),
            ("main.luau", ""),
            (
                &format!("native/{DLL_PREFIX}broken{DLL_SUFFIX}"),
                "not a library",
            ),
        ],
    );
    let err = start(&folder).err().expect("loading fails").to_string();
    assert!(err.contains("native library `broken`"), "{err}");
}

#[test]
fn games_load_the_libraries_of_the_settings_theyre_given() {
    runtime::link_library("given_probe", wingine_native::library::<Probe>());
    let folder = game(
        "given",
        &[
            ("conf.luau", &conf(&["nowhere"])),
            ("main.luau", "has_probe = probe ~= nil"),
        ],
    );
    let mut config = runtime::config(&folder).unwrap();
    config.libraries = vec![LibraryConfig {
        name: "given_probe".to_owned(),
        optional: false,
    }];
    let mut simulation = Simulation::new(&config.window, |ctx| {
        LuauGame::load_with(ctx, &folder, &config)
    })
    .expect("game starts");
    assert!(simulation.eval::<bool>("has_probe"));
}

/// Builds the `native-particles` example, returning the library file. It builds in a target
/// folder of its own, since the one running the tests is busy.
fn build_particles() -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-build");
    let output = Command::new(env!("CARGO"))
        .args(["build", "--package", "native-particles", "--target-dir"])
        .arg(&target)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    target
        .join("debug")
        .join(format!("{DLL_PREFIX}particles{DLL_SUFFIX}"))
}

#[test]
fn libraries_load_from_the_games_native_folder() {
    let folder = game(
        "particles",
        &[
            ("conf.luau", &conf(&["particles"])),
            (
                "main.luau",
                "local wg = require('@wingine')
                sparks = particles.new_emitter('#ffffff')
                sparks:burst(10, 10, 12)
                sparks:burst_3d(vector.zero, 5)
                sparks:burst_each({ 2, 3, 18, 17 }, 3)
                local at = particles.positions()
                places = buffer.len(at) / 8
                last = `{buffer.readf32(at, 17 * 8)} {buffer.readf32(at, 17 * 8 + 4)}`
                gone = false
                wg.spawn(function()
                    particles.wait_until_gone()
                    gone = true
                end)
                function draw()
                    wg.graphics.clear('#000000')
                    particles.draw()
                    wg.graphics.begin_3d({
                        camera = { position = vector.create(0, 1, 5), target = vector.create(0, 1, 0) },
                    })
                    particles.draw_3d()
                end",
            ),
        ],
    );
    let file = build_particles();
    let native = folder.join("native");
    fs::create_dir_all(&native).unwrap();
    fs::copy(&file, native.join(file.file_name().unwrap())).unwrap();

    // What the library says of itself, read from its file.
    let definitions = runtime::definitions(&folder).unwrap();
    assert_eq!(definitions[0].library, "particles");
    for line in [
        "declare extern type Emitter with\n",
        "    --- Bursts `count` particles at a point of the canvas.\n    \
         function burst(self, x: number, y: number, count: number): ()\n",
        "    function burst_3d(self, position: vector, count: number): ()\n",
        "    new_emitter: (color: Color) -> Emitter,\n",
        "    positions: () -> buffer,\n",
        "    wait_until_gone: () -> (),\n",
    ] {
        assert!(
            definitions[0].text.contains(line),
            "{}",
            definitions[0].text
        );
    }

    let mut simulation = simulation(&folder);
    assert_eq!(simulation.eval::<u32>("sparks:bursts()"), 3);
    assert_eq!(simulation.eval::<usize>("particles.count()"), 23);
    // An array in, of two points, and a buffer out, of every particle's place.
    assert_eq!(simulation.eval::<u32>("places"), 18);
    assert_eq!(simulation.eval::<String>("last"), "18 17");
    simulation.step();
    let image = simulation.screenshot().unwrap();
    let lit = image.pixels().chunks(4).filter(|p| p[0] > 0).count();
    assert!(lit > 0, "particles drawn");
    // The sparks, a billboard each, all in one draw call, after the particles' quads.
    let stats = simulation.context().frame_stats();
    assert_eq!((stats.triangles, stats.draw_calls), (10, 1 + 1));

    simulation.run_for(1.0);
    assert_eq!(simulation.eval::<usize>("particles.count()"), 0);
    assert!(simulation.eval::<bool>("gone"));
}
