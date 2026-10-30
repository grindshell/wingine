//! Tests for the RPG example, a game written entirely in Luau, in `examples/rpg`. They run it
//! headlessly and check its state, which `main.luau` keeps in the global table `rpg`.

use wingine::{
    headless::Simulation,
    input::Key,
    lua::mlua::FromLuaMulti,
    runtime::{self, LuauGame},
};

const GAME: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/rpg");

// Where things are, as in the maps.
const START: [i32; 2] = [20, 13];
const ELDER: [i32; 2] = [16, 7];
const MAPLE: [i32; 2] = [24, 14];
const GATE: [i32; 2] = [20, 2];
const WELL: [i32; 2] = [20, 10];
const SIGNPOST: [i32; 2] = [21, 3];
/// Where the village's north exit leads, on the shrine's map, and back.
const SHRINE_ARRIVAL: [i32; 2] = [12, 18];
const VILLAGE_ARRIVAL: [i32; 2] = [20, 1];
const SEAL_STONE: [i32; 2] = [12, 4];

fn simulation() -> Simulation<LuauGame> {
    let config = runtime::config(GAME).expect("window settings load");
    Simulation::new(&config, |ctx| LuauGame::load(ctx, GAME)).expect("game starts")
}

/// The value of the Luau `expression`, such as `"rpg.quest"`.
fn eval<T: FromLuaMulti>(simulation: &Simulation<LuauGame>, expression: &str) -> T {
    let lua = simulation.game().scripts().lua();
    lua.load(format!("return {expression}"))
        .eval()
        .unwrap_or_else(|err| panic!("`{expression}`: {err}"))
}

/// Runs the Luau `code`, such as to change the game's state.
fn exec(simulation: &Simulation<LuauGame>, code: &str) {
    let scripts = simulation.game().scripts();
    scripts
        .load_source("test", code)
        .unwrap_or_else(|err| panic!("{err}"));
}

/// Runs `seconds` of game time, failing if the game stops with an error.
fn run_for(simulation: &mut Simulation<LuauGame>, seconds: f32) {
    simulation.run_for(seconds);
    if let Some(err) = simulation.game().error() {
        panic!("the game stopped: {err}");
    }
}

fn player_tile(simulation: &Simulation<LuauGame>) -> [i32; 2] {
    let (column, row) = eval(simulation, "rpg.player.mover:tile()");
    [column, row]
}

/// Stands the player still on `tile`, facing `facing`.
fn place_player(simulation: &Simulation<LuauGame>, [column, row]: [i32; 2], facing: &str) {
    exec(
        simulation,
        &format!("rpg.player.mover:place({column}, {row}) rpg.player.mover:face('{facing}')"),
    );
}

/// The Luau expression for the character called `name` on the level the player is on.
fn character(name: &str) -> String {
    format!(
        "(function()
            for _, character in rpg.level.characters do
                if character.name == '{name}' then
                    return character
                end
            end
            error('nobody called {name} is here')
        end)()"
    )
}

fn character_tile(simulation: &Simulation<LuauGame>, name: &str) -> [i32; 2] {
    let (column, row) = eval(simulation, &format!("{}.mover:tile()", character(name)));
    [column, row]
}

/// The text in the dialogue box, if it's open.
fn dialogue(simulation: &Simulation<LuauGame>) -> Option<String> {
    eval(simulation, "rpg.dialogue.box and rpg.dialogue.box.text")
}

fn quest(simulation: &Simulation<LuauGame>) -> String {
    eval(simulation, "rpg.quest")
}

fn level(simulation: &Simulation<LuauGame>) -> String {
    eval(simulation, "rpg.level.name")
}

