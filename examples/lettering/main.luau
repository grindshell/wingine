-- A type specimen: the fonts Wingine writes in. The built-in pixel font; a bitmap font drawn in a
-- PNG for the RPG's title (`fonts/title.png`, described by `fonts/title.font.luau`); and Hack, a
-- TrueType font, smooth and crisp, whose glyphs are rasterized as text first needs them, so it
-- writes Greek and Cyrillic as readily as English. Most of it is `lettering.scene.luau`, whose
-- text nodes name their font files.
--
-- Up and Down, or W and S, make the sample line bigger and smaller: `font:scaled` rasterizes Hack
-- again at each size, so it stays sharp. The paragraph at the bottom reveals itself a glyph at a
-- time, laid out once with `font:layout` and drawn with `graphics.draw_text_layout`.
--
-- Run with `cargo run -- examples/lettering`.

-- Hack's size as it's loaded, pixels to the em, and the sizes the sample can be.

local wg = require("@wingine")
local HACK_SIZE = 13
local SMALLEST = 8
local LARGEST = 48

local MARGIN = 24
local SAMPLE_Y = 222
local PARAGRAPH_Y = 296

-- Glyphs the paragraph reveals a second, and seconds it stays whole before revealing it again.
local REVEAL_RATE = 40
local REVEAL_PAUSE = 2
local PARAGRAPH = "Text is laid out once and drawn a glyph at a time, as dialogue reveals itself, "
    .. "wrapped between words to the width it's given, so no word jumps to the next line as it "
    .. "appears. Hack's glyphs go into pages of its own the first time they're drawn."

local BACKGROUND = "#14182c"
local INK = "#e8e4dc"
local DIM = "#8a92b8"

wg.input.bind("bigger", { "Up", "W" })
wg.input.bind("smaller", { "Down", "S" })

local hack = wg.graphics.load_font("fonts/Hack-Regular.ttf", { size = HACK_SIZE })
local width = wg.graphics.size()

lettering = {
    specimen = wg.scene.load("lettering.scene.luau"),
    -- The sample line's size, and Hack at it.
    size = 18,
    sample = hack:scaled(18 / HACK_SIZE),
    paragraph = hack:layout(PARAGRAPH, width - 2 * MARGIN),
    -- How many of the paragraph's glyphs show, counting on past them while it waits to start
    -- again.
    shown = 0,
}

-- The sample at `size` pixels to the em.
local function resize(size: number)
    size = math.clamp(size, SMALLEST, LARGEST)
    if size ~= lettering.size then
        lettering.size = size
        lettering.sample = hack:scaled(size / HACK_SIZE)
    end
end

function update(dt: number)
    if wg.input.pressed("bigger") or wg.input.repeated("bigger") then
        resize(lettering.size + 1)
    end
    if wg.input.pressed("smaller") or wg.input.repeated("smaller") then
        resize(lettering.size - 1)
    end

    lettering.shown += REVEAL_RATE * dt
    if lettering.shown >= lettering.paragraph:glyph_count() + REVEAL_RATE * REVEAL_PAUSE then
        lettering.shown = 0
    end
end

function draw()
    wg.graphics.clear(BACKGROUND)
    wg.graphics.draw_scene(lettering.specimen)
    local sample = `Up and Down: {lettering.size} px`
    wg.graphics.draw_text(lettering.sample, sample, MARGIN, SAMPLE_Y, INK)
    wg.graphics.draw_text_layout(lettering.paragraph, MARGIN, PARAGRAPH_Y, DIM, lettering.shown)
end
