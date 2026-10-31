-- Ways enemies fire bullets, through whichever bullet system is running.

local patterns = {}

local TAU = 2 * math.pi

-- The angle from `x`, `y` to the player.
function patterns.aim(x: number, y: number): number
    local player = shmup.player
    return math.atan2(player.y - y, player.x - x)
end

-- `count` bullets of `kind` in a ring around `x`, `y`, one heading at `angle`. The rest are the
-- motion they share, as `system.spawn` takes it.
function patterns.ring(
    kind: number,
    x: number,
    y: number,
    count: number,
    angle: number,
    speed: number,
    acceleration: number?,
    turn: number?,
    min_speed: number?,
    max_speed: number?
)
    shmup.bullets.spawn(
        kind,
        x,
        y,
        angle,
        speed,
        count,
        TAU / count,
        acceleration,
        turn,
        min_speed,
        max_speed
    )
end

-- `count` bullets of `kind` fanned out over `arc` radians, centered on the player.
function patterns.aimed(
    kind: number,
    x: number,
    y: number,
    count: number,
    arc: number,
    speed: number,
    acceleration: number?,
    min_speed: number?,
    max_speed: number?
)
    local spacing = if count > 1 then arc / (count - 1) else 0
    local angle = patterns.aim(x, y)
    shmup.bullets.spawn(
        kind,
        x,
        y,
        angle,
        speed,
        count,
        spacing,
        acceleration,
        0,
        min_speed,
        max_speed
    )
end

-- A random number from `low` to `high`.
function patterns.random(low: number, high: number): number
    return low + math.random() * (high - low)
end

return patterns
