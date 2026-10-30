-- The computer opponent in the pong example, which plays the right paddle. The game runs
-- `opponent` as a task from the start.
--
-- Besides the engine's built-ins (`wait`, `wait_until`, `spawn` and `print`), the game gives
-- scripts these functions:
--
--   ball()    the ball's center and velocity: x, y, vx, vy, in pixels and pixels per second
--   paddle()  the opponent's paddle: the x of its front edge, and the y of its center
--   field()   the field's width and height
--   aim(y)    moves the paddle's center toward y, as fast as the paddle goes

-- Seconds between looks at the ball. The longer, the easier the opponent is to beat.
local REACTION = 0.12
-- How far from the paddle's middle it meets the ball, to send it back at an angle.
local SPIN = 6

-- Where the ball's center will be when it reaches `x`, counting bounces off the top and bottom.
-- It ignores the ball's size, which is close enough for a paddle.
local function crossing(x: number): number
    local bx, by, vx, vy = ball()
    local _, height = field()
    local y = by + vy * (x - bx) / vx
    -- Each bounce folds the straight path back onto the field.
    y %= 2 * height
    if y > height then
        y = 2 * height - y
    end
    return y
end

function opponent()
    while true do
        local x, _, vx, vy = ball()
        local width, height = field()
        if vx > 0 and x > width / 2 then
            -- The ball is coming, past the halfway line. Meeting it off-center sends it back
            -- the way it was going, up or down.
            local front = paddle()
            aim(crossing(front) - math.sign(vy) * SPIN)
        else
            aim(height / 2)
        end
        wait(REACTION)
    end
end