/// Reads to the end of what's being said: shows each box in full and closes it, until nothing
/// is open or a box asks a question. Returns the text of each box closed.
fn read(simulation: &mut Simulation<LuauGame>) -> Vec<String> {
    let mut pages = Vec::new();
    while let Some(text) = dialogue(simulation) {
        if eval(simulation, "rpg.dialogue.box.choices ~= nil") {
            break;
        }
        pages.push(text);
        assert!(pages.len() < 20, "the conversation ends");
        // The first press shows the rest of the text, and the second closes the box. The next
        // box, if there is one, opens straight away.
        simulation.tap(Key::Space);
        simulation.tap(Key::Space);
    }
    pages
}

/// Answers the question being asked with choice `number`, counting from 1.
fn choose(simulation: &mut Simulation<LuauGame>, number: usize) {
    assert!(
        eval::<bool>(simulation, "rpg.dialogue.box.choices ~= nil"),
        "a question is being asked"
    );
    simulation.tap(Key::Space);
    for _ in 1..number {
        simulation.tap(Key::Down);
    }
    simulation.tap(Key::Enter);
}

/// Talks to the elder from below, and reads up to the question, if they ask one.
fn talk_to_elder(simulation: &mut Simulation<LuauGame>) -> Vec<String> {
    place_player(simulation, [ELDER[0], ELDER[1] + 1], "up");
    simulation.tap(Key::Space);
    read(simulation)
}

fn accept_quest(simulation: &mut Simulation<LuauGame>) {
    talk_to_elder(simulation);
    choose(simulation, 1);
    read(simulation);
    assert_eq!(quest(simulation), "accepted");
}

/// Walks from the top of the village through its north exit, and waits to arrive.
fn climb_to_the_shrine(simulation: &mut Simulation<LuauGame>) {
    place_player(simulation, VILLAGE_ARRIVAL, "up");
    simulation.tap(Key::Up);
    run_for(simulation, 1.0);
    assert_eq!(level(simulation), "shrine");
}

fn gate_is_shut(simulation: &Simulation<LuauGame>) -> bool {
    let [column, row] = GATE;
    eval(
        simulation,
        &format!("rpg.levels.village.map:is_solid({column}, {row})"),
    )
}

#[test]
fn the_game_starts_in_the_village() {
    let mut simulation = simulation();
    run_for(&mut simulation, 0.5);
    assert_eq!(level(&simulation), "village");
    assert_eq!(player_tile(&simulation), START);
    assert_eq!(quest(&simulation), "start");
    assert_eq!(eval::<String>(&simulation, "rpg.banner.text"), "Mossbrook");
    assert!(simulation.context().audio().music_playing());
}

#[test]
fn a_tap_walks_one_tile() {
    let mut simulation = simulation();
    simulation.tap(Key::Up);
    run_for(&mut simulation, 0.5);
    assert_eq!(player_tile(&simulation), [START[0], START[1] - 1]);
    assert!(!eval::<bool>(&simulation, "rpg.player.mover:is_moving()"));
}

#[test]
fn the_shut_gate_blocks_the_way_north() {
    let mut simulation = simulation();
    place_player(&simulation, [GATE[0], GATE[1] + 4], "up");
    simulation.key_down(Key::W);
    // Three steps to the gate, then a thud for walking into it.
    let thuds = (0..60)
        .map(|_| {
            simulation.step();
            simulation.context().audio().sounds_playing()
        })
        .max();
    simulation.key_up(Key::W);
    assert_eq!(player_tile(&simulation), [GATE[0], GATE[1] + 1]);
    assert_eq!(thuds, Some(1));
    assert!(gate_is_shut(&simulation));
}

#[test]
fn examining_a_cell_describes_whats_there() {
    let mut simulation = simulation();
    place_player(&simulation, [WELL[0], WELL[1] + 1], "up");
    simulation.tap(Key::Space);
    let text = dialogue(&simulation).expect("a description");
    assert!(text.contains("village well"), "{text}");
    assert_eq!(read(&mut simulation).len(), 1);
    assert_eq!(dialogue(&simulation), None);

    // A place's own text takes over from its kind of tile's.
    place_player(&simulation, [SIGNPOST[0], SIGNPOST[1] + 1], "up");
    simulation.tap(Key::Space);
    let text = dialogue(&simulation).expect("the signpost's text");
    assert!(text.contains("North: Shrine Hill"), "{text}");
}

