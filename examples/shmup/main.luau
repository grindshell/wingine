-- A bullet hell in the style of Touhou, written in Luau. Move with the arrow keys (or WASD), hold
-- Shift to move slowly and show your hitbox, hold Z to shoot and press X to bomb. Grazing bullets
-- scores; a bullet that touches your hitbox costs a life. Fairies come in waves, then the boss
-- with its spell cards: shoot one down without being hit or bombing for its bonus.
--
-- The game is also a benchmark. Its enemy bullets run through one interface, implemented in Luau
-- by `bullets.luau` and in Rust by the `danmaku` native library, which the game loads if it's in
-- its `native` folder: Tab switches between them. The stress test pours out bullets until
-- frames take longer than a 60th of a second, plotting frame time against the bullet count for
-- each.
--
-- The player is in `player.luau`, enemies in `enemies.luau`, the waves in `stage.luau`, the boss
-- and its spell cards in `boss.luau`, and the ways they fire in `patterns.luau`. `stress.luau` is
-- the stress test and `hud.luau` the panel and screens. The game's state is in the global table
-- `shmup`, where tests can see it.
--
-- Run with `cargo run --release -- examples/shmup`. `examples/native-danmaku` says how to build
-- the library and where to put it.

local art = require("./art")
local bullets = require("./bullets")
local enemies = require("./enemies")
local field = require("./field")
local hud = require("./hud")
local player = require("./player")
local sounds = require("./sounds")
local stage = require("./stage")
local stress = require("./stress")

-- Points for each life and bomb left at the end.
local LIFE_BONUS = 2000000
local BOMB_BONUS = 500000
-- Seconds a message shows.
local MESSAGE_SECONDS = 2.5
-- Stars drifting down behind the field, at up to this many pixels a second.
local STARS = 60
local STAR_SPEED = 40
local STAR_COLORS = { graphics.color("#3a405c"), graphics.color("#5a628c"), graphics.color("#8a92b8") }

local BACKGROUND = graphics.color("#14182c")
local FLASH = graphics.color({ r = 1, g = 1, b = 1, a = 0.35 })

local MENU = { "Play", "Boss practice", "Stress test" }

input.bind("up", { "Up", "W" })
input.bind("down", { "Down", "S" })
input.bind("left", { "Left", "A" })
input.bind("right", { "Right", "D" })
input.bind("focus", { "LeftShift", "RightShift" })
input.bind("shoot", { "Z", "J" })
input.bind("bomb", { "X", "K" })
input.bind("confirm", { "Z", "J", "Space", "Enter" })
input.bind("pause", { "Escape", "P" })
input.bind("switch", { "Tab" })
input.bind("restart", { "R" })

-- The bullet systems there are: Luau's own, and Rust's if the library loaded.
local systems = { { name = "Luau", bullets = bullets } }
if danmaku then
    table.insert(systems, { name = "native", bullets = danmaku })
end
for _, system in systems do
    art.register(system.bullets)
    system.bullets.set_bounds(0, 0, field.WIDTH, field.HEIGHT, field.BULLET_MARGIN)
end

