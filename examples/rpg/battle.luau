-- Battles: the player against one creature from `foes.luau`, turn by turn. Each turn the player
-- picks a move, an item or running away from a menu, then the faster of the two acts first;
-- items and guarding always go first. Messages and menus use the dialogue box.
--
-- `battle.fight` runs a battle as part of a task and returns how it ended, leaving the battle on
-- screen until `battle.close`, so the game can fade away from it. The player's hit points, items
-- and coins are in `rpg`, and change as the battle goes. Their stats include their gear's.

local dialogue = require("./dialogue")
local foes = require("./foes")
local items = require("./items")
local ui = require("./ui")
local conf = require("./conf")
local world = require("./world")

local battle = {}

local WIDTH = conf.resolution.width
local HEIGHT = conf.resolution.height

-- Fighters are drawn this many times their size in the world.
local SCALE = 4
local SIZE = world.TILE * SCALE
-- Where the fighters stand, and their panels go.
local FOE_X, FOE_Y = 212, 14
local PLAYER_X, PLAYER_Y = 40, 70
local FOE_PANEL = { x = 8, y = 10, width = 120 }
local PLAYER_PANEL = { x = 130, y = 108, width = 120 }
local PANEL_HEIGHT = 30
-- Where the ground starts.
local HORIZON = 62

-- Seconds the screen flashes as a battle starts, and the fighters take to slide in.
local FLASH_SECONDS = 0.6
local FLASH_PERIOD = 0.2
local INTRO_SECONDS = 0.5
-- Seconds a fighter blinks after a hit, and takes to faint.
local BLINK_SECONDS = 0.5
local BLINK_PERIOD = 0.1
local FAINT_SECONDS = 0.6
-- Hit point bars move this share of a full bar per second.
local DRAIN_RATE = 1
-- Damage falls short of a move's full strength by up to this share, at random.
local VARIANCE = 0.15
-- Stages a stat can be raised or lowered by, each worth half of it.
local MAX_STAGE = 2

-- The player's moves.
local MOVES: { foes.Move } = {
    { name = "Strike", power = 8, weight = 1 },
    { name = "Focus", effect = "raise_attack", weight = 1 },
    { name = "Guard", effect = "guard", weight = 1 },
}

local SKY = "#141a2c"
local GROUND = "#24301f"
local PLATFORM = "#1a2417"
local FLASH = "#f4f0e8"
local BAR_EMPTY = "#2c3048"
local HEALTHY = "#5fc35a"
local HURT = "#e8c040"
local CRITICAL = "#d04a3a"
local HERO = "#f2d3ab"

local font = graphics.default_font()
local hit = audio.load("sounds/bump.wav")
local hero = world.sprite(27, 0)

-- A number from 0 up to 1, at random: how hard hits land, whether they miss, and which move a
-- creature picks. Tests replace it to make battles predictable.
battle.roll = function(): number
    return math.random()
end

type Fighter = {
    -- What it's called in messages, and the possessive, both in the middle of a sentence.
    called: string,
    possessive: string,
    hp: number,
    max_hp: number,
    attack: number,
    defense: number,
    speed: number,
    -- Stages each stat is raised (or with a negative number, lowered) by.
    stages: { [string]: number },
    guarding: boolean,
    -- The hit points the panel shows, moving toward `hp`.
    shown_hp: number,
    -- Seconds left of blinking after a hit.
    blink: number,
    -- Seconds since it fainted, once it has.
    fainted: number?,
}

type State = {
    foe_kind: foes.Foe,
    -- The foe's sprite, facing the player.
    foe_sprite: any,
    -- "flash" while the screen flashes over the map, then "fight".
    phase: string,
    -- Seconds since the phase began.
    time: number,
    player: Fighter,
    foe: Fighter,
}

-- The battle under way, if there is one.
battle.state = nil :: State?

