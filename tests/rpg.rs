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

/// Reads to the end of what's being said: shows each box in full and closes it, until a box
/// asks a question or the event under way is over, waiting out the pauses in between, such as
/// fades and animations. Returns the text of each box closed.
fn read(simulation: &mut Simulation<LuauGame>) -> Vec<String> {
    let mut pages = Vec::new();
    for _ in 0..600 {
        match dialogue(simulation) {
            Some(_) if eval(simulation, "rpg.dialogue.box.choices ~= nil") => break,
            Some(text) => {
                pages.push(text);
                // The first press shows the rest of the text, and the second closes the box.
                simulation.tap(Key::Space);
                simulation.tap(Key::Space);
            }
            None if eval::<i32>(simulation, "rpg.busy") > 0 => simulation.step(),
            None => break,
        }
        if let Some(err) = simulation.game().error() {
            panic!("the game stopped: {err}");
        }
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
    assert_eq!(quest(&simulation), "hunt");
    assert_eq!(
        eval::<u32>(&simulation, "rpg.items.herb"),
        3,
        "the elder's herbs"
    );

    // The creatures are out now.
    climb_to_the_shrine(&mut simulation);
    assert_eq!(foes(&simulation).len(), 3);

    // Beating Gloamfang clears the hill.
    meet(&mut simulation, "Gloamfang");
    read(&mut simulation);
    exec(
        &simulation,
        "rpg.battle.state.foe.hp = 1 rpg.battle.state.foe.shown_hp = 1",
    );
    attack(&mut simulation);
    let pages = read(&mut simulation);
    assert!(
        pages.iter().any(|page| page.contains("they ring")),
        "{pages:?}"
    );
    assert_eq!(quest(&simulation), "cleared");
    assert!(foes(&simulation).is_empty(), "the rest scattered");
    run_for(&mut simulation, 3.0);
    assert!(
        simulation.context().audio().music_playing(),
        "the shrine has music again"
    );

    place_player(&simulation, SHRINE_ARRIVAL, "down");
    simulation.tap(Key::Down);
    run_for(&mut simulation, 1.0);
    let pages = talk_to_elder(&mut simulation);
    assert!(pages[0].contains("The chimes!"), "{pages:?}");
    assert_eq!(quest(&simulation), "done");
}

/// The names of the creatures on the level the player is on.
fn foes(simulation: &Simulation<LuauGame>) -> Vec<String> {
    eval(
        simulation,
        "(function()
            local names = {}
            for _, character in rpg.level.characters do
                if character.foe then
                    table.insert(names, character.name)
                end
            end
            return names
        end)()",
    )
}

/// Starts the hunt and takes the player up to Shrine Hill, where the creatures are, with
/// battles made predictable: every hit lands as hard as it can, and creatures always use their
/// first move.
fn hunt(simulation: &mut Simulation<LuauGame>) {
    exec(
        simulation,
        "rpg.quest = 'hunt'
        rpg.items.herb = 3
        rpg.battle.roll = function() return 0 end",
    );
    climb_to_the_shrine(simulation);
}

/// Stands the player below the creature called `name` and waits for the battle to start, up to
/// the menu.
fn meet(simulation: &mut Simulation<LuauGame>, name: &str) {
    let [column, row] = character_tile(simulation, name);
    place_player(simulation, [column, row + 1], "up");
    for _ in 0..120 {
        simulation.step();
        if eval(simulation, "rpg.battle.state ~= nil") {
            break;
        }
    }
    assert!(
        eval::<bool>(simulation, "rpg.battle.state ~= nil"),
        "a battle starts"
    );
    let pages = read(simulation);
    assert_eq!(pages.len(), 1, "the creature's entrance: {pages:?}");
    assert_eq!(menu(simulation), "What will you do?");
}

/// The question the menu that's open asks.
fn menu(simulation: &Simulation<LuauGame>) -> String {
    assert!(
        eval::<bool>(simulation, "rpg.dialogue.box.choices ~= nil"),
        "a menu is open"
    );
    dialogue(simulation).unwrap()
}

