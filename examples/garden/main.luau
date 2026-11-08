-- A garden around a windmill, laid out in a scene file, `garden.scene.luau`, which this script
-- loads and plays. Walk with the arrow keys or WASD; standing beside something tells you about it.
--
-- Everything the garden holds, and where, is in the scene file: the trees and flowers, the pond,
-- the mill and its sails, the birds, the player, and the text at the top and the note at the
-- bottom. A scene file is plain data, so a tool could write it as well as a person; this one is
-- in the canonical layout the engine writes scenes in. This script only brings it to life: it
-- finds nodes by their paths and tags, reads the props the scene gives them, and moves and turns
-- them.
--
-- - Turning a node turns everything inside it. The mill's `sails` node holds four sails, each an
--   arm and a cloth, so turning it turns them all; the `flock` node holds two birds, which circle
--   as it turns.
-- - Nodes tagged `solid` are in the player's way, and areas tagged `note` show their `text` when
--   the player stands in them. The player walks with `physics`: a world of colliders covering the
--   solid nodes as they're drawn, and the ground's edges, which the player's feet stop at and
--   slide along. The notes' areas are sensors in it, which nothing bumps into but which tell
--   what's in them, and the player is in it too, covered where it's drawn, for them to find.
-- - The player turns and mirrors about its pivot, the middle of its feet: it waddles as it walks,
--   and faces the way it's going.
-- - The trees and flowers are instances of `tree.scene.luau` and `flower.scene.luau`, each changed
--   by overrides: another sprite, another tint. Pressing Space picks the flower the player stands
--   on, removing it, or throws a seed the way it faces: a ray finds where it lands, short of
--   anything solid in the way, and a flower grows there, adding a copy of `flower.scene.luau` to
--   the garden, if a circle around it overlaps nothing solid.
-- - F1 shows what this script works with, drawn over the garden: outlines of what's solid and of
--   the areas with notes, the strip across the player's feet that bumps into them, where a seed
--   would land, and the nodes it turns, with lines out to what they hold and the circles they turn
--   it around.
--
-- Run with `cargo run -- examples/garden`. The tileset is Kenney's 1-bit Pack
-- (<https://kenney.nl/assets/1-bit-pack>, CC0). Its sprites are white on transparent, so the scene
-- tints them. The game's state is in the global table `garden`, where tests can see it.

local garden_scene = scene.load("garden.scene.luau")

input.bind("left", { "Left", "A" })
input.bind("right", { "Right", "D" })
input.bind("up", { "Up", "W" })
input.bind("down", { "Down", "S" })
input.bind("tend", { "Space", "Enter", "E" })
input.bind("debug", { "F1" })

-- How far either side of the middle of its feet the player bumps into things.
local FOOT = 4
-- What of the player bumps into things, from its position, the middle of its feet: a strip a
-- pixel high across them.
local FEET = { x = -FOOT, y = -1.5, width = FOOT * 2, height = 1 }
-- How far the player waddles each way as it walks, in radians, and how quickly.
local WADDLE = 0.12
local WADDLE_SPEED = 14
-- How far the player throws a seed, how much room a flower needs, nothing solid nearer it than
-- this, and how far short of anything solid in its way a seed lands, a little more.
local THROW = 24
local ROOM = 5
local SHORT = ROOM + 1

local player = garden_scene:find("player")
local sails = garden_scene:find("mill/sails")
local flock = garden_scene:find("flock")

-- What's in the player's way: the solid nodes, and the ground's edges, which keep it on the
-- ground. The notes' areas are sensors, which find the player, in the world too.
local solids = garden_scene:tagged("solid")
local notes = garden_scene:tagged("note")
local world = physics.world()
for _, solid in solids do
    world:add(solid)
end
world:add(garden_scene:find("ground"), { edges = true })
for _, area in notes do
    world:add(area, { sensor = true })
end
world:add(player)

garden = {
    scene = garden_scene,
    player = player,
    -- Pixels a second.
    speed = player:props().speed,
    -- Seconds the player has been walking without stopping, for its waddle.
    walk_time = 0,
    solids = solids,
    -- The physics world the player walks in.
    world = world,
    -- The way the player faces, which it last walked, of length 1.
    facing = { x = 1, y = 0 },
    notes = notes,
    note = garden_scene:find("hud/note"),
    note_text = garden_scene:find("hud/note/text"),
    -- The area whose note is showing, if any.
    showing = nil,
    -- Where the flowers are, those the scene places and those the player plants, and how many
    -- the player has planted.
    flowers = garden_scene:find("flowers"),
    planted = 0,
    -- Whether F1's view of the script's workings is showing.
    debug = false,
}

-- The colors of F1's view: what's solid, the notes' areas, the player's feet, and the turning
-- nodes.
local DEBUG_SOLID = graphics.color("#ff5a4ad0")
local DEBUG_NOTE = graphics.color("#f2c14ed0")
local DEBUG_FEET = graphics.color("#5ad2ff")
local DEBUG_TURNING = graphics.color("#ffffff90")
-- Where a seed would land: green with room for a flower, red without.
local DEBUG_ROOM = graphics.color("#7ad27a")
local DEBUG_NO_ROOM = graphics.color("#ff5a4a")

-- Walks the player by the keys held, sliding along whatever it bumps into.
local function walk(dt: number)
    local dx, dy = 0, 0
    if input.held("left") then
        dx -= 1
    end
    if input.held("right") then
        dx += 1
    end
    if input.held("up") then
        dy -= 1
    end
    if input.held("down") then
        dy += 1
    end
    if dx == 0 and dy == 0 then
        garden.walk_time = 0
        player:set_rotation(0)
        return
    end
    -- As fast diagonally as straight.
    local length = math.sqrt(dx * dx + dy * dy)
    local step = garden.speed * dt / length
    garden.world:move(player, dx * step, dy * step, FEET)
    garden.facing = { x = dx / length, y = dy / length }
    if dx ~= 0 then
        player:set_scale(dx, 1)
    end
    garden.walk_time += dt
    player:set_rotation(math.sin(garden.walk_time * WADDLE_SPEED) * WADDLE)
end

-- Whether `nodes` has the player among them.
local function has_player(nodes: { SceneNode }): boolean
    for _, node in nodes do
        if node == player then
            return true
        end
    end
    return false
end

-- Shows the note of the area the player stands in, if it stands in one.
local function show_note()
    local here = nil
    for _, area in garden.notes do
        if has_player(garden.world:inside(area)) then
            here = area
            break
        end
    end
    if here ~= garden.showing then
        garden.showing = here
        garden.note:set_visible(here ~= nil)
        garden.note_text:set_text(if here then here:props().text else "")
    end
end

-- Where a seed thrown from the player's feet the way it faces lands, and whether a flower has
-- room to grow there: as far as it's thrown, or `SHORT` of the first solid thing in its way.
local function seed_landing(): (number, number, boolean)
    local x, y = player:position()
    local facing = garden.facing
    local hit = garden.world:ray(x, y, facing.x, facing.y, { distance = THROW, except = player })
    local reach = if hit then math.max(hit.distance - SHORT, 0) else THROW
    local seed_x, seed_y = x + facing.x * reach, y + facing.y * reach
    local crowded = garden.world:overlap_circle(seed_x, seed_y, ROOM, { except = player })
    return seed_x, seed_y, #crowded == 0
end

-- Picks the flower the player stands on, or if there isn't one, throws a seed, which grows where
-- it lands if there's room.
local function tend()
    local x, y = player:position()
    for _, flower in garden.flowers:children() do
        if flower:find("flower"):contains(x, y - 1) then
            flower:remove()
            return
        end
    end
    local seed_x, seed_y, room = seed_landing()
    if not room then
        return
    end
    garden.planted += 1
    local flower = garden.flowers:add(scene.load("flower.scene.luau"), `planted_{garden.planted}`)
    flower:set_position(seed_x, seed_y)
end

-- How fast the sails and the flock turn, from their props: radians a second.
local SAILS_SPEED = sails:props().speed
local FLOCK_SPEED = flock:props().speed

-- Turns `node` by `speed` radians a second, over `dt` seconds.
local function spin(node, speed: number, dt: number)
    node:set_rotation((node:rotation() + speed * dt) % (2 * math.pi))
end

function update(dt: number)
    spin(sails, SAILS_SPEED, dt)
    spin(flock, FLOCK_SPEED, dt)
    walk(dt)
    show_note()
    if input.pressed("tend") then
        tend()
    end
    if input.pressed("debug") then
        garden.debug = not garden.debug
    end
end

-- Outlines `node`'s bounds, if it has any.
local function outline(node, color: number)
    local bounds = node:bounds()
    if bounds then
        graphics.outline_rect(bounds.x, bounds.y, bounds.width, bounds.height, color)
    end
end

-- Draws F1's view of what the script works with.
local function draw_debug()
    for _, solid in garden.solids do
        outline(solid, DEBUG_SOLID)
    end
    for _, area in garden.notes do
        outline(area, DEBUG_NOTE)
    end
    local x, y = player:position()
    graphics.outline_rect(x + FEET.x, y + FEET.y, FEET.width, FEET.height, DEBUG_FEET)
    local seed_x, seed_y, room = seed_landing()
    local seed_color = if room then DEBUG_ROOM else DEBUG_NO_ROOM
    graphics.draw_line(x, y, seed_x, seed_y, seed_color)
    graphics.outline_circle(seed_x, seed_y, ROOM, seed_color)
    for _, node in { sails, flock } do
        local cx, cy = node:world_position()
        graphics.fill_circle(cx, cy, 2, DEBUG_TURNING)
        local reach = 0
        for _, child in node:children() do
            local child_x, child_y = child:world_position()
            graphics.draw_line(cx, cy, child_x, child_y, DEBUG_TURNING)
            reach = math.max(reach, math.sqrt((child_x - cx) ^ 2 + (child_y - cy) ^ 2))
        end
        if reach > 0 then
            graphics.outline_circle(cx, cy, reach, DEBUG_TURNING)
        end
    end
end

function draw()
    graphics.clear("#000000")
    graphics.draw_scene(garden.scene)
    if garden.debug then
        draw_debug()
    end
end
