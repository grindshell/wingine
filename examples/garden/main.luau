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
--   the player stands in them.
-- - The player turns and mirrors about its pivot, the middle of its feet: it waddles as it walks,
--   and faces the way it's going.
--
-- Run with `cargo run -- examples/garden`. The tileset is Kenney's 1-bit Pack
-- (<https://kenney.nl/assets/1-bit-pack>, CC0). Its sprites are white on transparent, so the scene
-- tints them. The game's state is in the global table `garden`, where tests can see it.

local garden_scene = scene.load("garden.scene.luau")

input.bind("left", { "Left", "A" })
input.bind("right", { "Right", "D" })
input.bind("up", { "Up", "W" })
input.bind("down", { "Down", "S" })

-- How far either side of the middle of its feet the player bumps into things.
local FOOT = 4
-- How far the player waddles each way as it walks, in radians, and how quickly.
local WADDLE = 0.12
local WADDLE_SPEED = 14

local player = garden_scene:find("player")
local sails = garden_scene:find("mill/sails")
local flock = garden_scene:find("flock")

garden = {
    scene = garden_scene,
    player = player,
    -- Pixels a second.
    speed = player:props().speed,
    -- Seconds the player has been walking without stopping, for its waddle.
    walk_time = 0,
    solids = garden_scene:tagged("solid"),
    notes = garden_scene:tagged("note"),
    note = garden_scene:find("hud/note"),
    note_text = garden_scene:find("hud/note/text"),
    -- The area whose note is showing, if any.
    showing = nil,
}

local WIDTH, HEIGHT = garden_scene:find("ground"):size()

-- Whether the player, with its feet at `x`, `y`, would be in something's way, or off the ground.
local function blocked(x: number, y: number): boolean
    if x - FOOT < 0 or x + FOOT > WIDTH or y < 1 or y > HEIGHT then
        return true
    end
    for _, solid in garden.solids do
        if solid:contains(x - FOOT, y - 1) or solid:contains(x + FOOT, y - 1) then
            return true
        end
    end
    return false
end

-- Walks the player by the keys held, one axis at a time, so it slides along whatever it bumps
-- into.
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
    local step = garden.speed * dt / math.sqrt(dx * dx + dy * dy)
    local x, y = player:position()
    if not blocked(x + dx * step, y) then
        x += dx * step
    end
    if not blocked(x, y + dy * step) then
        y += dy * step
    end
    player:set_position(x, y)
    if dx ~= 0 then
        player:set_scale(dx, 1)
    end
    garden.walk_time += dt
    player:set_rotation(math.sin(garden.walk_time * WADDLE_SPEED) * WADDLE)
end

-- Shows the note of the area the player stands in, if it stands in one.
local function show_note()
    local x, y = player:position()
    local here = nil
    for _, area in garden.notes do
        if area:contains(x, y - 1) then
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
end

function draw()
    graphics.clear("#000000")
    graphics.draw_scene(garden.scene)
end
