--!native
-- Enemy bullets, written in Luau as well as Luau allows: each of a bullet's numbers lives in an
-- array of its own, which are kept from frame to frame rather than reallocated, and the module
-- asks Luau to compile it to native code. With a table for each bullet instead, the stress test
-- reached its limit at not much more than a third as many bullets: tens of thousands of tables
-- don't fit in the CPU's cache, where arrays of numbers do far better.
--
-- The rest of the game reaches bullets only through this interface, which the `danmaku` native
-- library implements too, so either can run them. Both do the same arithmetic in the same order,
-- so from the same calls they produce the same bullets, to the bit: change one, and change the
-- other to match. A bullet moves by its parameters alone, so moving one never calls back into the
-- game:
--
-- - `system.kind(sprite, radius, rotates)` registers a look for bullets and returns its number,
--   counting from 1 in the order they're registered. A bullet of it is drawn as `sprite`,
--   centered on the bullet, turned to the way it's heading if `rotates`, and hits within
--   `radius` of its center.
-- - `system.set_bounds(x, y, width, height, margin)` sets the area bullets live in: one more than
--   `margin` outside it is removed.
-- - `system.spawn(kind, x, y, angle, speed, count, spacing, acceleration, turn, min_speed,
--   max_speed)` spawns `count` bullets (1 by default) at `x`, `y`, heading at angles `spacing`
--   radians apart, centered on `angle`, at `speed` pixels a second. Angles are radians clockwise
--   from the right, since `y` grows downward. Each second, a bullet's speed changes by
--   `acceleration`, staying from `min_speed` to `max_speed`, and its angle by `turn`. Leaving out
--   the rest means 0, apart from the speed limits, which then don't limit. It fails for a kind
--   that isn't registered, or a `min_speed` above `max_speed`.
-- - `system.step(dt)` moves every bullet on by `dt` seconds: first its speed changes, then its
--   angle, then it moves by both. Bullets that end up out of bounds are removed: each removed
--   bullet's place goes to the last bullet, which is moved next.
-- - `system.collide(x, y, radius, graze_radius)` returns whether a bullet touches the circle at
--   `x`, `y` of `radius`, and how many bullets graze it for the first time: come within
--   `graze_radius` without touching. A bullet grazes once.
-- - `system.clear()` removes every bullet and returns how many there were.
-- - `system.count()` returns how many bullets there are.
-- - `system.draw()` draws every bullet, in their order.
-- - `system.positions()` returns a list of every bullet's `x` and `y`, for tests.

local bullets = {}

type Kind = {
    sprite: any,
    -- Half the sprite's size, to center it on the bullet.
    half_width: number,
    half_height: number,
    radius: number,
    rotates: boolean,
}

local kinds: { Kind } = {}

-- Bullet `i`'s numbers are element `i` of each array, for `i` from 1 to `count`. What's after is
-- left from removed bullets, and overwritten as bullets spawn.
local count = 0
local xs: { number } = {}
local ys: { number } = {}
local angles: { number } = {}
local speeds: { number } = {}
local accelerations: { number } = {}
local turns: { number } = {}
local min_speeds: { number } = {}
local max_speeds: { number } = {}
local radii: { number } = {}
local looks: { Kind } = {}
local grazed: { boolean } = {}

local left, top, right, bottom = -math.huge, -math.huge, math.huge, math.huge

function bullets.kind(sprite, radius: number, rotates: boolean?): number
    local width, height = sprite:size()
    table.insert(kinds, {
        sprite = sprite,
        half_width = width / 2,
        half_height = height / 2,
        radius = radius,
        rotates = rotates == true,
    })
    return #kinds
end

function bullets.set_bounds(x: number, y: number, width: number, height: number, margin: number)
    left, top = x - margin, y - margin
    right, bottom = x + width + margin, y + height + margin
end

function bullets.spawn(
    kind: number,
    x: number,
    y: number,
    angle: number,
    speed: number,
    spawned: number?,
    spacing: number?,
    acceleration: number?,
    turn: number?,
    min_speed: number?,
    max_speed: number?
)
    local look = kinds[kind]
    if not look then
        error(`there's no bullet kind {kind}`, 2)
    end
    local n = spawned or 1
    local step = spacing or 0
    local first = angle - step * (n - 1) / 2
    local a, t = acceleration or 0, turn or 0
    local low, high = min_speed or -math.huge, max_speed or math.huge
    if low > high then
        error("min_speed is more than max_speed", 2)
    end
    for i = 0, n - 1 do
        count += 1
        xs[count] = x
        ys[count] = y
        angles[count] = first + step * i
        speeds[count] = speed
        accelerations[count] = a
        turns[count] = t
        min_speeds[count] = low
        max_speeds[count] = high
        radii[count] = look.radius
        looks[count] = look
        grazed[count] = false
    end
end

function bullets.step(dt: number)
    local i = 1
    while i <= count do
        local speed = math.clamp(speeds[i] + accelerations[i] * dt, min_speeds[i], max_speeds[i])
        local angle = angles[i] + turns[i] * dt
        local x = xs[i] + math.cos(angle) * speed * dt
        local y = ys[i] + math.sin(angle) * speed * dt
        if x < left or x > right or y < top or y > bottom then
            -- The last bullet takes its place, and is moved next.
            local last = count
            xs[i] = xs[last]
            ys[i] = ys[last]
            angles[i] = angles[last]
            speeds[i] = speeds[last]
            accelerations[i] = accelerations[last]
            turns[i] = turns[last]
            min_speeds[i] = min_speeds[last]
            max_speeds[i] = max_speeds[last]
            radii[i] = radii[last]
            looks[i] = looks[last]
            grazed[i] = grazed[last]
            count -= 1
        else
            speeds[i] = speed
            angles[i] = angle
            xs[i] = x
            ys[i] = y
            i += 1
        end
    end
end

function bullets.collide(x: number, y: number, radius: number, graze_radius: number): (boolean, number)
    local hit, grazes = false, 0
    for i = 1, count do
        local dx, dy = xs[i] - x, ys[i] - y
        local distance = dx * dx + dy * dy
        local reach = graze_radius + radii[i]
        if distance < reach * reach then
            local touch = radius + radii[i]
            if distance < touch * touch then
                hit = true
            elseif not grazed[i] then
                grazed[i] = true
                grazes += 1
            end
        end
    end
    return hit, grazes
end

function bullets.clear(): number
    local cleared = count
    count = 0
    return cleared
end

function bullets.count(): number
    return count
end

local draw_sprite = graphics.draw_sprite
local draw_sprite_rotated = graphics.draw_sprite_rotated

function bullets.draw()
    for i = 1, count do
        local look = looks[i]
        local x, y = xs[i] - look.half_width, ys[i] - look.half_height
        if look.rotates then
            draw_sprite_rotated(look.sprite, x, y, angles[i])
        else
            draw_sprite(look.sprite, x, y)
        end
    end
end

function bullets.positions(): { number }
    local positions = table.create(count * 2)
    for i = 1, count do
        positions[i * 2 - 1] = xs[i]
        positions[i * 2] = ys[i]
    end
    return positions
end

return bullets