#[test]
fn the_ground_can_be_examined_too() {
    let mut simulation = simulation();
    // The path, and grass to the left of it.
    simulation.tap(Key::Space);
    let path = dialogue(&simulation).expect("a description of the path");
    read(&mut simulation);
    place_player(&simulation, START, "left");
    simulation.tap(Key::Space);
    let grass = dialogue(&simulation).expect("a description of the grass");
    assert_ne!(path, grass);
}

#[test]
fn text_appears_gradually_with_blips() {
    let mut simulation = simulation();
    place_player(&simulation, [WELL[0], WELL[1] + 1], "up");
    simulation.tap(Key::Space);
    let shown = |simulation: &Simulation<LuauGame>| -> (f32, usize) {
        eval(
            simulation,
            "rpg.dialogue.box.shown, rpg.dialogue.box.layout:glyph_count()",
        )
    };
    assert_eq!(shown(&simulation).0, 0.0);
    let blips = (0..20)
        .map(|_| {
            simulation.step();
            simulation.context().audio().sounds_playing()
        })
        .max();
    assert_eq!(blips, Some(1));
    let (so_far, total) = shown(&simulation);
    assert!(so_far > 0.0 && so_far < total as f32, "{so_far} of {total}");

    // The next press shows the rest, and the one after closes it.
    simulation.tap(Key::E);
    assert_eq!(shown(&simulation), (total as f32, total));
    simulation.tap(Key::E);
    assert_eq!(dialogue(&simulation), None);
}

#[test]
fn the_player_stays_put_while_reading() {
    let mut simulation = simulation();
    place_player(&simulation, [WELL[0], WELL[1] + 1], "up");
    simulation.tap(Key::Space);
    for key in [Key::Left, Key::Down, Key::Right] {
        simulation.hold(key, 0.3);
    }
    assert_eq!(player_tile(&simulation), [WELL[0], WELL[1] + 1]);
    assert!(dialogue(&simulation).is_some());
}

#[test]
fn the_elder_gives_the_quest_and_opens_the_gate() {
    let mut simulation = simulation();
    let pages = talk_to_elder(&mut simulation);
    assert!(pages.iter().any(|page| page.contains("Stone Shrine")));
    assert_eq!(
        eval::<String>(&simulation, "rpg.dialogue.box.speaker"),
        "Elder Rowan"
    );
    choose(&mut simulation, 1);
    assert_eq!(quest(&simulation), "accepted");
    assert!(!gate_is_shut(&simulation));
    let pages = read(&mut simulation);
    assert!(pages[0].contains("north gate"), "{pages:?}");
    assert_eq!(eval::<i32>(&simulation, "rpg.busy"), 0, "the talk is over");

    // Asked again, the elder reminds the player where to go.
    let pages = talk_to_elder(&mut simulation);
    assert!(pages[0].contains("through the north gate"), "{pages:?}");
}

#[test]
fn declining_leaves_the_gate_shut_until_asked_again() {
    let mut simulation = simulation();
    talk_to_elder(&mut simulation);
    choose(&mut simulation, 2);
    read(&mut simulation);
    assert_eq!(quest(&simulation), "start");
    assert!(gate_is_shut(&simulation));

    // Backing out with the cancel key picks the last choice too.
    talk_to_elder(&mut simulation);
    simulation.tap(Key::Space);
    simulation.tap(Key::Escape);
    read(&mut simulation);
    assert_eq!(quest(&simulation), "start");

    accept_quest(&mut simulation);
}