/// Picks Fight, then Strike, from the battle menu.
fn attack(simulation: &mut Simulation<LuauGame>) {
    choose(simulation, 1);
    assert_eq!(menu(simulation), "Which move?");
    choose(simulation, 1);
}

/// The player's and the foe's hit points in the battle under way.
fn battle_hp(simulation: &Simulation<LuauGame>) -> (u32, u32) {
    eval(
        simulation,
        "rpg.battle.state.player.hp, rpg.battle.state.foe.hp",
    )
}

#[test]
fn creatures_only_roam_during_the_hunt() {
    let mut simulation = simulation();
    accept_quest(&mut simulation);
    climb_to_the_shrine(&mut simulation);
    assert!(foes(&simulation).is_empty());

    let mut simulation = self::simulation();
    hunt(&mut simulation);
    let mut names = foes(&simulation);
    names.sort();
    assert_eq!(names, ["Gloamfang", "Murk Wisp", "Shade Bat"]);
}

#[test]
fn blows_are_traded_in_order_of_speed() {
    let mut simulation = simulation();
    hunt(&mut simulation);
    meet(&mut simulation, "Shade Bat");
    assert_eq!(battle_hp(&simulation), (36, 14));
    attack(&mut simulation);
    let pages = read(&mut simulation);
    // The bat is faster, so it bites first: 6 power, 5 attack against 4 defense, halved.
    assert_eq!(pages[0], "The shade bat used Bite!");
    assert_eq!(pages[1], "You used Strike!");
    // 8 power, 6 attack against 3 defense, halved.
    assert_eq!(battle_hp(&simulation), (32, 6));
    assert_eq!(menu(&simulation), "What will you do?");
}

#[test]
fn beating_a_creature_earns_coins_and_drives_it_off() {
    let mut simulation = simulation();
    hunt(&mut simulation);
    meet(&mut simulation, "Shade Bat");
    attack(&mut simulation);
    read(&mut simulation);
    attack(&mut simulation);
    let pages = read(&mut simulation);
    assert!(
        pages.contains(&"You found 6 coins.".to_owned()),
        "{pages:?}"
    );
    assert_eq!(eval::<u32>(&simulation, "rpg.coins"), 6);
    assert_eq!(eval::<u32>(&simulation, "rpg.hp"), 28, "two bites");
    assert!(eval::<bool>(&simulation, "rpg.battle.state == nil"));
    assert!(!foes(&simulation).contains(&"Shade Bat".to_owned()));
    assert_eq!(level(&simulation), "shrine");

    // It's gone for good.
    place_player(&simulation, SHRINE_ARRIVAL, "down");
    simulation.tap(Key::Down);
    run_for(&mut simulation, 1.0);
    climb_to_the_shrine(&mut simulation);
    assert!(!foes(&simulation).contains(&"Shade Bat".to_owned()));
}

#[test]
fn some_creatures_leave_items_behind() {
    let mut simulation = simulation();
    hunt(&mut simulation);
    meet(&mut simulation, "Murk Wisp");
    exec(
        &simulation,
        "rpg.battle.state.foe.hp = 1 rpg.battle.state.foe.shown_hp = 1",
    );
    attack(&mut simulation);
    let pages = read(&mut simulation);
    assert!(
        pages.contains(&"It left behind a Herb.".to_owned()),
        "{pages:?}"
    );
    assert_eq!(eval::<u32>(&simulation, "rpg.items.herb"), 4);
}

