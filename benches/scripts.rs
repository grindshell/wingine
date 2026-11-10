//! What Luau tasks cost: a game of [`TASKS`] tasks, each waiting a long time, for the next frame,
//! or until something happens. Run with `cargo bench --bench scripts`.
//!
//! Each case reports the median time `update` took over [`FRAMES`] updates, which resumes the
//! tasks, once they're all waiting.

use std::{fs, path::PathBuf};

use wingine::{WindowConfig, headless::Simulation, runtime::LuauGame};

const TASKS: usize = 1000;
const WARMUP: u32 = 30;
const FRAMES: u32 = 120;

fn main() {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("scripts-bench");
    println!("Luau, {TASKS} tasks, the median of {FRAMES} updates:");
    for (name, case, body) in [
        ("long", "each waiting a minute", "wait(60)"),
        (
            "frame",
            "each waiting for the next frame",
            "while true do wait() end",
        ),
        (
            "until",
            "each waiting until something happens",
            "wait_until(function() return happened end)",
        ),
    ] {
        let game = folder.join(name);
        fs::create_dir_all(&game).unwrap();
        fs::write(
            game.join("main.luau"),
            format!(
                "happened = false
                for i = 1, {TASKS} do
                    spawn(function() {body} end)
                end"
            ),
        )
        .unwrap();
        let config = WindowConfig {
            width: 320,
            height: 180,
            ..Default::default()
        };
        let mut simulation = Simulation::new(&config, |ctx| LuauGame::load(ctx, &game))
            .expect("a GPU adapter is available");
        simulation.run_frames(WARMUP);
        let mut times: Vec<f32> = (0..FRAMES)
            .map(|_| {
                simulation.step();
                simulation.context().frame_stats().update
            })
            .collect();
        if let Some(err) = simulation.game().error() {
            panic!("{name}: {err}");
        }
        times.sort_by(f32::total_cmp);
        let time = times[times.len() / 2];
        println!(
            "  {case:<44}{:>8.3} ms, {:>6.1} ns a task",
            time * 1000.0,
            time * 1e9 / TASKS as f32
        );
    }
    if cfg!(debug_assertions) {
        println!("(a debug build: run `cargo bench` for one built for speed)");
    }
}
