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
const SHOP_SIGN: [i32; 2] = [27, 10];
/// Where the village's north exit leads, on the shrine's map, and back.
const SHRINE_ARRIVAL: [i32; 2] = [12, 18];
const VILLAGE_ARRIVAL: [i32; 2] = [20, 1];
const SEAL_STONE: [i32; 2] = [12, 4];

fn simulation() -> Simulation<LuauGame> {
    let config = runtime::config(GAME).expect("settings load");
    Simulation::new(&config.window, |ctx| LuauGame::load(ctx, GAME)).expect("game starts")
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

/// The number of the frame of its walk the player shows, or 0 while standing still.
fn walk_frame(simulation: &Simulation<LuauGame>) -> u32 {
    eval(
        simulation,
        "if rpg.player.mover:is_moving() \
         then rpg.player.walk:frame_index(rpg.player.walk_time) else 0",
    )
}

#[test]
fn walking_strides_then_stands_on_each_tile() {
    let mut simulation = simulation();
    // Three open tiles up to the gate.
    place_player(&simulation, [GATE[0], GATE[1] + 4], "up");
    simulation.key_down(Key::W);
    let mut frames: Vec<u32> = (0..20)
        .map(|_| {
            simulation.step();
            walk_frame(&simulation)
        })
        .collect();
    simulation.key_up(Key::W);
    assert_eq!(frames[0], 1, "a step starts mid-stride");
    assert!(
        !frames.contains(&0),
        "walking on doesn't stop between tiles"
    );
    frames.dedup();
    assert!(frames.starts_with(&[1, 2, 1, 2]), "{frames:?}");

    run_for(&mut simulation, 0.5);
    assert_eq!(walk_frame(&simulation), 0);
    assert_eq!(eval::<f32>(&simulation, "rpg.player.walk_time"), 0.0);
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
fn the_shop_sign_says_whether_the_shop_is_open() {
    let mut simulation = simulation();
    place_player(&simulation, [SHOP_SIGN[0] - 1, SHOP_SIGN[1]], "right");
    simulation.tap(Key::Space);
    let text = dialogue(&simulation).expect("the sign's text");
    assert!(text.contains("shut today"), "{text}");
    read(&mut simulation);

    exec(&simulation, "rpg.quest = 'hunt'");
    simulation.tap(Key::Space);
    let text = dialogue(&simulation).expect("the sign's text");
    assert!(text.contains("propped open"), "{text}");
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
        eval::<(String, String)>(
            &simulation,
            "rpg.dialogue.box.choices[1], rpg.dialogue.box.details[1]"
        ),
        ("Herb".to_owned(), "x3".to_owned())
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

    // Held still beside the player, the bat leaves them be until it calms down, then fights
    // again.
    let bat = character("Shade Bat");
    exec(&simulation, &format!("{bat}.talking = true"));
    for _ in 0..60 {
        if !eval::<bool>(&simulation, &format!("{bat}.mover:is_moving()")) {
            break;
        }
        simulation.step();
    }
    let [column, row] = character_tile(&simulation, "Shade Bat");
    place_player(&simulation, [column, row + 1], "up");
    let calm_left: f32 = eval(&simulation, &format!("{bat}.calm_until - rpg.time"));
    assert!(calm_left > 0.5, "still calm: {calm_left}");
    run_for(&mut simulation, calm_left - 0.25);
    assert_eq!(
        eval::<i32>(&simulation, "rpg.busy"),
        0,
        "no fight while calm"
    );
    run_for(&mut simulation, 0.5);
    assert_eq!(
        eval::<i32>(&simulation, "rpg.busy"),
        1,
        "a fight once calm again"
    );
}

#[test]
fn items_are_kept_when_the_player_is_not_hurt() {
    let mut simulation = simulation();
    hunt(&mut simulation);
    meet(&mut simulation, "Shade Bat");
    choose(&mut simulation, 2);
    choose(&mut simulation, 1);
    assert_eq!(read(&mut simulation), ["You're not hurt."]);
    assert_eq!(count(&simulation, "herb"), 3);
    assert_eq!(
        menu(&simulation),
        "What will you do?",
        "the turn isn't used up"
    );
    assert_eq!(battle_hp(&simulation), (36, 14));
}

#[test]
fn a_creature_that_comes_up_beside_the_player_faces_them() {
    let mut simulation = simulation();
    hunt(&mut simulation);
    // To Gloamfang's right, looking up the hill rather than at it.
    place_player(&simulation, [13, 6], "up");
    simulation.step();
    let facing = |name: &str| -> String {
        eval(&simulation, &format!("{}.mover:facing()", character(name)))
    };
    assert_eq!(facing("Gloamfang"), "right");
    assert_eq!(
        eval::<String>(&simulation, "rpg.player.mover:facing()"),
        "left"
    );
}

/// Picks the choice labeled `label` from the menu that's open.
fn choose_labeled(simulation: &mut Simulation<LuauGame>, label: &str) {
    let choices: Vec<String> = eval(simulation, "rpg.dialogue.box.choices");
    let number = choices
        .iter()
        .position(|choice| choice == label)
        .unwrap_or_else(|| panic!("no choice {label} in {choices:?}"));
    choose(simulation, number + 1);
}

fn choices(simulation: &Simulation<LuauGame>) -> Vec<String> {
    eval(simulation, "rpg.dialogue.box.choices")
}

const HAZEL: [i32; 2] = [25, 10];

/// Talks to Hazel from below, and reads up to the first question, if she asks one.
fn talk_to_hazel(simulation: &mut Simulation<LuauGame>) -> Vec<String> {
    place_player(simulation, [HAZEL[0], HAZEL[1] + 1], "up");
    simulation.tap(Key::Space);
    read(simulation)
}

/// Opens the shop during the hunt with `coins` to spend, up to its first menu.
fn open_shop(simulation: &mut Simulation<LuauGame>, coins: u32) {
    exec(
        simulation,
        &format!("rpg.quest = 'hunt' rpg.coins = {coins}"),
    );
    talk_to_hazel(simulation);
    assert!(menu(simulation).starts_with("What can I do for you?"));
}

fn count(simulation: &Simulation<LuauGame>, item: &str) -> u32 {
    eval(simulation, &format!("rpg.items.{item} or 0"))
}

fn equipped(simulation: &Simulation<LuauGame>, slot: &str) -> Option<String> {
    eval(simulation, &format!("rpg.equipped.{slot}"))
}

#[test]
fn hazel_keeps_the_shop_shut_until_the_hunt() {
    let mut simulation = simulation();
    let pages = talk_to_hazel(&mut simulation);
    assert!(pages[0].contains("we're shut"), "{pages:?}");
    assert_eq!(dialogue(&simulation), None);

    open_shop(&mut simulation, 0);
    assert_eq!(choices(&simulation), ["Buy", "Sell", "Leave"]);
    choose_labeled(&mut simulation, "Leave");
    read(&mut simulation);
    assert_eq!(eval::<i32>(&simulation, "rpg.busy"), 0, "the visit is over");
}

#[test]
fn buying_gear_spends_coins_and_offers_to_equip_it() {
    let mut simulation = simulation();
    open_shop(&mut simulation, 25);
    choose_labeled(&mut simulation, "Buy");
    assert!(menu(&simulation).ends_with("You have 25 coins."));
    // Prices sit beside each item, and the highlighted item's note says what it does.
    simulation.tap(Key::Space);
    for _ in 0..2 {
        simulation.tap(Key::Down);
    }
    let (detail, note): (String, String) = eval(
        &simulation,
        "rpg.dialogue.box.details[3], rpg.dialogue.box.notes[rpg.dialogue.box.selected]",
    );
    assert_eq!(detail, "10");
    assert!(note.contains("Attack +2."), "{note}");
    simulation.tap(Key::Enter);
    assert_eq!(menu(&simulation), "The Oak Staff? That'll be 10 coins.");
    choose_labeled(&mut simulation, "Buy it");
    read(&mut simulation);
    assert_eq!(menu(&simulation), "Equip the Oak Staff now?");
    choose_labeled(&mut simulation, "Equip it");
    read(&mut simulation);

    assert_eq!(eval::<u32>(&simulation, "rpg.coins"), 15);
    assert_eq!(count(&simulation, "oak_staff"), 1);
    assert_eq!(
        equipped(&simulation, "weapon").as_deref(),
        Some("oak_staff")
    );
    assert!(
        menu(&simulation).ends_with("You have 15 coins."),
        "back to buying"
    );
}

#[test]
fn the_shop_turns_away_the_player_who_cant_pay() {
    let mut simulation = simulation();
    open_shop(&mut simulation, 5);
    choose_labeled(&mut simulation, "Buy");
    choose_labeled(&mut simulation, "Oak Staff");
    let pages = read(&mut simulation);
    assert!(pages[0].contains("a little short"), "{pages:?}");
    assert_eq!(eval::<u32>(&simulation, "rpg.coins"), 5);
    assert_eq!(count(&simulation, "oak_staff"), 0);
}

#[test]
fn selling_pays_half_but_not_for_gear_in_use() {
    let mut simulation = simulation();
    exec(
        &simulation,
        "rpg.items.herb = 3 rpg.items.oak_staff = 1 rpg.equipped.weapon = 'oak_staff'",
    );
    open_shop(&mut simulation, 0);
    choose_labeled(&mut simulation, "Sell");
    assert_eq!(choices(&simulation), ["Herb", "Oak Staff", "Back"]);
    assert_eq!(
        eval::<Vec<String>>(&simulation, "rpg.dialogue.box.details"),
        ["2", "5"]
    );
    choose_labeled(&mut simulation, "Herb");
    assert_eq!(menu(&simulation), "I'll give you 2 coins for your Herb.");
    choose_labeled(&mut simulation, "Sell it");
    read(&mut simulation);
    assert_eq!(eval::<u32>(&simulation, "rpg.coins"), 2);
    assert_eq!(count(&simulation, "herb"), 2);

    choose_labeled(&mut simulation, "Oak Staff");
    let pages = read(&mut simulation);
    assert!(pages[0].contains("Take it off first"), "{pages:?}");
    assert_eq!(count(&simulation, "oak_staff"), 1);
}

#[test]
fn the_bag_changes_gear() {
    let mut simulation = simulation();
    exec(
        &simulation,
        "rpg.items.oak_staff = 1 rpg.items.wooden_shield = 1 rpg.items.ward_charm = 1",
    );
    simulation.tap(Key::Tab);
    assert!(eval::<bool>(&simulation, "rpg.bag.open"));
    assert_eq!(menu(&simulation), "Your bag.");
    choose_labeled(&mut simulation, "Gear");
    assert_eq!(choices(&simulation), ["Weapon", "Shield", "Charm", "Back"]);
    choose_labeled(&mut simulation, "Weapon");
    assert_eq!(choices(&simulation), ["Oak Staff", "Nothing", "Back"]);
    choose_labeled(&mut simulation, "Oak Staff");
    assert_eq!(
        equipped(&simulation, "weapon").as_deref(),
        Some("oak_staff")
    );
    assert_eq!(
        eval::<Vec<String>>(&simulation, "rpg.dialogue.box.details"),
        ["Oak Staff", "—", "—"]
    );

    // A charm that raises max HP doesn't heal, and taking it off lowers hit points to fit.
    choose_labeled(&mut simulation, "Charm");
    choose_labeled(&mut simulation, "Ward Charm");
    assert_eq!(eval::<u32>(&simulation, "rpg.hp"), 36);
    exec(&simulation, "rpg.hp = 46");
    choose_labeled(&mut simulation, "Charm");
    choose_labeled(&mut simulation, "Nothing");
    assert_eq!(equipped(&simulation, "charm"), None);
    assert_eq!(eval::<u32>(&simulation, "rpg.hp"), 36);

    // Backing out twice closes the bag.
    simulation.tap(Key::Space);
    simulation.tap(Key::Escape);
    simulation.tap(Key::Space);
    simulation.tap(Key::Escape);
    assert_eq!(dialogue(&simulation), None);
    assert!(!eval::<bool>(&simulation, "rpg.bag.open"));
    assert_eq!(eval::<i32>(&simulation, "rpg.busy"), 0);
}

#[test]
fn herbs_can_be_used_from_the_bag() {
    let mut simulation = simulation();
    exec(&simulation, "rpg.hp = 10 rpg.items.herb = 1");
    simulation.tap(Key::I);
    choose_labeled(&mut simulation, "Items");
    choose_labeled(&mut simulation, "Herb");
    let pages = read(&mut simulation);
    assert_eq!(pages[0], "You used a Herb and recovered 15 HP.");
    assert_eq!(eval::<u32>(&simulation, "rpg.hp"), 25);
    assert_eq!(count(&simulation, "herb"), 0);
    assert_eq!(pages[1], "You don't have anything to use.");
    assert_eq!(menu(&simulation), "Your bag.");
}

#[test]
fn examining_things_turns_up_coins_and_items() {
    let mut simulation = simulation();
    // The player's cottage, with their savings inside.
    place_player(&simulation, [6, 6], "up");
    simulation.tap(Key::Space);
    let pages = read(&mut simulation);
    assert!(pages[0].contains("10 coins"), "{pages:?}");
    assert_eq!(eval::<u32>(&simulation, "rpg.coins"), 10);
    simulation.tap(Key::Space);
    let pages = read(&mut simulation);
    assert!(pages[0].contains("too nice a day"), "only once: {pages:?}");
    assert_eq!(eval::<u32>(&simulation, "rpg.coins"), 10);

    // A feather among the bones at the shrine.
    climb_to_the_shrine(&mut simulation);
    place_player(&simulation, [15, 6], "up");
    simulation.tap(Key::Space);
    read(&mut simulation);
    assert_eq!(count(&simulation, "swift_feather"), 1);
}

#[test]
fn gear_counts_in_battle() {
    let mut simulation = simulation();
    hunt(&mut simulation);
    exec(
        &simulation,
        "rpg.items.oak_staff = 1 rpg.items.ward_charm = 1
        rpg.equipped.weapon = 'oak_staff' rpg.equipped.charm = 'ward_charm'",
    );
    meet(&mut simulation, "Shade Bat");
    let (attack, max_hp): (u32, u32) = eval(
        &simulation,
        "rpg.battle.state.player.attack, rpg.battle.state.player.max_hp",
    );
    assert_eq!((attack, max_hp), (8, 46));
    attack_with_strike(&mut simulation);
    // 8 power, 8 attack against 3 defense, halved.
    assert_eq!(battle_hp(&simulation), (32, 3));
}

#[test]
fn battles_only_offer_things_to_use() {
    let mut simulation = simulation();
    hunt(&mut simulation);
    exec(&simulation, "rpg.items.oak_staff = 1");
    meet(&mut simulation, "Shade Bat");
    choose_labeled(&mut simulation, "Item");
    assert_eq!(choices(&simulation), ["Herb", "Back"]);
}

/// Picks Strike from the battle menu, and reads what happens up to the next menu.
fn attack_with_strike(simulation: &mut Simulation<LuauGame>) {
    attack(simulation);
    read(simulation);
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
