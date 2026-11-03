-- An orchard where apples ripen and fall, and a picker with a basket on their head who catches
-- them. Walk with the arrow keys or A and D; three apples on the ground and the day is over.
--
-- The picker is a rig, in `picker.scene.luau`: sprites of their head, body, arms and legs, each
-- turning about its joint, its pivot, inside the part it hangs from. Their animations are clips
-- in the same file, keyframes of the parts' positions, turns and stretches, of which sprite the
-- face shows and so on, made to be written by a tool as much as by hand. Like sprite animations,
-- clips don't keep time: this script keeps how long each has played, and poses the picker with
-- them each frame, one over another:
--
-- - `idle`, standing and breathing, has keys for everything the other clips move, so posing with
--   it first puts every part back before the rest have their say.
-- - `walk` is weighted by how much the picker is walking, easing in and out, so it never snaps
--   between standing and walking. Its time only passes while the picker walks, so they don't
--   keep stepping as they stop.
-- - `blink` changes only the face's sprite, now and then.
-- - `catch` and `miss` are reactions, which play once, faded in and out; `slump` is the end of
--   the day, which holds its last pose.
--
-- The orchard, `orchard.scene.luau`, places the picker as an instance of their file, so these
-- clips, whose paths are from the top of that file, pose the picker from the instance's node.
-- The picker faces the way they walk by mirroring that node; the clips don't need to know.
-- Apples are copies of a hidden node, and the catching is done against an area in the basket,
-- `mouth`, whose bounds move as the picker is posed.
--
-- The orchard is drawn through a camera, and the HUD over it without one. Once the day is over,
-- the camera eases in on the picker, ending at a zoom of 2, a whole number, so the pixel art is
-- even again once it gets there.
--
-- Run with `cargo run -- examples/orchard`. The art was drawn pixel by pixel for it. The game's
-- state is in the global table `orchard`, where tests can see it.

local orchard_scene = scene.load("orchard.scene.luau")
local clips = scene.clips("picker.scene.luau")

input.bind("left", { "Left", "A" })
input.bind("right", { "Right", "D" })
input.bind("again", { "Space", "Enter" })

-- Pixels a second the picker walks, and how far it goes either way.
local SPEED = 80
local LEFT, RIGHT = 16, 304
-- Seconds to ease between standing and walking, and in and out of a reaction.
local STRIDE_FADE = 0.15
local REACTION_FADE = 0.08
-- Seconds an apple grows on the tree, then shakes before it falls, and lies on the ground.
local GROW, SHAKE, ROT = 1.2, 0.5, 1.5
-- Pixels a second, every second, that apples speed up as they fall.
local GRAVITY = 240
-- How far below an apple's middle its bottom is, and where it lands.
local APPLE_RADIUS = 3.5
local GROUND = 166
-- Apples missed before the day is over.
local LIVES = 3
-- Seconds between apples, at first and at the quickest, and how much quicker each catch makes
-- them.
local FIRST_INTERVAL, LAST_INTERVAL, QUICKER = 2, 0.7, 0.05
-- The cells of an apple and a bruised one.
local APPLE, BRUISED = { 5, 0 }, { 6, 0 }
-- The orchard, which the camera stays inside: the canvas, which the scene is laid out on.
local ORCHARD = { x = 0, y = 0, width = 320, height = 180 }
-- How far the camera zooms in on the picker once the day is over, in seconds, and how far above
-- their feet it looks.
local CLOSE_UP, CLOSE_UP_TIME, CLOSE_UP_ABOVE = 2, 1, 24

local picker = orchard_scene:find("picker")
local canopy = orchard_scene:find("canopy"):bounds()

type Apple = {
    node: any,
    x: number,
    y: number,
    -- Pixels a second down.
    speed: number,
    -- Seconds since it began to grow.
    age: number,
    -- "growing", "falling" or "fallen".
    state: string,
}

