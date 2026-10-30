-- Pong against the computer, written entirely in Luau. Move your paddle, on the left, with W and
-- S or the arrow keys, or with the mouse, and press Space or click to serve. The first to 5
-- points wins.
--
-- The computer's play is in `opponent.luau`. Hits blip, higher as a rally speeds up, and shake
-- the field; walls thud and points chime, each panned to where it happened, and music loops in
-- the background.
--
-- Run with `cargo run -- examples/pong`.

local opponent = require("./opponent")
local window = require("./window")

local WIDTH = window.resolution.width
local HEIGHT = window.resolution.height

local MUSIC_VOLUME = 0.4
-- How far sounds pan toward the side of the field they happen on: 1 is all the way.
local STEREO_WIDTH = 0.7

local PADDLE_WIDTH = 4
local PADDLE_HEIGHT = 24
-- The gap between each paddle and its end of the field.
local PADDLE_MARGIN = 8
-- Pixels per second the player's paddle moves with the keys.
local PLAYER_SPEED = 160
-- Pixels per second the opponent's paddle moves: slower than the player's, to be fair.
local OPPONENT_SPEED = 110

local BALL_SIZE = 4
-- Pixels per second the ball moves when it's served.
local SERVE_SPEED = 120
-- Each hit speeds the ball up by this factor, up to `MAX_SPEED`.
local SPEEDUP = 1.08
local MAX_SPEED = 320
-- The steepest angle a paddle sends the ball back at, when it hits the paddle's very end.
local MAX_BOUNCE_ANGLE = math.pi / 3
-- The angles serves leave at, in turn, in radians from straight across. Positive is downward.
local SERVE_ANGLES = { 0.35, -0.2, 0.5, -0.45, 0.1 }
-- The furthest the ball moves between collision checks, so it can't pass through a paddle.
local MAX_STEP = 1

local WINNING_SCORE = 5
-- Seconds between a point and the next serve.
local SERVE_DELAY = 1
-- Seconds the field shakes after a hit, and how far it moves at first.
local SHAKE_SECONDS = 0.15
local SHAKE_PIXELS = 2
-- The dashes of the halfway line.
local DASH_LENGTH = 6
local DASH_SPACING = 12
local MESSAGE_PADDING = 8

local BACKGROUND = "#14182c"
local LINE = "#3a405c"
local PADDLE = "#f4f0e8"
local BALL = "#f2c14e"
local SCORE = "#8a92b8"
local PANEL = { r = 0, g = 0, b = 0, a = 0.85 }
local TEXT = "#f4f0e8"

type Side = "left" | "right"
-- `x` and `y` are the top-left corner.
type Paddle = { x: number, y: number }
-- `x` and `y` are the center; `vx` and `vy` are in pixels per second.
type Ball = { x: number, y: number, vx: number, vy: number }

local SIDES: { Side } = { "left", "right" }

local function opposite(side: Side): Side
    return if side == "left" then "right" else "left"
end

-- The way along `x` the ball moves to reach `side`: -1 or 1.
local function direction(side: Side): number
    return if side == "left" then -1 else 1
end

-- Halfway down `side` of the field.
local function new_paddle(side: Side): Paddle
    local x = if side == "left" then PADDLE_MARGIN else WIDTH - PADDLE_MARGIN - PADDLE_WIDTH
    return { x = x, y = (HEIGHT - PADDLE_HEIGHT) / 2 }
end

local function paddle_center(paddle: Paddle): number
    return paddle.y + PADDLE_HEIGHT / 2
end

-- Moves `paddle` down by `dy`, or up if it's negative, staying on the field.
local function move_paddle(paddle: Paddle, dy: number)
    paddle.y = math.clamp(paddle.y + dy, 0, HEIGHT - PADDLE_HEIGHT)
end

-- At rest in the middle of the field.
local function resting_ball(): Ball
    return { x = WIDTH / 2, y = HEIGHT / 2, vx = 0, vy = 0 }
end

local function speed(ball: Ball): number
    return math.sqrt(ball.vx * ball.vx + ball.vy * ball.vy)
end

-- Whether `ball` is hitting the paddle on `side`, moving toward it. A ball that got behind the
-- paddle is past saving.
local function hits(ball: Ball, paddle: Paddle, side: Side): boolean
    local left, top = ball.x - BALL_SIZE / 2, ball.y - BALL_SIZE / 2
    local in_front = if side == "left"
        then left >= paddle.x
        else left + BALL_SIZE <= paddle.x + PADDLE_WIDTH
    local toward = ball.vx * direction(side) > 0
    local overlaps = left < paddle.x + PADDLE_WIDTH
        and paddle.x < left + BALL_SIZE
        and top < paddle.y + PADDLE_HEIGHT
        and paddle.y < top + BALL_SIZE
    return toward and in_front and overlaps
end

