-- Pong's tests, which `cargo run -- test examples/pong` runs, each against a fresh game with no
-- frames run yet. They read and change the game's state, the global `pong`, as it is.

-- Puts the ball in play, centered on `x`, `y` and moving at `vx`, `vy`.
local function play_ball(x: number, y: number, vx: number, vy: number)
    pong.ball = { x = x, y = y, vx = vx, vy = vy }
    pong.phase = "rally"
end

-- The middle of the player's paddle, which is 24 pixels tall.
local function player_center(): number
    return pong.left.y + 12
end

local function rally_over(): boolean
    return pong.phase ~= "rally"
end

test("the ball waits for a serve", function()
    sim.run_for(1)
    assert_equal(pong.phase, "ready")
    assert_equal(pong.ball, { x = 160, y = 90, vx = 0, vy = 0 })

    sim.tap("Space")
    assert_equal(pong.phase, "rally")
    assert(pong.ball.vx > 0, "the first serve goes to the computer")
end)

test("the keys move the paddle until it reaches a wall", function()
    sim.hold("W", 2)
    assert_equal(pong.left.y, 0)
    -- 160 pixels a second.
    sim.hold("Down", 0.5)
    assert_near(pong.left.y, 80, 1e-3)
end)

test("the paddle follows the mouse", function()
    sim.move_mouse(100, 40)
    sim.step()
    assert_equal(player_center(), 40)
    sim.click(100, 40)
    assert_equal(pong.phase, "rally", "clicking serves")
end)

test("missing the ball scores for the other side", function()
    -- Above the player's paddle, which waits in the middle.
    play_ball(100, 20, -200, 0)
    wait_until(rally_over)
    assert_equal(pong.score, { left = 0, right = 1 })
    assert_equal(pong.phase, "pause")

    -- The next serve goes to the player, who lost the point, a second later.
    wait(1.1)
    assert_equal(pong.phase, "rally")
    assert(pong.ball.vx < 0, "toward the player")
end)

test("the first to five wins", function()
    pong.score = { left = 2, right = 4 }
    play_ball(100, 20, -200, 0)
    wait_until(rally_over)
    assert_equal(pong.phase, "over")
    assert_equal(pong.winner, "right")

    -- It stays over until the player serves again.
    sim.run_for(2)
    assert_equal(pong.phase, "over")
    sim.tap("Enter")
    assert_equal(pong.phase, "rally")
    assert_equal(pong.score, { left = 0, right = 0 })
end)

test("F11 covers the screen and stops covering it", function()
    assert_equal(window.fullscreen(), nil)
    sim.tap("F11")
    assert_equal(window.fullscreen(), "borderless")
    sim.tap("F11")
    assert_equal(window.fullscreen(), nil)
end)

test("the court before the first serve", function()
    sim.step()
    assert(sim.matches_screenshot("screenshots/ready.png"))
end)
