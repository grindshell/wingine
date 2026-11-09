-- The specimen's tests, which `cargo run -- test examples/lettering` runs, each against a fresh
-- game with no frames run yet. They read and change its state, the global `lettering`, as it is.

local function revealed(): boolean
    return lettering.shown >= lettering.paragraph:glyph_count()
end

test("the specimen shows every font", function()
    lettering.shown = lettering.paragraph:glyph_count()
    sim.step()
    assert(sim.matches_screenshot("screenshots/specimen.png"))
end)

test("Up and Down resize the sample, rasterizing Hack again", function()
    local height = lettering.sample:line_height()
    local width = lettering.sample:measure("Lettering")
    sim.tap("Up")
    assert_equal(lettering.size, 19)
    assert(lettering.sample:line_height() > height, "taller lines")
    assert(lettering.sample:measure("Lettering") > width, "wider words")

    -- Held, the key repeats, down to the smallest size.
    sim.hold("Down", 2)
    assert_equal(lettering.size, 8)
    sim.hold("W", 3)
    assert_equal(lettering.size, 48)
end)

test("the paragraph reveals a glyph at a time, then again", function()
    sim.step()
    assert(lettering.shown > 0 and lettering.shown < 1, "a part of a glyph a frame")
    assert(lettering.paragraph:line_count() > 1, "wrapped")
    wait_until(revealed)
    wait(2.1)
    assert(lettering.shown < 10, "starting again")
end)
