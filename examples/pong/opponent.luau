-- The computer opponent, which plays the right paddle. `main.luau` runs `opponent.play` as a
-- task from the start.

-- Seconds between looks at the ball. The longer, the easier the opponent is to beat.
local REACTION = 0.12
-- How far from the paddle's middle it meets the ball, to send it back at an angle.
local SPIN = 6

local opponent = {}

-- Where the ball's center will be when it reaches `x`, counting bounces off the top and bottom
-- of a field `height` tall. It ignores the ball's size, which is close enough for a paddle.
local function crossing(ball, x: number, height: number): number
    local y = ball.y + ball.vy * (x - ball.x) / ball.vx
    -- Each bounce folds the straight path back onto the field.
    y %= 2 * height
    if y > height then
        y = 2 * height - y
    end
    return y
end

-- Plays the right paddle of `pong`, the game's state, on a field `width` by `height`. A few
-- times a second it looks at the ball and sets `pong.aim`, where the game moves the paddle's
-- center to.
function opponent.play(pong, width: number, height: number)
    while true do
        local ball = pong.ball
        if ball.vx > 0 and ball.x > width / 2 then
            -- The ball is coming, past the halfway line. Meeting it off-center sends it back
            -- the way it was going, up or down.
            pong.aim = crossing(ball, pong.right.x, height) - math.sign(ball.vy) * SPIN
        else
            pong.aim = height / 2
        end
        wait(REACTION)
    end
end

return opponent
