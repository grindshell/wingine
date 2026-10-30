-- A top-down RPG, written entirely in Luau. Walk with the arrow keys or WASD; holding a direction
-- keeps walking. Press Space, Enter or E to examine whatever is in front of you, or to talk.
--
-- The elder of Mossbrook asks you to check on the Stone Shrine, on the hill north of the
-- village. This is the start of the quest: exploring the village, the elder's request, and the
-- climb to the shrine.
--
-- The world and its maps are in `world.luau` and `maps/`, the people in `characters.luau`, the
-- dialogue box in `dialogue.luau`, and conversations and events in `story.luau`. The game's state
-- is in the global table `rpg`, where tests can see it.
--
-- Run with `cargo run -- examples/rpg`. The tileset is Kenney's 1-bit Pack
-- (<https://kenney.nl/assets/1-bit-pack>, CC0). Its sprites are white on transparent, so they're
-- tinted when drawn.

local characters = require("./characters")
local dialogue = require("./dialogue")
local story = require("./story")
local window = require("./window")
local world = require("./world")

local MAPS = {
    village = require("./maps/village"),
    shrine = require("./maps/shrine"),
}

local WIDTH = window.resolution.width
local HEIGHT = window.resolution.height
local TILE = world.TILE

-- Where the game begins.
local START = { map = "village", at = { 20, 13 }, facing = "up" }
-- Seconds the player takes to walk one tile.
local STEP_TIME = 0.16
-- Seconds between thuds while walking into something.
local BUMP_SECONDS = 0.3
-- Seconds the screen takes to fade out, and back in, between maps.
local FADE_SECONDS = 0.25
-- Seconds a map's name shows on arriving, and spends fading in and out.
local BANNER_SECONDS = 2.5
local BANNER_FADE = 0.5
local MUSIC_VOLUME = 0.5
-- Movement actions, which are also the directions they walk in, checked in this order.
local DIRECTIONS = { "up", "down", "left", "right" }
-- What to do next, at each stage of the quest.
local OBJECTIVES = {
    start = "Talk to Elder Rowan.",
    accepted = "Check on the shrine, north of the village.",
    seen_shrine = "Tell Elder Rowan what you found.",
    reported = "To be continued…",
}

local BACKGROUND = "#101418"
local HERO = "#f2d3ab"
local HUD_PANEL = { r = 0, g = 0, b = 0, a = 0.6 }
local TEXT = "#f4f0e8"

input.bind("up", { "Up", "W" })
input.bind("down", { "Down", "S" })
input.bind("left", { "Left", "A" })
input.bind("right", { "Right", "D" })
input.bind("interact", { "Space", "Enter", "E" })
input.bind("cancel", { "Escape", "Backspace" })

local bump = audio.load("sounds/bump.wav")
audio.set_music_volume(MUSIC_VOLUME)

local font = graphics.default_font()

-- Everything that changes as the game plays. It's global so that tests can see it.
rpg = {
    -- Every level the player has been to, by name, so what changes on them lasts.
    levels = {} :: { [string]: world.Level },
    -- The level the player is on.
    level = nil :: world.Level?,
    player = characters.new({
        name = "you",
        sprite = world.sprite(27, 0),
        tint = HERO,
        tile = START.at,
        step_time = STEP_TIME,
    }),
    -- How far the quest has got: see `story.luau`.
    quest = "start",
    -- How many events are running, such as conversations. The player waits for them.
    busy = 0,
    -- A direction tapped during a step, to walk once the step ends.
    queued = nil :: string?,
    -- Seconds until walking into something can thud again.
    bump_cooldown = 0,
    -- How dark the screen is, from 0 to 1, while moving between maps.
    fade = 0,
    -- The map's name, shown on arriving, and when it started showing.
    banner = nil :: { text: string, start: number }?,
    -- The music playing, if any.
    music = nil :: string?,
    time = 0,
    dialogue = dialogue,
}

-- Puts the player on the map `name`, building its level the first time, on cell `at`, facing
-- `facing`.
local function enter(name: string, at: { number }, facing: string)
    local level = rpg.levels[name]
    if not level then
        level = world.build(MAPS[name] or error(`there's no map called {name}`))
        rpg.levels[name] = level
    end
    if rpg.level then
        world.remove(rpg.level, rpg.player)
    end
    rpg.level = level
    rpg.player.mover:place(at[1], at[2])
    characters.face(rpg.player, facing)
    world.add(level, rpg.player)
    rpg.queued = nil
    rpg.banner = { text = level.title, start = rpg.time }
    if level.music ~= rpg.music then
        if level.music then
            audio.play_music(level.music, 1)
        else
            audio.stop_music(1)
        end
        rpg.music = level.music or nil
    end
end

-- Starts `f(...)` as a task, such as a conversation. The player waits until it's done.
local function run_event(f, ...)
    rpg.busy += 1
    spawn(function(...)
        local ok, err = pcall(f, ...)
        rpg.busy -= 1
        if not ok then
            error(err, 0)
        end
    end, ...)
end

-- Fades the screen to `target` darkness over `seconds`.
local function fade_to(target: number, seconds: number)
    local from, start = rpg.fade, rpg.time
    repeat
        wait()
        local progress = math.min((rpg.time - start) / seconds, 1)
        rpg.fade = from + (target - from) * progress
    until progress >= 1
end

-- Takes the player through `exit` to another map.
local function travel(exit)
    fade_to(1, FADE_SECONDS)
    enter(exit.to, exit.arrive, exit.facing)
    fade_to(0, FADE_SECONDS)
end

-- Talks to `character`, who turns to face the player, or examines it if it has nothing to say.
local function talk(character: characters.Character)
    run_event(function()
        character.talking = true
        characters.face(character, grid.opposite(rpg.player.mover:facing()))
        local ok, err = pcall(function()
            if character.talk then
                story.talk[character.talk](character)
            else
                dialogue.say(character.text or `It's {character.name}.`)
            end
        end)
        character.talking = false
        if not character.route then
            characters.face(character, character.home_facing)
        end
        if not ok then
            error(err, 0)
        end
    end)
end

-- What the player would do by pressing the interact key, facing what they face: "talk",
-- "examine", or nil when there's nothing in particular there. The ground can be examined too,
-- but it's not worth a prompt.
local function interaction(): string?
    local level = rpg.level :: world.Level
    local column, row = characters.facing_tile(rpg.player)
    if world.character_at(level, column, row) then
        return "talk"
    elseif world.place_at(level, column, row) or world.has_object(level, column, row) then
        return "examine"
    end
    return nil
end

-- Talks to whoever is in front of the player, or examines the cell there.
local function interact()
    local level = rpg.level :: world.Level
    local column, row = characters.facing_tile(rpg.player)
    local character = world.character_at(level, column, row)
    if character then
        talk(character)
        return
    end
    local place = world.place_at(level, column, row)
    if place and place.event then
        run_event(story.events[place.event], place)
        return
    end
    local text = world.describe(level, column, row)
    if text then
        run_event(dialogue.say, text)
    end
end

-- The first direction whose action passes `test`, such as `input.held`.
local function direction_where(test: (string) -> boolean): string?
    for _, direction in DIRECTIONS do
        if test(direction) then
            return direction
        end
    end
    return nil
end

-- Walks the player with the keys, a tile at a time.
local function walk()
    local player = rpg.player
    local held = direction_where(input.held)
    -- A tap shorter than a frame is pressed but never held, so presses count too.
    local pressed = direction_where(input.pressed)
    local queued = rpg.queued
    rpg.queued = nil
    local direction = held or pressed or queued
    if not direction then
        return
    end
    local level = rpg.level :: world.Level
    local stepped = characters.step(player, direction, world.blocker(level, player))
    -- Walking into something thuds, but only now and then while the key is held.
    if not stepped and (held or pressed) and rpg.bump_cooldown == 0 then
        audio.play(bump)
        rpg.bump_cooldown = BUMP_SECONDS
    end
end

function update(dt: number)
    rpg.time += dt
    rpg.bump_cooldown = math.max(rpg.bump_cooldown - dt, 0)
    local level = rpg.level :: world.Level
    for _, character in level.characters do
        character.mover:update(dt)
    end
    if dialogue.is_open() then
        dialogue.update(dt)
        return
    end
    if rpg.busy > 0 then
        return
    end

    local player = rpg.player
    if player.mover:is_moving() then
        -- Presses last one frame, so remember a tap made mid-step for when the step ends.
        local pressed = direction_where(input.pressed)
        if pressed then
            rpg.queued = pressed
        end
        return
    end
    local exit = world.exit_at(level, player.mover:tile())
    if exit then
        run_event(travel, exit)
    elseif input.pressed("interact") then
        interact()
    else
        walk()
    end
end

-- Draws `text` on a dark panel, with its top-left corner at `x`, `y`.
local function label(text: string, x: number, y: number, color)
    local width, height = font:measure(text)
    graphics.fill_rect(x - 3, y - 2, width + 6, height + 2, HUD_PANEL)
    graphics.draw_text(font, text, x, y, color)
end

local function draw_hud()
    label(OBJECTIVES[rpg.quest], 5, 4, TEXT)

    local banner = rpg.banner
    if banner then
        local age = rpg.time - banner.start
        local fade_in = age / BANNER_FADE
        local fade_out = (BANNER_SECONDS - age) / BANNER_FADE
        local alpha = math.clamp(math.min(fade_in, fade_out), 0, 1)
        if alpha > 0 then
            local title = font:scaled(2)
            local width, height = title:measure(banner.text)
            local x, y = math.round((WIDTH - width) / 2), 24
            local backing = { r = 0, g = 0, b = 0, a = HUD_PANEL.a * alpha }
            graphics.fill_rect(x - 6, y - 4, width + 12, height + 4, backing)
            graphics.draw_text(title, banner.text, x, y, { r = 0.96, g = 0.94, b = 0.91, a = alpha })
        end
    end

    local action = if rpg.busy == 0 and not dialogue.is_open() then interaction() else nil
    if action then
        local prompt = `Space: {action}`
        local width, height = font:measure(prompt)
        label(prompt, math.round((WIDTH - width) / 2), HEIGHT - height - 4, TEXT)
    end
end

function draw()
    graphics.clear(BACKGROUND)
    local level = rpg.level :: world.Level
    local x, y = characters.position(rpg.player, level.map)
    graphics.set_camera(x + TILE / 2, y + TILE / 2, level.map:bounds())
    graphics.draw_tilemap(level.map)
    for _, character in level.characters do
        characters.draw(character, level.map)
    end

    graphics.set_camera(nil)
    draw_hud()
    dialogue.draw()
    if rpg.fade > 0 then
        graphics.fill_rect(0, 0, WIDTH, HEIGHT, { r = 0, g = 0, b = 0, a = rpg.fade })
    end
end

enter(START.map, START.at, START.facing)