#[test]
fn characters_turn_to_talk_and_block_the_way() {
    let mut simulation = simulation();
    let facing = format!("{}.mover:facing()", character("Elder Rowan"));
    place_player(&simulation, [ELDER[0] + 1, ELDER[1]], "left");
    simulation.tap(Key::Space);
    assert_eq!(eval::<String>(&simulation, &facing), "right");
    read(&mut simulation);
    choose(&mut simulation, 2);
    read(&mut simulation);
    assert_eq!(
        eval::<String>(&simulation, &facing),
        "down",
        "turned back afterwards"
    );

    simulation.hold(Key::Left, 0.5);
    assert_eq!(player_tile(&simulation), [ELDER[0] + 1, ELDER[1]]);
}

#[test]
fn maple_walks_a_route_and_stops_to_talk() {
    let mut simulation = simulation();
    // Five steps east, a rest, and back. The first step starts as the map loads.
    let walking = character_tile(&simulation, "Maple");
    assert_eq!(walking, [MAPLE[0] + 1, MAPLE[1]]);
    run_for(&mut simulation, 2.0);
    let resting = character_tile(&simulation, "Maple");
    assert_eq!(resting, [MAPLE[0] + 5, MAPLE[1]]);

    // Talking stops Maple until the talk is over.
    place_player(&simulation, [resting[0], resting[1] - 1], "down");
    simulation.tap(Key::Space);
    assert!(dialogue(&simulation).is_some());
    run_for(&mut simulation, 4.0);
    assert_eq!(character_tile(&simulation, "Maple"), resting);
    read(&mut simulation);
    run_for(&mut simulation, 2.0);
    assert_ne!(character_tile(&simulation, "Maple"), resting);
}

#[test]
fn the_path_north_leads_to_the_shrine_and_back() {
    let mut simulation = simulation();
    accept_quest(&mut simulation);
    climb_to_the_shrine(&mut simulation);
    assert_eq!(player_tile(&simulation), SHRINE_ARRIVAL);
    assert_eq!(eval::<f32>(&simulation, "rpg.fade"), 0.0, "faded back in");
    assert_eq!(
        eval::<String>(&simulation, "rpg.banner.text"),
        "Shrine Hill"
    );
    run_for(&mut simulation, 1.0);
    assert!(
        !simulation.context().audio().music_playing(),
        "the shrine is quiet"
    );

    simulation.tap(Key::Down);
    run_for(&mut simulation, 1.0);
    assert_eq!(level(&simulation), "village");
    assert_eq!(player_tile(&simulation), VILLAGE_ARRIVAL);
    assert!(!gate_is_shut(&simulation), "the village is as it was left");
    assert!(simulation.context().audio().music_playing());
}

#[test]
fn the_player_waits_while_moving_between_maps() {
    let mut simulation = simulation();
    accept_quest(&mut simulation);
    place_player(&simulation, VILLAGE_ARRIVAL, "up");
    simulation.tap(Key::Up);
    simulation.hold(Key::Down, 0.3);
    run_for(&mut simulation, 0.5);
    assert_eq!(level(&simulation), "shrine");
    assert_eq!(player_tile(&simulation), SHRINE_ARRIVAL);
}

#[test]
fn the_quest_so_far() {
    let mut simulation = simulation();
    accept_quest(&mut simulation);
    climb_to_the_shrine(&mut simulation);

    place_player(&simulation, [SEAL_STONE[0], SEAL_STONE[1] + 1], "up");
    simulation.tap(Key::Space);
    let pages = read(&mut simulation);
    assert!(pages[0].contains("split clean in two"), "{pages:?}");
    assert_eq!(quest(&simulation), "seen_shrine");
    // Looking again doesn't tell it all over again.
    simulation.tap(Key::Space);
    assert_eq!(read(&mut simulation).len(), 1);

    place_player(&simulation, SHRINE_ARRIVAL, "down");
    simulation.tap(Key::Down);
    run_for(&mut simulation, 1.0);
    let pages = talk_to_elder(&mut simulation);
    assert!(pages.iter().any(|page| page.contains("seal stone")));
    assert_eq!(quest(&simulation), "reported");
}
