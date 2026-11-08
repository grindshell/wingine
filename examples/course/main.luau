-- A course to run around in 3D, written entirely in Luau: up the stairs onto the platform, down
-- the slope, onto the block, and around the crates, picking up the coins. Walk with the arrow keys,
-- WASD or a gamepad's stick, and jump with Space or the gamepad's bottom button. Push the crates:
-- off the platform, or into the stack, which topples.
--
-- The course is a scene, `course.scene.luau`, which the editor edits: a 3D view of models from
-- Kenney's Prototype Kit. This script makes it physical with `physics.world_3d()`:
--
-- - Nodes tagged `solid` are fixed bodies, solid where their models' triangles are, so the stairs
--   are stairs and the slope a slope.
-- - Nodes tagged `crate` are dynamic bodies, boxes that fall, tumble and come to rest, moving
--   their nodes as the world steps. One pushed off the edge falls away and is gone.
-- - The player is a character: an upright cylinder the size of its model, which `world:walk`
--   walks across the ground, up the stairs and the slope, along the walls, and into the crates,
--   pushing them. This script keeps how fast it's falling, and jumps.
-- - Under the course, `fall` is a sensor, an area nothing bumps into: the player in it has fallen
--   off, and starts again, and crates in it are gone.
--
-- The world tells where things are, too. A ray straight down from the player puts its shadow on
-- whatever is below, so a jump shows where it lands. The coins aren't bodies: they spin, and the
-- player picks one up by coming near it, into a ball around it.
--
-- Run with `cargo run -- examples/course`. The models are Kenney's Prototype Kit
-- (<https://kenney.nl/assets/prototype-kit>, CC0). The game's state is in the global table
-- `course`, where tests can see it.

local level = scene.load("course.scene.luau")
local view = level:find("view") :: SceneNode
local player = view:find("player") :: SceneNode
local shadow = view:find("shadow") :: SceneNode
local fall = view:find("fall") :: SceneNode

input.bind("left", { "Left", "A", "DPadLeft", "LeftStickLeft" })
input.bind("right", { "Right", "D", "DPadRight", "LeftStickRight" })
input.bind("up", { "Up", "W", "DPadUp", "LeftStickUp" })
input.bind("down", { "Down", "S", "DPadDown", "LeftStickDown" })
input.bind("jump", { "Space", "South" })

-- Units a second the player walks, and how fast it rises as it jumps.
local SPEED = 2.5
local JUMP = 5.5
-- How fast the player's falling speeds up, each second: faster than the crates fall, as games'
-- characters do, so jumps are quick.
local GRAVITY = 18
-- How near the player a coin must come to be picked up: how far a ball around the coin reaches.
local REACH = 0.25
-- How high the player's middle is over its feet, where its shadow's ray starts.
local MIDDLE = 0.35
-- How far above what's below the shadow lies, so it isn't hidden in it.
local SHADOW_LIFT = 0.01
local DOWN = vector.create(0, -1, 0)
-- How fast the coins spin, in radians a second, and how far and fast they bob.
local SPIN = 3
local BOB = 0.05
-- Where the camera is from the point it looks at, a little over the player's feet.
local CAMERA_OFFSET = vector.create(0, 4.5, 5.5)
local LOOK_ABOVE = vector.create(0, 0.4, 0)
-- How far the player waddles each way as it walks, in radians, and how quickly.
local WADDLE = 0.15
local WADDLE_SPEED = 14

local world = physics.world_3d()
for _, node in view:tagged("solid") do
    world:add(node)
end
local crates = view:tagged("crate")
for _, crate in crates do
    world:add(crate, { body = "dynamic", shape = "box", density = 2 })
end
world:add(player, {
    character = { radius = 0.2, height = 0.7, step = 0.3, slope = 0.9, mass = 1 },
})
world:add(fall, { sensor = true, shape = "box" })
-- The shadow's ray goes through the player.
local SHADOW_RAY = { except = player }

type Coin = { node: SceneNode, home: vector, found: boolean }
local coins: { Coin } = {}
for _, node in view:tagged("coin") do
    table.insert(coins, { node = node, home = node:position_3d(), found = false })
end

course = {
    level = level,
    view = view,
    world = world,
    player = player,
    shadow = shadow,
    fall = fall,
    start = player:position_3d(),
    -- How fast the player is rising, or falling if it's negative, in units a second.
    rising = 0,
    -- Whether the player ended its last walk on the ground, where it can jump.
    grounded = true,
    -- Which way the player faces, in radians about `y`, from facing the camera.
    facing = 0,
    -- Seconds the player has been walking without stopping, for its waddle.
    walk_time = 0,
    crates = crates,
    coins = coins,
    found = 0,
    -- Whether every coin has been found.
    done = false,
    time = 0,
}

-- Where the player's feet are.
function course.feet(): vector
    return player:position_3d()
end

-- The view's camera, following the player.
function course.camera()
    local target = course.feet() + LOOK_ABOVE
    return { position = target + CAMERA_OFFSET, target = target }
end

-- Walks the player by the keys held, as fast diagonally as straight, falling unless it's on the
-- ground, and jumping off it.
local function walk(dt: number)
    local across = input.axis("left", "right")
    local along = input.axis("up", "down")
    local length = math.sqrt(across * across + along * along)
    if length > 1 then
        across /= length
        along /= length
    end
    if course.grounded and input.pressed("jump") then
        course.rising = JUMP
    end
    course.rising -= GRAVITY * dt
    local motion = vector.create(across * SPEED * dt, course.rising * dt, along * SPEED * dt)
    local moved, grounded = world:walk(player, motion)
    course.grounded = grounded
    if grounded or (course.rising > 0 and moved.y < motion.y / 2) then
        -- On the ground, or bumped its head.
        course.rising = 0
    end
    if across ~= 0 or along ~= 0 then
        -- Facing the way it walks, from facing `+z`, toward the camera.
        course.facing = exact.atan2(across, along)
        course.walk_time += dt
    else
        course.walk_time = 0
    end
    local waddle = if grounded then math.sin(course.walk_time * WADDLE_SPEED) * WADDLE else 0
    player:set_orientation(course.facing, 0, waddle)
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

-- Spins the coins and picks up those the player comes near.
local function collect()
    for i, coin in coins do
        if coin.found then
            continue
        end
        local bob = math.sin(course.time * 2 + i) * BOB
        coin.node:set_position_3d(coin.home + vector.create(0, bob, 0))
        coin.node:set_orientation(course.time * SPIN + i)
        if has_player(world:overlap_ball(coin.home, REACH)) then
            coin.found = true
            coin.node:set_visible(false)
            course.found += 1
            course.done = course.found == #coins
        end
    end
end

-- Starts the player again if it has fallen off the course, into `fall`, and takes away crates
-- that have.
local function catch_fallen()
    for _, node in world:inside(fall) do
        if node == player then
            player:set_position_3d(course.start)
            course.rising = 0
            continue
        end
        for i, crate in course.crates do
            if crate == node then
                crate:remove()
                table.remove(course.crates, i)
                break
            end
        end
    end
end

-- Puts the shadow on whatever is below the player, tilted as that is, or hides it over nothing.
local function cast_shadow()
    local hit = world:ray(course.feet() + vector.create(0, MIDDLE, 0), DOWN, SHADOW_RAY)
    shadow:set_visible(hit ~= nil)
    if hit then
        shadow:set_position_3d(hit.point + hit.normal * SHADOW_LIFT)
        -- Rolled, then pitched, so its up is the normal.
        local normal = hit.normal
        shadow:set_orientation(0, math.atan2(normal.z, normal.y), math.asin(-normal.x))
    end
end

function update(dt: number)
    course.time += dt
    walk(dt)
    world:step(dt)
    catch_fallen()
    collect()
    cast_shadow()
end

local font = graphics.default_font()

function draw()
    graphics.clear("#a8c8e8")
    view:set_camera(course.camera())
    graphics.draw_scene(level)

    -- The HUD, over the view.
    local width = graphics.size()
    graphics.fill_rect(0, 0, width, 14, "#00000080")
    graphics.draw_text(font, `Coins: {course.found}/{#coins}`, 4, 3, "#ffffff")
    local hint = "Arrows walk, Space jumps"
    graphics.draw_text(font, hint, width - font:measure(hint) - 4, 3, "#ffffff")
    if course.done then
        local message = "Every coin found!"
        local x, y = view:project(course.feet() + vector.create(0, 1, 0))
        if x and y then
            graphics.draw_text(font, message, x - font:measure(message) / 2, y - 10, "#ffd870")
        end
    end
end