orchard = {
    scene = orchard_scene,
    picker = picker,
    clips = clips,
    mouth = picker:find("hips/body/head/basket/mouth"),
    apples = {} :: { Apple },
    -- Apples made, for naming their nodes.
    made = 0,
    score = 0,
    best = 0,
    misses = 0,
    -- Seconds the game has run, which the idle and blink clips play by.
    time = 0,
    -- Seconds the picker has walked, which the walk plays by, and how much it's walking, from 0
    -- (standing) to 1, which weights the walk.
    walk_time = 0,
    stride = 0,
    -- The reaction playing, if any, and when it started.
    reaction = nil :: string?,
    reaction_at = 0,
    -- When the day ended, if it has.
    over_at = nil :: number?,
    -- How far the camera has gone from the whole orchard (0) to its close-up of the picker at the
    -- end of the day (1), and where it looks and its zoom.
    close_up = 0,
    camera = { x = 160, y = 90, zoom = 1 },
    -- Seconds until the next apple, and between apples. Tests turn `growing` off and grow apples
    -- where they like.
    growing = true,
    next_apple = 1,
    interval = FIRST_INTERVAL,
}

local hud = {
    apples = orchard_scene:find("hud/score/apples"),
    best = orchard_scene:find("hud/score/best"),
    lives = orchard_scene:find("hud/lives"),
    hint = orchard_scene:find("hud/hint"),
    banner = orchard_scene:find("hud/banner"),
    detail = orchard_scene:find("hud/banner/detail"),
}
local template = orchard_scene:find("templates/apple")
local apple_nodes = orchard_scene:find("apples")
local clouds = orchard_scene:find("clouds"):children()
-- What the camera shows: everything but the HUD.
local hud_node = orchard_scene:find("hud")
local world = {}
for _, node in orchard_scene:children() do
    if node ~= hud_node then
        table.insert(world, node)
    end
end

-- Starts an apple growing at `x` across the canopy, and `y` down it, or halfway down.
function orchard.grow(x: number, y: number?): Apple
    orchard.made += 1
    local node = apple_nodes:add(template, `{orchard.made}`)
    local apple = {
        node = node,
        x = x,
        y = y or canopy.y + canopy.height / 2,
        speed = 0,
        age = 0,
        state = "growing",
    }
    node:set_position(apple.x, apple.y)
    node:set_scale(0.2)
    table.insert(orchard.apples, apple)
    return apple
end

local function react(clip: string)
    orchard.reaction = clip
    orchard.reaction_at = orchard.time
end

local function show_score()
    hud.apples:set_text(`Apples {orchard.score}`)
    hud.best:set_text(`Best {orchard.best}`)
end

local function catch(apple: Apple)
    apple.node:remove()
    orchard.score += 1
    orchard.best = math.max(orchard.best, orchard.score)
    orchard.interval = math.max(LAST_INTERVAL, orchard.interval - QUICKER)
    show_score()
    hud.hint:set_visible(false)
    react("catch")
end

local function land(apple: Apple)
    apple.state = "fallen"
    apple.age = 0
    apple.y = GROUND - APPLE_RADIUS
    apple.node:set_position(apple.x, apple.y)
    apple.node:set_rotation(0)
    apple.node:set_cell(BRUISED[1], BRUISED[2])
    if orchard.over_at then
        return
    end
    orchard.misses += 1
    hud.lives:find(`{orchard.misses}`):set_cell(BRUISED[1], BRUISED[2])
    if orchard.misses >= LIVES then
        orchard.over_at = orchard.time
        orchard.reaction = nil
        hud.detail:set_text(`You caught {orchard.score} apples.`)
        hud.banner:set_visible(true)
    else
        react("miss")
    end
end

-- Grows, shakes, drops and catches the apples, and clears away those on the ground.
local function update_apples(dt: number)
    local mouth = orchard.mouth:bounds()
    local i = 1
    while i <= #orchard.apples do
        local apple = orchard.apples[i]
        apple.age += dt
        local gone = false
        if apple.state == "growing" then
            local grown = math.min(1, apple.age / GROW)
            apple.node:set_scale(0.2 + 0.8 * grown)
            if apple.age >= GROW then
                apple.node:set_rotation(math.sin((apple.age - GROW) * 40) * 0.3)
            end
            if apple.age >= GROW + SHAKE then
                apple.state = "falling"
                apple.node:set_rotation(0)
            end
        elseif apple.state == "falling" then
            local bottom = apple.y + APPLE_RADIUS
            apple.speed += GRAVITY * dt
            apple.y += apple.speed * dt
            apple.node:set_position(apple.x, apple.y)
            -- Into the basket's mouth from above, between this frame and the last.
            local now = apple.y + APPLE_RADIUS
            local over = apple.x >= mouth.x and apple.x <= mouth.x + mouth.width
            if over and now >= mouth.y and bottom <= mouth.y + mouth.height then
                catch(apple)
                gone = true
            elseif now >= GROUND then
                land(apple)
            end
        elseif apple.age >= ROT then
            apple.node:remove()
            gone = true
        end
        if gone then
            table.remove(orchard.apples, i)
        else
            i += 1
        end
    end
    if orchard.growing and not orchard.over_at then
        orchard.next_apple -= dt
        if orchard.next_apple <= 0 then
            orchard.next_apple += orchard.interval
            orchard.grow(canopy.x + math.random() * canopy.width)
        end
    end