-- Sends `ball` back from the paddle on `side`: faster, and steeper the further from the
-- paddle's middle it hit.
local function bounce_off(ball: Ball, paddle: Paddle, side: Side)
    local reach = (PADDLE_HEIGHT + BALL_SIZE) / 2
    local offset = math.clamp((ball.y - paddle_center(paddle)) / reach, -1, 1)
    local angle = offset * MAX_BOUNCE_ANGLE
    local new_speed = math.min(speed(ball) * SPEEDUP, MAX_SPEED)
    ball.vx = direction(opposite(side)) * math.cos(angle) * new_speed
    ball.vy = math.sin(angle) * new_speed
    -- In front of the paddle, so it doesn't hit again.
    ball.x = if side == "left"
        then paddle.x + PADDLE_WIDTH + BALL_SIZE / 2
        else paddle.x - BALL_SIZE / 2
end

-- Settings that play a sound panned toward `x` on the field.
local function panned(x: number)
    return { pan = (math.clamp(x / WIDTH, 0, 1) * 2 - 1) * STEREO_WIDTH }
end

input.bind("up", { "W", "Up" })
input.bind("down", { "S", "Down" })
input.bind("serve", { "Space", "Enter" })

local sounds = {
    hit = audio.load("sounds/blip.wav"),
    wall = audio.load("sounds/bump.wav"),
    point = audio.load("sounds/chime.wav"),
}
audio.set_music_volume(MUSIC_VOLUME)
audio.play_music("sounds/music.ogg", 1)

local font = graphics.default_font()
local headline_font = font:scaled(2)
local score_font = font:scaled(3)

-- Everything that changes as the game plays. It's global so that tests can see it.
pong = {
    -- The player's paddle.
    left = new_paddle("left"),
    -- The opponent's paddle, which `opponent.luau` steers.
    right = new_paddle("right"),
    ball = resting_ball(),
    score = { left = 0, right = 0 },
    -- "ready" until the first serve, "rally" while the ball is in play, "pause" from a point
    -- until the next serve, and "over" once a side has won.
    phase = "ready",
    -- Seconds left of a pause.
    pause_left = 0,
    -- The side that won, once the match is over.
    winner = nil :: Side?,
    -- The side the next serve goes toward. The first goes to the computer, so the player can
    -- see how it plays.
    serve_toward = "right" :: Side,
    -- Serves so far, which picks the angle of the next.
    serves = 0,
    -- Whether the player's paddle follows the cursor: from when the mouse moves until a key is
    -- held.
    mouse_control = false,
    -- The cursor's position the frame before, to tell when it moves.
    last_mouse = nil :: { x: number, y: number }?,
    -- Where the opponent is moving its paddle's center to.
    aim = HEIGHT / 2,
    -- Seconds left of the field shaking.
    shake = 0,
    time = 0,
}

spawn(opponent.play, pong, WIDTH, HEIGHT)

