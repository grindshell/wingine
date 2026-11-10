//! What scenes cost: posing a rigged character by its clips, adding copies of it, loading a level
//! that places it many times, and moving particles on in stacks. Run with
//! `cargo bench --bench scenes`.
//!
//! The character is the orchard's picker. Each case reports the median of [`BATCHES`] batches, per
//! operation.

use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use wingine::{
    headless::Headless,
    scene::{Scene, SceneFile},
};

const BATCHES: usize = 15;
/// Poses a batch.
const POSES: usize = 1000;
/// Copies of the picker a batch adds.
const COPIES: usize = 100;
/// Pickers the level places.
const PICKERS: usize = 50;
/// Stacks of particles, each of [`EMITTERS`] emitters, two deep, in the particles case.
const STACKS: usize = 20;
const EMITTERS: usize = 10;

fn main() {
    let orchard = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/orchard");
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("scenes-bench");
    fs::create_dir_all(&folder).unwrap();
    for file in ["orchard.png", "picker.scene.luau"] {
        fs::copy(orchard.join(file), folder.join(file)).unwrap();
    }
    fs::write(folder.join("level.scene.luau"), level()).unwrap();
    fs::write(folder.join("particles.scene.luau"), particles()).unwrap();

    let mut headless = Headless::new().expect("a GPU adapter is available");
    let ctx = headless.context();
    let picker = Scene::load(ctx, &folder, "picker.scene.luau").unwrap();
    let clips: Vec<_> = SceneFile::read(folder.join("picker.scene.luau"))
        .unwrap()
        .clips
        .into_values()
        .collect();
    let tracks: usize = clips.iter().map(|clip| clip.tracks.len()).sum();
    println!(
        "The median of {BATCHES} batches, the picker having {} nodes and {} clips of {tracks} \
         tracks in all:",
        picker.descendants(picker.root()).len(),
        clips.len(),
    );

    let mut posed = picker.clone();
    let root = posed.root();
    report(
        "Scene::pose, the picker, by each of its clips in turn",
        POSES,
        || {
            for i in 0..POSES {
                let clip = &clips[i % clips.len()];
                posed.pose(root, clip, i as f32 * 0.013, 1.0).unwrap();
            }
        },
    );

    report("Scene::add, a copy of the picker", COPIES, || {
        let mut scene = picker.clone();
        let root = scene.root();
        for i in 0..COPIES {
            let name = format!("copy{i}");
            scene
                .add(root, &picker, picker.root(), Some(&name))
                .unwrap();
        }
    });

    report(
        &format!("Scene::load, a level placing {PICKERS} pickers"),
        1,
        || {
            Scene::load(ctx, &folder, "level.scene.luau").unwrap();
        },
    );

    let mut particles = Scene::load(ctx, &folder, "particles.scene.luau").unwrap();
    let root = particles.root();
    report(
        &format!(
            "Scene::advance_particles, {} emitters in fitting stacks",
            STACKS * EMITTERS
        ),
        1,
        || particles.advance_particles(root, 1.0 / 60.0),
    );

    if cfg!(debug_assertions) {
        println!("(a debug build: run `cargo bench` for one built for speed)");
    }
}

/// Runs `batch`, which does `count` of what's measured, [`BATCHES`] times, and reports the median
/// time each took.
fn report(case: &str, count: usize, mut batch: impl FnMut()) {
    batch();
    let mut times: Vec<Duration> = (0..BATCHES)
        .map(|_| {
            let start = Instant::now();
            batch();
            start.elapsed()
        })
        .collect();
    times.sort();
    let each = times[BATCHES / 2].as_secs_f64() / count as f64 * 1e6;
    println!("  {case:<62}{each:>9.2} us each");
}

/// A level placing [`PICKERS`] pickers in a row.
fn level() -> String {
    let mut text = String::from("return {\n    format = 4,\n    children = {\n");
    for i in 0..PICKERS {
        writeln!(
            text,
            "        {{ name = \"picker{i}\", kind = \"instance\", x = {}, y = 100, \
             scene = \"picker.scene.luau\" }},",
            i * 20
        )
        .unwrap();
    }
    text.push_str("    },\n}\n");
    text
}

/// [`STACKS`] stacks in a stack, each of [`EMITTERS`] emitters in areas, all fitting what they
/// hold.
fn particles() -> String {
    let mut text = String::from(
        "return {\n    format = 4,\n    children = {\n        { name = \"column\", kind = \
         \"stack\", gap = 2, children = {\n",
    );
    for i in 0..STACKS {
        writeln!(
            text,
            "            {{ name = \"row{i}\", kind = \"stack\", direction = \"row\", gap = 2, \
             children = {{"
        )
        .unwrap();
        for j in 0..EMITTERS {
            writeln!(
                text,
                "                {{ name = \"emitter{j}\", kind = \"particles\", width = 8, \
                 height = 8, rate = 20, lifetime = {{ 0.5, 1 }}, speed = {{ 10, 20 }} }},"
            )
            .unwrap();
        }
        text.push_str("            } },\n");
    }
    text.push_str("        } },\n    },\n}\n");
    text
}
