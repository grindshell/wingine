-- The bonfire's tests, which `cargo run -- test examples/bonfire` runs, each against a fresh game
-- with no frames run yet. They read and change its state, the global `bonfire`, as it is.

test("the night starts as its scene lays it out", function()
    -- GPUs round blending a little differently, which adds up where the fire's flames stack
    -- dozens of layers, so the frame is compared without its particles. Hidden, they still burn,
    -- so the fire's glow is as bright.
    for _, path in { "stars", "camp/smoke", "camp/flames", "camp/sparks", "fireflies" } do
        bonfire.scene:find(path):set_visible(false)
    end
    sim.step()
    assert(sim.matches_screenshot("screenshots/start.png"))
end)

test("space puts the fire out and lights it again", function()
    sim.step()
    assert(bonfire.flames:particle_count() > 10, "burning")
    sim.tap("Space")
    assert(not bonfire.flames:is_emitting() and not bonfire.sparks:is_emitting())
    -- Its last flames die down, its last sparks fall, and its embers still smoke.
    wait(2)
    assert_equal(bonfire.flames:particle_count(), 0)
    assert_equal(bonfire.sparks:particle_count(), 0)
    assert(bonfire.smoke:particle_count() > 0, "smoking")
    sim.tap("Space")
    wait(0.5)
    assert(bonfire.flames:particle_count() > 10, "burning again")
end)

test("a click in the sky sends a firework up to burst there", function()
    sim.step()
    sim.click(100, 40)
    assert_equal(#bonfire.rockets, 1)
    local rocket = bonfire.rockets[1]
    wait(0.3)
    assert(not rocket.burst_yet, "still climbing")
    assert(rocket.trail:count() > 0, "leaving a trail")
    wait(0.5)
    assert(rocket.burst_yet, "burst")
    assert(rocket.burst:count() > 0, "into sparks")
    local x, y = rocket.burst:position()
    assert_equal(x, 100)
    assert_equal(y, 40)
    -- Its sparks fade, and it's let go.
    wait(2)
    assert_equal(#bonfire.rockets, 0)
end)

test("clicks on the meadow send nothing up", function()
    sim.step()
    sim.click(100, 150)
    assert_equal(#bonfire.rockets, 0)
end)
