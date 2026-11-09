-- The Old Quarry: a level seen from the side, laid out in `quarry.scene.luau`, which this script
-- makes physical with `physics.world()`. Run with the arrow keys or A and D, and jump with Space,
-- Up or W: push a crate against the ledge, climb it, knock the boulder down the slope, and ride
-- the lift up to the door.
--
-- - The level is a tile map, solid where its tiles are, and a slope, a turned rectangle tagged
--   `solid`.
-- - The crates are dynamic bodies: they fall, slide and stack, and the player pushes them by
--   walking into them. The boulder is one too, a circle, which rolls.
-- - The lift is a kinematic body. The script moves its node up and down, and the world moves its
--   body there over each step, pushing what's on it, and carrying the player.
-- - The player is a character, which `world:walk` walks along the ground, up steps and slopes, and
--   along with what it stands on. The script keeps how fast it's falling, pulled down by gravity,
--   and jumps it.
-- - The door's area is a sensor, which tells when the player reaches it.
-- - R starts over, and F1 outlines the bodies and the box the player bumps into things with.
--
-- Run with `cargo run -- examples/quarry`. The tiles are Kenney's 1-bit Pack
-- (<https://kenney.nl/assets/1-bit-pack>, CC0). The game's state is in the global table `quarry`,
-- where tests can see it.

input.bind("left", { "Left", "A" })
input.bind("right", { "Right", "D" })
input.bind("jump", { "Space", "Up", "W" })
input.bind("restart", { "R" })
input.bind("debug", { "F1" })

-- What of the player bumps into things: a box this wide and tall, standing on the middle of its
-- feet.
local WIDTH = 8
local HEIGHT = 14

-- The colors of F1's outlines: the bodies, and the player's box.
local DEBUG_BODY = graphics.color("#5ad2ff")
local DEBUG_PLAYER = graphics.color("#7ad27a")

quarry = {}

-- Loads the level and makes it physical, starting the game over.
local function start()
    local level = scene.load("quarry.scene.luau")
    local player = level:find("player")
    local props = player:props()
    -- Dynamic bodies fall as the player does.
    local world = physics.world({ gravity = props.gravity })
    world:add(level:find("level"))
    for _, node in level:tagged("solid") do
        world:add(node)
    end
    local crates = level:tagged("crate")
    for _, crate in crates do
        world:add(crate, { body = "dynamic", friction = 0.6 })
    end
    local boulder = level:find("boulder")
    world:add(boulder, { body = "dynamic", shape = "circle", density = 2, friction = 0.8 })
    local lift = level:find("lift")
    world:add(lift, { body = "kinematic" })
    local exit = level:find("exit")
    world:add(exit, { sensor = true })
    world:add(player, { character = { width = WIDTH, height = HEIGHT, step = 4, mass = 2 } })

    quarry.scene = level
    quarry.world = world
    quarry.player = player
    quarry.crates = crates
    quarry.boulder = boulder
    quarry.lift = lift
    quarry.exit = exit
    quarry.won_text = level:find("hud/won")
    -- Pixels a second, and pixels a second each second.
    quarry.speed = props.speed
    quarry.jump = props.jump
    quarry.gravity = props.gravity
    -- How fast the player is falling, in pixels a second: rising while it's less than 0.
    quarry.falling = 0
    quarry.grounded = false
    -- Seconds the lift has been going, which say where it is.
    quarry.lift_time = 0
    quarry.won = false
    quarry.debug = false
end

-- Where the lift's top is after `time` seconds: it waits at the bottom, goes up at its speed,
-- waits at the top, and comes down again, as its props say. It goes the same way every run.
local function lift_height(time: number): number
    local props = quarry.lift:props()
    local travel = (props.bottom - props.top) / props.speed
    local at = time % (2 * (travel + props.pause))
    if at < props.pause then
        return props.bottom
    end
    at -= props.pause
    if at < travel then
        return props.bottom - at * props.speed
    end
    at -= travel
    if at < props.pause then
        return props.top
    end
    return props.top + (at - props.pause) * props.speed
end

-- Runs and jumps the player as the keys say, falling as gravity pulls it.
local function walk(dt: number)
    local across = if quarry.won then 0 else input.axis("left", "right")
    if quarry.grounded and not quarry.won and input.pressed("jump") then
        quarry.falling = -quarry.jump
    end
    quarry.falling += quarry.gravity * dt
    local fall = quarry.falling * dt
    local _, moved_y, grounded = quarry.world:walk(quarry.player, across * quarry.speed * dt, fall)
    quarry.grounded = grounded
    -- On the ground, or rising into a ceiling, it stops falling or rising.
    if grounded or (fall < 0 and moved_y > fall / 2) then
        quarry.falling = 0
    end
    if across ~= 0 then
        quarry.player:set_scale(across, 1)
    end
end

-- Whether the player stands in the door's area.
local function at_the_door(): boolean
    for _, node in quarry.world:inside(quarry.exit) do
        if node == quarry.player then
            return true
        end
    end
    return false
end

start()

function update(dt: number)
    if input.pressed("restart") then
        start()
        return
    end
    if input.pressed("debug") then
        quarry.debug = not quarry.debug
    end
    quarry.lift_time += dt
    local lift_x = quarry.lift:position()
    quarry.lift:set_position(lift_x, lift_height(quarry.lift_time))
    walk(dt)
    quarry.world:step(dt)
    if not quarry.won and at_the_door() then
        quarry.won = true
        quarry.won_text:set_visible(true)
    end
    -- Fallen out of the level somehow, it starts over.
    local _, y = quarry.player:position()
    if y > 320 then
        start()
    end
end

-- Outlines `node`'s bounds.
local function outline(node, color: number)
    local bounds = node:bounds()
    if bounds then
        graphics.outline_rect(bounds.x, bounds.y, bounds.width, bounds.height, color)
    end
end

function draw()
    graphics.clear("#1c1a24")
    graphics.draw_scene(quarry.scene)
    if quarry.debug then
        for _, crate in quarry.crates do
            outline(crate, DEBUG_BODY)
        end
        outline(quarry.lift, DEBUG_BODY)
        local x, y = quarry.boulder:position()
        graphics.outline_circle(x, y, 8, DEBUG_BODY)
        local feet_x, feet_y = quarry.player:position()
        graphics.outline_rect(feet_x - WIDTH / 2, feet_y - HEIGHT, WIDTH, HEIGHT, DEBUG_PLAYER)
    end
end