-- The same stars each time the game runs, so that it looks the same each time.
math.randomseed(0)
local stars = {}
for i = 1, STARS do
    stars[i] = {
        x = math.random() * field.WIDTH,
        y = math.random() * field.HEIGHT,
        speed = 8 + math.random() * STAR_SPEED,
        color = STAR_COLORS[math.random(#STAR_COLORS)],
    }
end

-- Everything that changes as the game plays. It's global so that tests can see it.
shmup = {
    -- "title", "play", "over" (out of lives), "clear" (the boss is beaten) or "stress".
    mode = "title",
    -- The title menu's selected item.
    selected = 1,
    -- The bullet system running, and its name.
    bullets = systems[1].bullets,
    system = systems[1].name,
    -- What `math.random` starts from each game, so a game's bullets come out the same each time.
    seed = 1,
    -- For tests: `invincible` keeps the player from being hit, and `damage` multiplies the
    -- damage shots and bombs do.
    cheats = { invincible = false, damage = 1 },
    paused = false,
    player = player.new(),
    enemies = {} :: { enemies.Enemy },
    shots = {},
    score = 0,
    graze = 0,
    -- Seconds the game has been playing.
    time = 0,
    -- The script running the stage, and the boss, once it comes.
    stage = nil :: thread?,
    boss = nil :: enemies.Enemy?,
    -- The boss's attack, while one runs: see `boss.luau`.
    card = nil :: any,
    -- A message across the field, and the seconds it has left.
    message = nil :: { headline: string, detail: string?, time_left: number }?,
    -- The stress test: see `stress.luau`.
    stress = nil :: any,
    -- Updates run so far this frame, and in the frame before, for the stress test's timing.
    frame_updates = 0,
    last_frame_updates = 1,
}

-- Shows `headline`, and `detail` under it, across the field for a while.
function shmup.show(headline: string, detail: string?)
    shmup.message = { headline = headline, detail = detail, time_left = MESSAGE_SECONDS }
end

-- Starts a game from the beginning, or with `practice` "boss", from the boss.
function shmup.start(practice: string?)
    math.randomseed(shmup.seed)
    shmup.mode = "play"
    shmup.paused = false
    shmup.player = player.new()
    shmup.enemies = {}
    shmup.shots = {}
    shmup.score = 0
    shmup.graze = 0
    shmup.time = 0
    shmup.boss = nil
    shmup.card = nil
    shmup.message = nil
    shmup.bullets.clear()
    shmup.stage = coroutine.create(stage.run)
    enemies.resume(shmup.stage, practice)
end

-- Ends the game with the boss beaten, paying for the lives and bombs left.
function shmup.finish()
    local p = shmup.player
    local bonus = math.max(p.lives, 0) * LIFE_BONUS + p.bombs * BOMB_BONUS
    shmup.score += bonus
    shmup.mode = "clear"
    shmup.clear_bonus = bonus
end

function shmup.start_stress()
    math.randomseed(shmup.seed)
    shmup.mode = "stress"
    shmup.enemies = {}
    shmup.shots = {}
    shmup.boss = nil
    shmup.card = nil
    shmup.message = nil
    stress.start()
end

-- Runs the bullets in the next system, which takes over the last one's bullets as they are, so
-- they carry on as though nothing changed. The stress test starts again instead.
function shmup.switch_system()
    if #systems == 1 then
        if shmup.mode == "play" then
            shmup.show("No danmaku library", "Bullets run in Luau")
        end
        return
    end
    local index = 1
    for i, system in systems do
        if system.name == shmup.system then
            index = i % #systems + 1
        end
    end
    local from, to = shmup.bullets, systems[index].bullets
    to.clear()
    if shmup.mode ~= "stress" then
        for i = 1, from.count() do
            to.add(from.bullet(i))
        end
    end
    from.clear()
    shmup.bullets = to
    shmup.system = systems[index].name
    if shmup.mode == "stress" then
        stress.start()
    end
end

local function update_stars(dt: number)
    for _, star in stars do
        star.y += star.speed * dt
        if star.y > field.HEIGHT then
            -- Somewhere else across, without `math.random`, whose numbers are the game's: a
            -- game's bullets come out the same each time only if nothing else takes them.
            star.y -= field.HEIGHT
            star.x = (star.x + field.WIDTH * 0.618) % field.WIDTH
        end
    end
end

local function update_title()
    if input.pressed("up") then
        shmup.selected = (shmup.selected - 2) % #MENU + 1
    elseif input.pressed("down") then
        shmup.selected = shmup.selected % #MENU + 1
    elseif input.pressed("confirm") then
        local item = MENU[shmup.selected]
        if item == "Play" then
            shmup.start()
        elseif item == "Boss practice" then
            shmup.start("boss")
        else
            shmup.start_stress()
        end
    end
end

local function update_play(dt: number)
    if input.pressed("pause") then
        shmup.paused = not shmup.paused
    end
    if shmup.paused then
        if input.pressed("bomb") then
            shmup.mode = "title"
            shmup.bullets.clear()
        end
        return
    end
    shmup.time += dt
    if shmup.stage and not enemies.resume(shmup.stage, dt) then
        shmup.stage = nil
    end
    enemies.update(dt)
    player.update(dt)
    player.update_shots(dt)
    shmup.bullets.step(dt)
    player.collide()
    if shmup.message then
        shmup.message.time_left -= dt
        if shmup.message.time_left <= 0 then
            shmup.message = nil
        end
    end
end

function update(dt: number)
    shmup.frame_updates += 1
    update_stars(dt)
    if input.pressed("switch") then
        shmup.switch_system()
    end
    if shmup.mode == "title" then
        update_title()
    elseif shmup.mode == "play" then
        update_play(dt)
    elseif shmup.mode == "stress" then
        if input.pressed("pause") then
            shmup.mode = "title"
            shmup.bullets.clear()
        elseif input.pressed("restart") then
            stress.start()
        else
            stress.update(dt)
        end
    elseif input.pressed("confirm") then
        -- Game over, or the stage cleared.
        shmup.mode = "title"
        shmup.bullets.clear()
    end
end

function draw()
    shmup.last_frame_updates = shmup.frame_updates
    shmup.frame_updates = 0
    graphics.clear(BACKGROUND)
    field.camera()
    for _, star in stars do
        graphics.fill_rect(star.x, star.y, 1, if star.speed > 30 then 2 else 1, star.color)
    end
    if shmup.mode ~= "title" and shmup.mode ~= "stress" then
        enemies.draw()
        player.draw()
    end
    shmup.bullets.draw()
    if shmup.player.bombing > 0 and shmup.mode == "play" then
        graphics.fill_rect(0, 0, field.WIDTH, field.HEIGHT, FLASH)
    end
    if shmup.mode == "play" then
        hud.boss()
    end

    if shmup.mode == "title" then
        hud.title(MENU, shmup.selected)
    elseif shmup.mode == "over" then
        hud.banner("Game over", `Score {hud.number(shmup.score)} - Z to go on`)
    elseif shmup.mode == "clear" then
        hud.banner("Stage clear!", `Score {hud.number(shmup.score)} - Z to go on`)
    elseif shmup.paused then
        hud.banner("Paused", "Esc to go on, X to quit")
    elseif shmup.message then
        hud.banner(shmup.message.headline, shmup.message.detail)
    end

    graphics.set_camera(nil)
    hud.frame()
    hud.panel()
end

sounds.start_music()
