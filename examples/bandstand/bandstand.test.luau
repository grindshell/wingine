-- The bandstand's tests, which `cargo run -- test examples/bandstand` runs, each against a fresh
-- game with no frames run yet. They read and change its state, the global `bandstand`, as it is.
-- Without a sound device the song is silent, but keeps time with the game, so its beats land on
-- the same frames every run.

local wg = require("@wingine")

-- How many beats a frame plays, at the tempo playing.
local function beats_a_frame(): number
    return (bandstand.band:tempo() or 0) / 60 / 60
end

-- Runs frames until the next cue is due, and presses on it, as near its beat as frames allow.
local function press_on_the_next_cue()
    local cue = bandstand.judged + 1
    -- The press is heard in the frame after this, a frame later in the song.
    while bandstand.now + beats_a_frame() * 1.5 < cue do
        sim.step()
    end
    sim.tap("Space")
end

-- Hits the next `count` cues.
local function keep_time(count: number)
    for _ = 1, count do
        press_on_the_next_cue()
    end
end

-- Runs frames until `condition` holds, failing if it doesn't within `seconds`.
local function run_until(seconds: number, condition: () -> boolean)
    for _ = 1, math.ceil(seconds * 60) do
        if condition() then
            return
        end
        sim.step()
    end
    assert(condition(), `not within {seconds} seconds`)
end

test("the bandstand starts as it's drawn", function()
    sim.step()
    assert(sim.matches_screenshot("screenshots/start.png"))
end)

test("presses on the beat hit cues, and the streak brings the bass in, then the lead", function()
    keep_time(8)
    assert_equal(bandstand.streak, 8)
    assert_equal(bandstand.said, "perfect")
    assert(bandstand.layers.bass and not bandstand.layers.lead, "the bass is in")
    keep_time(8)
    assert(bandstand.layers.lead, "the lead is in")
    assert_equal(bandstand.score, 8 * 100 + 8 * 200)
end)

test("a cue gone by breaks the streak, and the layers fade out", function()
    keep_time(8)
    assert(bandstand.layers.bass)
    local missed = bandstand.judged + 1
    run_until(1, function()
        return bandstand.judged == missed
    end)
    assert_equal(bandstand.results[missed], "miss")
    assert_equal(bandstand.streak, 0)
    assert(not bandstand.layers.bass, "the bass fades out")
    -- The next is there to hit.
    keep_time(1)
    assert_equal(bandstand.streak, 1)
end)

test("a press between beats is a miss, and the cue waits", function()
    keep_time(3)
    local next_cue = bandstand.judged + 1
    run_until(1, function()
        return bandstand.now >= next_cue - 0.5
    end)
    sim.tap("Space")
    assert_equal(bandstand.said, "miss")
    assert_equal(bandstand.streak, 0)
    assert_equal(bandstand.judged, next_cue - 1)
    keep_time(1)
    assert_equal(bandstand.results[next_cue], "perfect")
end)

test("presses before the cues come count for nothing", function()
    sim.step(30)
    sim.tap("Space")
    assert_equal(bandstand.said, nil)
end)

test("a long streak through the verse plays the chorus from the next bar", function()
    keep_time(24)
    assert(bandstand.chorus_due, "the chorus is coming")
    assert_equal(bandstand.section, "verse")
    local chorus = bandstand.band:wait_for("section:chorus")
    assert(chorus, "the chorus started")
    -- A test hears the song as the frame it ran left it, which the game hears in its next update.
    sim.step()
    assert_equal(bandstand.section, "chorus")
    local _, bar, beat = bandstand.band:position()
    assert_equal(bar, 1)
    assert(beat < 1.1, "at the bar line")
    -- The 24th cue was the last beat of a bar, the verse's sixth, so the chorus started on the
    -- next beat.
    assert_equal(bandstand.beats - 1, 8 + 24)
end)

test("each pass through the verse is quicker", function()
    assert_equal(bandstand.band:tempo(), 112)
    bandstand.band:wait_for("section:verse")
    bandstand.band:wait_for("section:verse")
    -- It slides over a bar.
    wg.wait(1)
    local tempo = bandstand.band:tempo()
    assert(tempo and tempo > 112 and tempo < 116, `sliding, at {tempo}`)
    wg.wait(2)
    assert_equal(bandstand.band:tempo(), 116)
    bandstand.band:wait_for("section:verse")
    wg.wait(3)
    assert_equal(bandstand.band:tempo(), 120)
end)

test("the drums flash the lights, and the colors move along each bar", function()
    sim.step(2)
    assert(bandstand.flash > 0.5, "flashed")
    wg.wait(0.4)
    assert_equal(bandstand.flash, 0)
    local bars = bandstand.bars
    bandstand.band:wait_for("bar")
    sim.step()
    assert_equal(bandstand.bars, bars + 1)
end)

test("the drop sets off confetti", function()
    bandstand.band:go_to("chorus")
    local drop = bandstand.band:wait_for("marker:drop")
    assert(drop and drop.section == "chorus")
    sim.step()
    assert_equal(bandstand.drops, 1)
    local flying = 0
    for _, emitter in bandstand.confetti do
        flying += emitter:count()
    end
    assert_equal(flying, 4 * 2 * 12)
    -- It falls, and is gone.
    wg.wait(3)
    flying = 0
    for _, emitter in bandstand.confetti do
        flying += emitter:count()
    end
    assert_equal(flying, 0)
end)