-- Puts the ball in play from the middle of the field, toward `pong.serve_toward`.
local function serve()
    local angle = SERVE_ANGLES[pong.serves % #SERVE_ANGLES + 1]
    pong.serves += 1
    local away = direction(pong.serve_toward)
    pong.ball = {
        x = WIDTH / 2,
        y = HEIGHT / 2,
        vx = away * math.cos(angle) * SERVE_SPEED,
        vy = math.sin(angle) * SERVE_SPEED,
    }
    pong.phase = "rally"
    pong.winner = nil
end

-- Scores a point for `scorer`. The next serve goes to the side that lost it.
local function point(scorer: Side)
    audio.play(sounds.point, panned(pong.ball.x))
    pong.score[scorer] += 1
    pong.ball = resting_ball()
    pong.serve_toward = opposite(scorer)
    if pong.score[scorer] >= WINNING_SCORE then
        pong.phase = "over"
        pong.winner = scorer
    else
        pong.phase = "pause"
        pong.pause_left = SERVE_DELAY
    end
end

-- Moves the player's paddle with the keys, or to the cursor once the mouse moves.
local function move_player(dt: number)
    local heading = 0
    if input.held("up") then
        heading -= 1
    end
    if input.held("down") then
        heading += 1
    end
    local x, y = input.mouse_position()
    local last = pong.last_mouse
    if heading ~= 0 then
        pong.mouse_control = false
    elseif x and (not last or last.x ~= x or last.y ~= y) then
        pong.mouse_control = true
    end
    pong.last_mouse = if x then { x = x, y = y } else nil
    if y and pong.mouse_control then
        move_paddle(pong.left, y - paddle_center(pong.left))
    else
        move_paddle(pong.left, heading * PLAYER_SPEED * dt)
    end
end

-- Moves the opponent's paddle toward where it aims, as fast as the paddle goes.
local function move_opponent(dt: number)
    local most = OPPONENT_SPEED * dt
    move_paddle(pong.right, math.clamp(pong.aim - paddle_center(pong.right), -most, most))
end

-- Moves the ball on by `dt` seconds, bouncing it off the walls and paddles. Returns the side
-- that scored, if it left the field.
local function advance_ball(dt: number): Side?
    local ball = pong.ball
    local half = BALL_SIZE / 2
    -- Short steps, so a fast ball can't pass through a paddle between frames.
    local steps = math.max(math.ceil(speed(ball) * dt / MAX_STEP), 1)
    local step = dt / steps
    for _ = 1, steps do
        ball.x += ball.vx * step
        ball.y += ball.vy * step
        if (ball.y - half < 0 and ball.vy < 0) or (ball.y + half > HEIGHT and ball.vy > 0) then
            ball.vy = -ball.vy
            audio.play(sounds.wall, panned(ball.x))
        end
        for _, side in SIDES do
            local paddle = pong[side]
            if hits(ball, paddle, side) then
                bounce_off(ball, paddle, side)
                pong.shake = SHAKE_SECONDS
                local settings = panned(paddle.x)
                -- Higher as the rally speeds up.
                settings.speed = math.sqrt(speed(ball) / SERVE_SPEED)
                audio.play(sounds.hit, settings)
            end
        end
        if ball.x + half < 0 then
            return "right"
        end
        if ball.x - half > WIDTH then
            return "left"
        end
    end
    return nil
end

function update(dt: number)
    pong.time += dt
    pong.shake = math.max(pong.shake - dt, 0)
    move_opponent(dt)
    move_player(dt)

    local serving = input.pressed("serve") or input.mouse_pressed("left")
    if pong.phase == "ready" and serving then
        serve()
    elseif pong.phase == "over" and serving then
        pong.score = { left = 0, right = 0 }
        serve()
    elseif pong.phase == "pause" then
        if pong.pause_left > dt then
            pong.pause_left -= dt
        else
            serve()
        end
    elseif pong.phase == "rally" then
        local scorer = advance_ball(dt)
        if scorer then
            point(scorer)
        end
    end
end

-- The headline and prompt to show over the field, if any.
local function message(): (string?, string?)
    if pong.phase == "ready" then
        return "PONG", "Space or click to serve"
    elseif pong.phase == "over" then
        local headline = if pong.winner == "left" then "You win!" else "The computer wins"
        return headline, "Space or click to play again"
    end
    return nil, nil
end

-- Draws `text` centered on `x`, with its top at `y`.
local function draw_centered(text_font, text: string, x: number, y: number, color)
    local width = text_font:measure(text)
    graphics.draw_text(text_font, text, x - width / 2, y, color)
end

function draw()
    graphics.clear(BACKGROUND)
    -- Hits shake the field, but not the score and messages drawn over it.
    local strength = SHAKE_PIXELS * pong.shake / SHAKE_SECONDS
    graphics.set_camera(
        WIDTH / 2 + math.sin(pong.time * 97) * strength,
        HEIGHT / 2 + math.cos(pong.time * 61) * strength
    )
    for i = 0, math.ceil(HEIGHT / DASH_SPACING) - 1 do
        local y = i * DASH_SPACING + (DASH_SPACING - DASH_LENGTH) / 2
        graphics.fill_rect(WIDTH / 2 - 1, y, 2, DASH_LENGTH, LINE)
    end
    for _, side in SIDES do
        local paddle = pong[side]
        graphics.fill_rect(paddle.x, paddle.y, PADDLE_WIDTH, PADDLE_HEIGHT, PADDLE)
    end
    -- Between matches, the message takes the ball's place.
    if pong.phase == "rally" or pong.phase == "pause" then
        local half = BALL_SIZE / 2
        graphics.fill_rect(pong.ball.x - half, pong.ball.y - half, BALL_SIZE, BALL_SIZE, BALL)
    end

    graphics.set_camera(nil)
    draw_centered(score_font, tostring(pong.score.left), WIDTH / 4, 8, SCORE)
    draw_centered(score_font, tostring(pong.score.right), WIDTH * 0.75, 8, SCORE)
    local headline, prompt = message()
    if headline and prompt then
        local headline_width, headline_height = headline_font:measure(headline)
        local prompt_width, prompt_height = font:measure(prompt)
        local width = math.max(headline_width, prompt_width) + MESSAGE_PADDING * 2
        local height = headline_height + prompt_height + MESSAGE_PADDING * 2
        local x, y = math.round((WIDTH - width) / 2), math.round((HEIGHT - height) / 2)
        graphics.fill_rect(x, y, width, height, PANEL)
        local top = y + MESSAGE_PADDING
        draw_centered(headline_font, headline, WIDTH / 2, top, TEXT)
        draw_centered(font, prompt, WIDTH / 2, top + headline_height, TEXT)
    end
end
