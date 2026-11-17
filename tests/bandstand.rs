//! Tests for the bandstand example, a rhythm game written in Luau, in `examples/bandstand`, played
//! to a song of the engine's own instruments that it changes as it plays: layers fading in as the
//! player keeps time, the chorus at the next bar after a long streak, quicker each verse, lights
//! flashing with the drums and confetti at the drop. They run it headlessly, where the song is
//! silent but keeps time with the game, and check its state, which `main.luau` keeps in the
//! global table `bandstand`, and run its own tests, in `bandstand.test.luau`.

use std::{fs, path::Path};

use wingine::{
    headless::Simulation,
    input::Key,
    music::SongFile,
    runtime::{self, LuauGame, testing},
};

const GAME: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/bandstand");

fn simulation() -> Simulation<LuauGame> {
    let config = runtime::config(GAME).expect("settings load");
    Simulation::new(&config.window, |ctx| {
        LuauGame::load_with(ctx, GAME, &config)
    })
    .expect("game starts")
}

fn step(simulation: &mut Simulation<LuauGame>) {
    simulation.step();
    if let Some(err) = simulation.game().error() {
        panic!("the game stopped: {err}");
    }
}

#[test]
fn the_song_is_in_the_canonical_layout() {
    let path = Path::new(GAME).join("bandstand.song.luau");
    // As checked out, which may have turned line endings into CRLF.
    let text = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    let song = SongFile::parse(&text, &path).unwrap();
    assert!(
        song.to_luau().unwrap() == text,
        "bandstand.song.luau isn't in the layout songs are written in: write it again with \
         `SongFile::write`, or save it in the editor"
    );
}

/// Its own tests, in `bandstand.test.luau`, as `wingine test examples/bandstand` runs them. With
/// `UPDATE_SCREENSHOTS=1`, their reference is saved again.
#[test]
fn the_luau_tests_pass() {
    let options = testing::Options {
        update_screenshots: std::env::var_os("UPDATE_SCREENSHOTS").is_some(),
        ..Default::default()
    };
    let report = testing::run(GAME, &options, |_| {}).expect("the tests run");
    assert!(report.outcomes.len() >= 8, "{report}");
    assert!(report.passed(), "{report}");
}

/// Presses Space on the next cue, as near its beat as frames allow: the press is heard in the
/// frame after the last one run, a frame further into the song.
fn press_on_the_next_cue(simulation: &mut Simulation<LuauGame>) {
    let cue: f64 = simulation.eval("bandstand.judged + 1");
    loop {
        let (now, tempo): (f64, f64) = simulation.eval("bandstand.now, bandstand.band:tempo()");
        let beats_a_frame = tempo / 60.0 / 60.0;
        if now + beats_a_frame * 1.5 >= cue {
            break;
        }
        step(simulation);
    }
    simulation.tap(Key::Space);
}

/// A player who keeps time brings the band in, plays the chorus, and reaches the drop, hitting
/// every cue perfectly, through the song speeding up.
#[test]
fn keeping_time_brings_the_band_in_and_reaches_the_drop() {
    // From the verse's first beat, 8 beats in, 24 to the chorus, which starts on the next bar,
    // and its first 16 beats to the drop, which is on the 17th.
    const FIRST_CUE: u32 = 8;
    const DROP: u32 = FIRST_CUE + 24 + 16;
    let mut simulation = simulation();
    for hits in 1..=DROP - FIRST_CUE + 1 {
        press_on_the_next_cue(&mut simulation);
        assert_eq!(simulation.eval::<u32>("bandstand.streak"), hits);
        if hits == 24 {
            assert!(simulation.eval::<bool>("bandstand.chorus_due"));
        }
    }
    simulation.run_for(0.1);
    assert_eq!(simulation.eval::<u32>("bandstand.drops"), 1);
    let (section, bass, lead): (String, bool, bool) =
        simulation.eval("bandstand.section, bandstand.layers.bass, bandstand.layers.lead");
    assert_eq!(section, "chorus");
    assert!(bass && lead, "the whole band plays");
    // Every hit perfect, each 8 in a row worth 100 more.
    let hits = DROP - FIRST_CUE + 1;
    let score: u32 = (0..hits).map(|hit| 100 * (1 + hit / 8)).sum();
    assert_eq!(simulation.eval::<u32>("bandstand.score"), score);
    // The verse came around once on the way, so the song is quicker by a step.
    assert_eq!(simulation.eval::<f64>("bandstand.band:tempo()"), 116.0);
    assert_eq!(simulation.context().audio().songs_playing(), 1);
}