end

-- Walks the picker by the keys held, unless the day is over.
local function walk(dt: number)
    local direction = 0
    if not orchard.over_at then
        if input.held("left") then
            direction -= 1
        end
        if input.held("right") then
            direction += 1
        end
    end
    if direction ~= 0 then
        picker:set_scale(direction, 1)
    end
    local x, y = picker:position()
    local to = math.clamp(x + direction * SPEED * dt, LEFT, RIGHT)
    local moving = to ~= x
    if moving then
        picker:set_position(to, y)
        orchard.walk_time += dt
    end
    local step = dt / STRIDE_FADE
    orchard.stride = math.clamp(orchard.stride + (if moving then step else -step), 0, 1)
end

-- How much a reaction `time` seconds into a clip `duration` long counts: fading in, then out.
local function fade(time: number, duration: number): number
    return math.clamp(math.min(time, duration - time) / REACTION_FADE, 0, 1)
end

-- Poses the picker with its clips, one over another.
local function pose()
    picker:pose(clips.idle, orchard.time)
    if orchard.stride > 0 then
        picker:pose(clips.walk, orchard.walk_time, orchard.stride)
    end
    picker:pose(clips.blink, orchard.time)
    if orchard.reaction then
        local clip = clips[orchard.reaction]
        local time = orchard.time - orchard.reaction_at
        if clip:is_finished(time) then
            orchard.reaction = nil
        else
            picker:pose(clip, time, fade(time, clip:duration()))
        end
    end
    if orchard.over_at then
        local time = orchard.time - orchard.over_at
        picker:pose(clips.slump, time, math.min(1, time / REACTION_FADE))
    end
end

-- Starts the day again.
local function again()
    for _, apple in orchard.apples do
        apple.node:remove()
    end
    table.clear(orchard.apples)
    orchard.score = 0
    orchard.misses = 0
    orchard.over_at = nil
    orchard.reaction = nil
    orchard.interval = FIRST_INTERVAL
    orchard.next_apple = 1
    for _, life in hud.lives:children() do
        life:set_cell(APPLE[1], APPLE[2])
    end
    hud.banner:set_visible(false)
    show_score()
end

-- Drifts the clouds to the right, round again once they're past the edge.
local function drift(dt: number)
    for _, cloud in clouds do
        local x, y = cloud:position()
        x += 4 * dt
        if x > 320 then
            x -= 360
        end
        cloud:set_position(x, y)
    end
end

-- Eases the camera in on the picker once the day is over, and back out to the whole orchard for
-- the next.
local function frame(dt: number)
    local step = if orchard.over_at then dt / CLOSE_UP_TIME else -dt / CLOSE_UP_TIME
    orchard.close_up = math.clamp(orchard.close_up + step, 0, 1)
    local t = orchard.close_up
    local eased = t * t * (3 - 2 * t)
    local x, y = picker:position()
    y -= CLOSE_UP_ABOVE
    local camera = orchard.camera
    camera.x = 160 + (x - 160) * eased
    camera.y = 90 + (y - 90) * eased
    camera.zoom = 1 + (CLOSE_UP - 1) * eased
end

function update(dt: number)
    orchard.time += dt
    if orchard.over_at and input.pressed("again") then
        again()
    end
    drift(dt)
    walk(dt)
    pose()
    update_apples(dt)
    frame(dt)
end

function draw()
    graphics.clear("#000000")
    local camera = orchard.camera
    graphics.set_camera(camera.x, camera.y, ORCHARD, camera.zoom)
    for _, node in world do
        graphics.draw_scene(node)
    end
    graphics.set_camera(nil)
    graphics.draw_scene(hud_node)
end