local function fighter(called: string, possessive: string, stats, hp: number): Fighter
    return {
        called = called,
        possessive = possessive,
        hp = hp,
        max_hp = stats.max_hp or stats.hp,
        attack = stats.attack,
        defense = stats.defense,
        speed = stats.speed,
        stages = { attack = 0, defense = 0 },
        guarding = false,
        shown_hp = hp,
        blink = 0,
        fainted = nil,
    }
end

local function capitalize(text: string): string
    return string.upper(string.sub(text, 1, 1)) .. string.sub(text, 2)
end

local function say(text: string)
    dialogue.say(text)
end

function battle.is_active(): boolean
    return battle.state ~= nil
end

-- Whether the panels have caught up with what happened and nobody is blinking.
local function settled(): boolean
    local state = battle.state :: State
    for _, fighter in { state.player, state.foe } do
        if fighter.shown_hp ~= fighter.hp or fighter.blink > 0 then
            return false
        end
    end
    return true
end

-- `fighter`'s `stat`, with its stage applied.
local function stat(fighter: Fighter, name: string): number
    local stage = fighter.stages[name] or 0
    local value = fighter[name]
    return if stage >= 0 then value * (1 + stage / 2) else value / (1 - stage / 2)
end

-- The damage a move of `power` does from `user` to `target`.
local function damage(power: number, user: Fighter, target: Fighter): number
    local amount = power * stat(user, "attack") / stat(target, "defense") / 2
    amount *= 1 - VARIANCE * battle.roll()
    if target.guarding then
        amount /= 2
    end
    return math.max(math.round(amount), 1)
end

-- Raises or lowers `fighter`'s `stat` by a stage, by `change` (1 or -1).
local function change_stage(fighter: Fighter, name: string, change: number)
    local stage = fighter.stages[name]
    local whose = capitalize(fighter.possessive)
    if math.abs(stage + change) > MAX_STAGE then
        say(`{whose} {name} won't go any {if change > 0 then "higher" else "lower"}!`)
        return
    end
    fighter.stages[name] = stage + change
    say(`{whose} {name} {if change > 0 then "rose" else "fell"}!`)
end

-- `user` uses `move` on `target`.
local function act(user: Fighter, target: Fighter, move: foes.Move)
    say(`{capitalize(user.called)} used {move.name}!`)
    if move.power then
        if battle.roll() >= (move.accuracy or 1) then
            say("It missed!")
            return
        end
        target.hp = math.max(target.hp - damage(move.power, user, target), 0)
        target.blink = BLINK_SECONDS
        audio.play(hit)
        wait_until(settled)
    elseif move.effect == "guard" then
        user.guarding = true
        say("You brace yourself for the next blow.")
    elseif move.effect == "raise_attack" then
        change_stage(user, "attack", 1)
    elseif move.effect == "raise_defense" then
        change_stage(user, "defense", 1)
    elseif move.effect == "lower_defense" then
        change_stage(target, "defense", -1)
    else
        error(`{move.name} has no power and an unknown effect`)
    end
end

-- The player uses one of the item `name`, which `choose_action` checked will do some good.
local function use_item(state: State, name: string)
    local player = state.player
    local healed = items.healing(name, player.hp, player.max_hp)
    items.take(name)
    say(`You used a {items.kind(name).name}.`)
    player.hp += healed
    wait_until(settled)
    say(`You recovered {healed} HP.`)
end

-- Asks the player what to do this turn, until they settle on something: a move, an item or
-- running away.
local function choose_action(state: State): { move: foes.Move?, item: string?, run: boolean? }
    while true do
        local actions = { "Fight", "Item", "Run" }
        local choice = dialogue.ask("What will you do?", actions, { cancel = false })
        if choice == 1 then
            local names = {}
            for i, move in MOVES do
                names[i] = move.name
            end
            table.insert(names, "Back")
            local picked = dialogue.ask("Which move?", names)
            if picked <= #MOVES then
                return { move = MOVES[picked] }
            end
        elseif choice == 2 then
            local owned = items.usable()
            if #owned == 0 then
                say("You don't have anything to use.")
            else
                local choices, options = items.menu(owned, items.count_detail)
                table.insert(choices, "Back")
                local picked = dialogue.ask("Use which item?", choices, options)
                local player = state.player
                if picked > #owned then
                    continue
                elseif items.healing(owned[picked], player.hp, player.max_hp) == 0 then
                    say("You're not hurt.")
                else
                    return { item = owned[picked] }
                end
            end
        else
            return { run = true }
        end
    end
