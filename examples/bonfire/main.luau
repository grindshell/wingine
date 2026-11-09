-- Bonfire: a night by a fire, laid out in `bonfire.scene.luau`, to show blend modes and
-- particles. Space puts the fire out and lights it again, and a click in the sky sends up a
-- firework.
--
-- - The scene's particles nodes are its effects: stars twinkling, smoke drifting, flames and
--   sparks rising from the fire, and fireflies over the meadow. Scenes keep no time of their own,
--   so `update` advances them.
-- - A rect multiplies everything under it by the blue of the night, and the fire's glow and the
--   moon's are screened over it, lightening what's around them. Flames, sparks, stars and
--   fireflies add their light to what's under them.
-- - The fire lights the camp as brightly as it burns: its glow's tint follows how many flames
--   are alive.
-- - Fireworks are emitters the script makes with `graphics.new_particles`: a rocket's trail,
--   moved as it climbs, which leaves its sparks behind, then a burst where it was sent, in each of
--   the colors in turn.
--
-- Run with `cargo run -- examples/bonfire`. The tent, trees and logs are Kenney's 1-bit Pack
-- (<https://kenney.nl/assets/1-bit-pack>, CC0); `bonfire.png` was drawn for the example. The
-- game's state is in the global table `bonfire`, where tests can see it.

input.bind("fire", { "Space" })

-- Where the meadow starts: clicks above it send fireworks up.
local GROUND = 136
-- How fast rockets climb, in pixels a second.
local CLIMB = 150
-- How many flames are alive while the fire burns as it does, for its glow.
local FULL_FIRE = 30
-- The color of the fire's glow, at full strength.
local GLOW = graphics.color("#ff9a50")
-- Fireworks' colors, one after another.
local COLORS = { "#ff6080", "#60c0ff", "#ffd040", "#80ff90" }

-- What a rocket leaves behind it as it climbs.
local TRAIL = {
    rate = 60,
    lifetime = { 0.3, 0.6 },
    speed = { 0, 6 },
    colors = { "#ffe0a0", "#ff602000" },
    blend = "add",
}

-- The sparks a rocket bursts into, in its color.
local function burst_settings(color: string)
    return {
        rate = 0,
        lifetime = { 0.8, 1.4 },
        speed = { 30, 70 },
        gravity = { 0, 40 },
        drag = 1.2,
        colors = { "#ffffff", color, color .. "00" },
        blend = "add",
    }
end

type Rocket = {
    trail: Particles,
    burst: Particles,
    x: number,
    y: number,
    -- Where it bursts.
    top: number,
    burst_yet: boolean,
}

bonfire = {}

local level = scene.load("bonfire.scene.luau")
bonfire.scene = level
bonfire.flames = level:find("camp/flames")
bonfire.sparks = level:find("camp/sparks")
bonfire.smoke = level:find("camp/smoke")
bonfire.glow = level:find("camp/glow")
bonfire.lit = true
-- Rockets in the air, and those whose sparks haven't all faded.
bonfire.rockets = {} :: { Rocket }
-- How many rockets have gone up, which picks the next one's color.
bonfire.launched = 0

-- Sends a rocket up from the meadow below `x` to burst at `y`.
local function launch(x: number, y: number)
    bonfire.launched += 1
    local color = COLORS[(bonfire.launched - 1) % #COLORS + 1]
    local trail = graphics.new_particles(TRAIL)
    trail:set_position(x, GROUND + 8)
    table.insert(bonfire.rockets, {
        trail = trail,
        burst = graphics.new_particles(burst_settings(color)),
        x = x,
        y = GROUND + 8,
        top = y,
        burst_yet = false,
    })
end

-- Climbs each rocket, bursts those that have arrived, and lets go of those whose sparks are gone.
local function fly(dt: number)
    for i = #bonfire.rockets, 1, -1 do
        local rocket = bonfire.rockets[i]
        if not rocket.burst_yet then
            rocket.y = math.max(rocket.y - CLIMB * dt, rocket.top)
            rocket.trail:set_position(rocket.x, rocket.y)
            if rocket.y <= rocket.top then
                rocket.burst_yet = true
                rocket.trail:set_emitting(false)
                rocket.burst:set_position(rocket.x, rocket.top)
                rocket.burst:emit(60)
            end
        end
        rocket.trail:advance(dt)
        rocket.burst:advance(dt)
        if rocket.burst_yet and rocket.trail:count() == 0 and rocket.burst:count() == 0 then
            table.remove(bonfire.rockets, i)
        end
    end
end

function update(dt: number)
    if input.pressed("fire") then
        bonfire.lit = not bonfire.lit
        bonfire.flames:set_emitting(bonfire.lit)
        bonfire.sparks:set_emitting(bonfire.lit)
    end
    if input.mouse_pressed("left") then
        local x, y = input.mouse_position()
        if x and y and y >= 0 and y < GROUND then
            launch(x, y)
        end
    end
    bonfire.scene:advance_particles(dt)
    fly(dt)
    local burning = math.min(bonfire.flames:particle_count() / FULL_FIRE, 1)
    bonfire.glow:set_tint(graphics.color(GLOW, burning))
end

function draw()
    graphics.clear("#000000")
    graphics.draw_scene(bonfire.scene)
    for _, rocket in bonfire.rockets do
        graphics.draw_particles(rocket.trail)
        graphics.draw_particles(rocket.burst)
    end
end
