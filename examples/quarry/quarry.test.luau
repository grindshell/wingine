-- The quarry's tests, which `cargo run -- test examples/quarry` runs, each against a fresh game
-- with no frames run yet. They read and change its state, the global `quarry`, as it is.

local function x_of(node): number
    local x = node:position()
    return x
end

test("the quarry starts as its scene lays it out", function()
    sim.step()
    assert(sim.matches_screenshot("screenshots/start.png"))
end)

test("the player pushes a crate along the ground", function()
    local crate = quarry.scene:find("crates/near")
    local start = x_of(crate)
    -- Short of the pit, which it would fall into.
    sim.hold("Right", 0.45)
    assert(x_of(crate) > start + 4, "pushed")
    assert(quarry.grounded, "on the ground")
end)

test("the lift carries the player up to the door", function()
    -- On the lift, as it waits at the bottom before going up.
    sim.step()
    quarry.player:set_position(336, 192)
    wait(3.5)
    local _, y = quarry.player:position()
    assert(y < 100, "carried up")
    sim.hold("Right", 1.5)
    assert(quarry.won, "at the door")
    assert(quarry.won_text:is_visible(), "says so")
end)

test("R starts over", function()
    sim.hold("Right", 0.5)
    sim.tap("R")
    assert_equal(x_of(quarry.player), 40)
    assert(not quarry.won)
end)