end

-- One of the foe's moves, at random in proportion to their weights.
local function pick_move(kind: foes.Foe): foes.Move
    local total = 0
    for _, move in kind.moves do
        total += move.weight
    end
    local roll = battle.roll() * total
    for _, move in kind.moves do
        roll -= move.weight
        if roll < 0 then
            return move
        end
    end
    return kind.moves[#kind.moves]
end

local function faint(fighter: Fighter)
    fighter.fainted = 0
    audio.play(hit, { speed = 0.5 })
    wait(FAINT_SECONDS)
end

local function win(state: State): string
    local kind = state.foe_kind
    faint(state.foe)
    say(kind.defeat)
    rpg.coins += kind.coins
    say(`You found {kind.coins} coins.`)
    if kind.drops then
        items.give(kind.drops)
        say(`It left behind a {items.kind(kind.drops).name}.`)
    end
    return "won"
end

-- Plays a turn, and returns how the battle ended if it did.
local function turn(state: State): string?
    local player, foe = state.player, state.foe
    local action = choose_action(state)
    if action.run then
        if state.foe_kind.boss then
            say("There's no running from this fight!")
            return nil
        end
        say("You got away safely!")
        return "fled"
    end

    local foe_move = pick_move(state.foe_kind)
    local function player_turn()
        if action.item then
            use_item(state, action.item)
        else
            act(player, foe, action.move :: foes.Move)
        end
    end
    local function foe_turn()
        act(foe, player, foe_move)
    end
    local goes_first = action.item ~= nil
        or (action.move :: foes.Move).effect == "guard"
        or player.speed >= foe.speed
    local order = if goes_first then { player_turn, foe_turn } else { foe_turn, player_turn }
    for _, take_turn in order do
        take_turn()
        if foe.hp == 0 then
            return win(state)
        elseif player.hp == 0 then
            faint(player)
            say("You collapse…")
            return "lost"
        end
    end
    player.guarding = false
    return nil
end

local function run(state: State): string
    wait(FLASH_SECONDS)
    state.phase = "fight"
    state.time = 0
    wait(INTRO_SECONDS)
    say(state.foe_kind.intro)
    local outcome = nil
    while not outcome do
        outcome = turn(state)
    end
    return outcome
end

-- Fights a creature of the kind `kind`, and returns how it went: "won", "lost" or "fled". The
-- battle stays on screen until `battle.close`.
function battle.fight(kind: string): string
    local foe_kind = foes[kind] or error(`there's no kind of creature called {kind}`)
    local state: State = {
        foe_kind = foe_kind,
        foe_sprite = world.sprite(foe_kind.sprite[1], foe_kind.sprite[2]):flipped_x(),
        phase = "flash",
        time = 0,
        player = fighter("you", "your", items.stats(), rpg.hp),
        foe = fighter(foe_kind.called, `{foe_kind.called}'s`, foe_kind, foe_kind.hp),
    }
    battle.state = state
    local ok, result = pcall(run, state)
    rpg.hp = state.player.hp
    if not ok then
        battle.state = nil
        error(result, 0)
    end
    return result
end

-- Takes the battle off the screen.
function battle.close()
    battle.state = nil
end

-- Moves the panels and animations on.
function battle.update(dt: number)
    local state = battle.state
    if not state then
        return
    end
    state.time += dt
    for _, fighter in { state.player, state.foe } do
        local change = fighter.max_hp * DRAIN_RATE * dt
        if fighter.shown_hp > fighter.hp then
            fighter.shown_hp = math.max(fighter.shown_hp - change, fighter.hp)
        else
            fighter.shown_hp = math.min(fighter.shown_hp + change, fighter.hp)
        end
        fighter.blink = math.max(fighter.blink - dt, 0)
        if fighter.fainted then
            fighter.fainted += dt
        end
    end
end

-- A fighter's name and hit points, in a panel. The player's shows the numbers too.
local function draw_panel(fighter: Fighter, name: string, at, numbers: boolean)
    ui.panel(at.x, at.y, at.width, PANEL_HEIGHT)
    graphics.draw_text(font, name, at.x + 6, at.y + 4, ui.TEXT)
    if numbers then
        local text = `{math.ceil(fighter.shown_hp)}/{fighter.max_hp}`
        local width = font:measure(text)
        graphics.draw_text(font, text, at.x + at.width - 6 - width, at.y + 4, ui.TEXT)
    end
    graphics.draw_text(font, "HP", at.x + 6, at.y + 16, ui.TEXT)
    local bar_x, bar_y, bar_width = at.x + 22, at.y + 19, at.width - 28
    local share = fighter.shown_hp / fighter.max_hp
    local color = if share > 0.5 then HEALTHY elseif share > 0.2 then HURT else CRITICAL
    graphics.fill_rect(bar_x, bar_y, bar_width, 4, BAR_EMPTY)
    graphics.fill_rect(bar_x, bar_y, math.ceil(bar_width * share), 4, color)
end

-- A shadow for a fighter to stand on, centered on `x`.
local function platform(x: number, y: number)
    graphics.fill_rect(x - 20, y - 4, 40, 10, PLATFORM)
    graphics.fill_rect(x - 28, y - 2, 56, 6, PLATFORM)
    graphics.fill_rect(x - 32, y, 64, 2, PLATFORM)
end

-- A fighter, blinking after a hit, and sinking out of sight as it faints.
local function draw_fighter(fighter: Fighter, sprite, x: number, y: number, tint: string)
    if fighter.blink > 0 and fighter.blink % (BLINK_PERIOD * 2) > BLINK_PERIOD then
        return
    end
    local alpha = 1
    if fighter.fainted then
        local progress = math.min(fighter.fainted / FAINT_SECONDS, 1)
        y += progress * SIZE / 2
        alpha = 1 - progress
    end
    if alpha > 0 then
        local color = `{tint}{string.format("%02x", math.floor(alpha * 255))}`
        graphics.draw_sprite(sprite, x, math.round(y), color, SIZE, SIZE)
    end
end

-- Draws the battle: a flash over the map as it starts, then the battle itself.
function battle.draw()
    local state = battle.state
    if not state then
        return
    end
    if state.phase == "flash" then
        if state.time % FLASH_PERIOD < FLASH_PERIOD / 2 then
            graphics.fill_rect(0, 0, WIDTH, HEIGHT, FLASH)
        end
        return
    end

    graphics.clear(SKY)
    graphics.fill_rect(0, HORIZON, WIDTH, HEIGHT - HORIZON, GROUND)
    -- The foe slides in from the right, and the player from the left.
    local intro = 1 - math.min(state.time / INTRO_SECONDS, 1)
    local foe_x = math.round(FOE_X + intro * (WIDTH - FOE_X))
    local player_x = math.round(PLAYER_X - intro * (PLAYER_X + SIZE))
    platform(foe_x + SIZE / 2, FOE_Y + SIZE - 4)
    platform(player_x + SIZE / 2, PLAYER_Y + SIZE - 4)
    draw_fighter(state.foe, state.foe_sprite, foe_x, FOE_Y, state.foe_kind.tint)
    draw_fighter(state.player, hero, player_x, PLAYER_Y, HERO)
    draw_panel(state.foe, state.foe_kind.title, FOE_PANEL, false)
    draw_panel(state.player, "You", PLAYER_PANEL, true)
end

return battle