#[test]
fn losing_a_battle_wakes_the_player_in_the_village() {
    let mut simulation = simulation();
    hunt(&mut simulation);
    exec(&simulation, "rpg.hp = 1 rpg.items.herb = 0");
    meet(&mut simulation, "Shade Bat");
    attack(&mut simulation);
    let pages = read(&mut simulation);
    assert!(pages.contains(&"You collapse…".to_owned()), "{pages:?}");
    assert!(pages.iter().any(|page| page.contains("carried you home")));
    assert_eq!(level(&simulation), "village");
    assert_eq!(player_tile(&simulation), [ELDER[0], ELDER[1] + 1]);
    assert_eq!(eval::<u32>(&simulation, "rpg.hp"), 36);
    assert_eq!(
        eval::<u32>(&simulation, "rpg.items.herb"),
        1,
        "the elder's spare"
    );
    assert_eq!(quest(&simulation), "hunt");
}

#[test]
fn herbs_heal_and_go_first() {
    let mut simulation = simulation();
    hunt(&mut simulation);
    exec(&simulation, "rpg.hp = 10");
    meet(&mut simulation, "Shade Bat");
    choose(&mut simulation, 2);
    assert_eq!(menu(&simulation), "Use which item?");
    assert_eq!(
        eval::<String>(&simulation, "rpg.dialogue.box.choices[1]"),
        "Herb x3"
    );
    choose(&mut simulation, 1);
    let pages = read(&mut simulation);
    assert_eq!(pages[..2], ["You used a Herb.", "You recovered 15 HP."]);
    assert_eq!(battle_hp(&simulation).0, 25 - 4, "healed, then bitten");
    assert_eq!(eval::<u32>(&simulation, "rpg.items.herb"), 2);
}

#[test]
fn focus_and_guard_change_the_blows() {
    let mut simulation = simulation();
    hunt(&mut simulation);
    meet(&mut simulation, "Gloamfang");
    // Focusing raises attack by half: 8 power, 9 attack against 5 defense, halved.
    choose(&mut simulation, 1);
    choose(&mut simulation, 2);
    let pages = read(&mut simulation);
    assert!(pages.contains(&"Your attack rose!".to_owned()), "{pages:?}");
    // Gloamfang claws: 8 power, 7 attack against 4 defense, halved.
    assert_eq!(battle_hp(&simulation), (29, 36));
    attack(&mut simulation);
    read(&mut simulation);
    assert_eq!(battle_hp(&simulation), (22, 29));

    // Guarding halves the next blow.
    choose(&mut simulation, 1);
    choose(&mut simulation, 3);
    read(&mut simulation);
    assert_eq!(battle_hp(&simulation).0, 22 - 4);
}

#[test]
fn the_player_can_run_from_creatures_but_not_from_gloamfang() {
    let mut simulation = simulation();
    hunt(&mut simulation);
    meet(&mut simulation, "Gloamfang");
    // The cancel key does nothing at the battle menu.
    simulation.tap(Key::Space);
    simulation.tap(Key::Escape);
    assert_eq!(menu(&simulation), "What will you do?");
    // The menu is showing in full already, so the choice is made without revealing it.
    simulation.tap(Key::Up);
    simulation.tap(Key::Enter);
    assert_eq!(
        read(&mut simulation),
        ["There's no running from this fight!"]
    );
    assert_eq!(menu(&simulation), "What will you do?");

    let mut simulation = self::simulation();
    hunt(&mut simulation);
    meet(&mut simulation, "Shade Bat");
    choose(&mut simulation, 3);
    assert_eq!(read(&mut simulation), ["You got away safely!"]);
    assert!(eval::<bool>(&simulation, "rpg.battle.state == nil"));
    // The bat leaves the player be for a while, though they're side by side.
    run_for(&mut simulation, 1.0);
    assert!(eval::<bool>(&simulation, "rpg.battle.state == nil"));
}

#[test]
fn the_well_restores_the_player() {
    let mut simulation = simulation();
    exec(&simulation, "rpg.hp = 5");
    place_player(&simulation, [WELL[0], WELL[1] + 1], "up");
    simulation.tap(Key::Space);
    let pages = read(&mut simulation);
    assert!(pages[0].starts_with("You drink from the well"), "{pages:?}");
    assert_eq!(eval::<u32>(&simulation, "rpg.hp"), 36);
}
