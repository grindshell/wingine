-- Bandstand: a rhythm game played to a song of the engine's own instruments, written in
-- `bandstand.song.luau` and played live, so the game changes it as the player plays.
--
-- Press Space, click, or press a gamepad's A as each cue reaches the ring, on the beat. Keep time
-- and the band joins in: the bass after 8 cues in a row, then the lead after 16, each a track of
-- the song that starts silent and fades in (`performance:set_track_volume`). 24 in a row plays
-- the chorus from the next bar (`performance:go_to`). A cue missed, or a press with no cue near
-- it, breaks the streak, and the bass and lead fade out again.
--
-- - Cues are on the song's beats, from the verse's first. The game counts the beats the song says
--   have started (`performance:events`), and how far into the beat playing it is
--   (`performance:position`), and judges presses by that, so they're timed to the music as the
--   mixer plays it rather than to the game's own clock.
-- - Each time the verse comes around again, the song speeds up, sliding there over a bar
--   (`performance:set_tempo`), up to 144 beats a minute.
-- - The drums' notes flash the lights (`performance:on("note:drums")`), whose colors move along a
--   lamp each bar, and the song's `drop` marker, halfway through the chorus, sets off confetti.
--
-- Run with `cargo run -- examples/bandstand`. The song opens on the editor's Music screen, where
-- it plays as it's changed: `cargo run -p wingine-editor -- examples/bandstand`. Everything is
-- drawn with shapes. The game's state is in the global table `bandstand`, where tests can see it.

local wg = require("@wingine")
local conf = require("./conf")

local WIDTH = conf.resolution.width

-- The first beat with a cue: the verse's first, after the intro's two bars.
local FIRST_CUE = 8
-- How far from its beat a press may be, in seconds, to hit a cue perfectly, or at all.
local PERFECT = 0.05
local GOOD = 0.11
-- Points for a perfect hit and a good one, times 1 more for each 8 cues in a row before it.
local PERFECT_POINTS = 100
local GOOD_POINTS = 50
local STREAK_PER_MULTIPLIER = 8
-- Cues in a row that bring each layer in.
local BASS_STREAK = 8
local LEAD_STREAK = 16
-- Cues in a row through the verse that play the chorus.
local CHORUS_STREAK = 24
-- Seconds layers fade in and out over.
local LAYER_FADE = 1
-- Beats a minute the song speeds up by each time the verse comes around, and the fastest it goes.
local TEMPO_STEP = 4
local MAX_TEMPO = 144
-- Seconds the lights take to fade after a flash, and a judgement shows for.
local FLASH_SECONDS = 0.25
local SAY_SECONDS = 0.5

-- Where the band stands, and the lane cues travel along to the ring.
local FLOOR = 126
local LANE_TOP = 144
local LANE_Y = 158
local RING_X = 30
local RING_RADIUS = 8
-- Pixels between one beat's cue and the next.
local SPACING = 40
local BASSIST_X = 80
local DRUMMER_X = 160
local SINGER_X = 240

local BACKGROUND = "#140d22"
local CURTAIN = { "#3b1432", "#2e0f28" }
local TRUSS = "#5a5470"
local LAMP = "#2c2838"
local STAGE = "#6b4426"
local STAGE_EDGE = "#3d2614"
local SHADOW = "#1c1428"
local SKIN = "#f0c8a0"
local TROUSERS = "#2a2a40"
local LANE = "#0b0814"
local LANE_LINE = "#2c2440"
local CUE = "#ffe9a8"
local MISSED = "#4a4060"
local RING = "#ffffff"
local TEXT = "#f4f0e8"
local DIM = "#8a82a8"
-- The lights' colors, which move along a lamp each bar.
local LIGHTS = { "#ff5a7a", "#ffc640", "#4ad9ff", "#a77bff" }
-- Where each lamp hangs, and the place on the stage it shines on.
local LAMPS = {
    { x = 40, aim = BASSIST_X },
    { x = 120, aim = DRUMMER_X },
    { x = 200, aim = DRUMMER_X },
    { x = 280, aim = SINGER_X },
}

type Judgement = "perfect" | "good" | "miss"
type Layer = "bass" | "lead"

local SAYINGS = { perfect = "PERFECT", good = "GOOD", miss = "MISS" }
local SAYING_COLORS = { perfect = "#ffe066", good = "#9be36b", miss = "#ff6b6b" }

wg.input.bind("tap", { "Space", "Z", "Enter", "MouseLeft", "South" })

local song = wg.audio.load_song("bandstand.song.luau")
local BEATS_PER_BAR = song:meter()
local band = wg.audio.play_song(song)
local font = wg.graphics.default_font()

-- An emitter of the confetti the drop sets off, in `color`.
local function confetti(color: string): Particles
    return wg.graphics.new_particles({
        rate = 0,
        lifetime = { 1.6, 2.6 },
        speed = { 70, 140 },
        direction = -math.pi / 2,
        spread = 1.1,
        gravity = { 0, 80 },
        drag = 1.2,
        angle = { 0, math.pi },
        spin = { -8, 8 },
        scales = { 3 },
        colors = { color, color, color .. "00" },
    })
end

bandstand = {
    song = song,
    band = band,
    -- The section playing, as of the last update.
    section = song:start(),
    -- How many beats have started, and how far the song is from its start, in beats.
    beats = 0,
    now = 0,
    -- The tempo, as of the last update, for drawing: songs are changed and read in `update`.
    tempo = song:tempo(),
    -- The last cue judged, hit or missed.
    judged = FIRST_CUE - 1,
    -- How each cue judged went, by its beat, until it's long gone by.
    results = {} :: { [number]: Judgement },
    score = 0,
    streak = 0,
    -- Cues hit in a row outside the chorus, toward the next, and whether it's coming.
    toward_chorus = 0,
    chorus_due = false,
    -- The layers faded in.
    layers = { bass = false, lead = false },
    -- Times the verse has started.
    verses = 0,
    -- Bars started, which move the lights' colors along.
    bars = 0,
    -- How brightly the drums' last note flashed the lights, fading.
    flash = 0,
    -- Times the drop has set off confetti, and the emitters it sets off, one of each color.
    drops = 0,
    confetti = {} :: { Particles },
    -- The last judgement, and how long it has shown.
    said = nil :: Judgement?,
    said_for = 0,
}
for _, color in LIGHTS do
    table.insert(bandstand.confetti, confetti(color))
end

-- The drums flash the lights, as hard as each note is played.
band:on("note:drums", function(note)
    bandstand.flash = note.velocity or 1
end)

band:on("bar", function()
    bandstand.bars += 1
end)

-- The song speeds up each time the verse comes around again.
band:on("section:verse", function()
    bandstand.verses += 1
    local tempo = math.min(song:tempo() + (bandstand.verses - 1) * TEMPO_STEP, MAX_TEMPO)
    if tempo ~= band:tempo() then
        band:set_tempo(tempo, { over = BEATS_PER_BAR })
    end
end)

-- The drop sets off confetti from both ends of the stage.
band:on("marker:drop", function()
    bandstand.drops += 1
    for _, emitter in bandstand.confetti do
        for _, x in { 28, WIDTH - 28 } do
            emitter:set_position(x, FLOOR)
            emitter:emit(12)
        end
    end
end)

-- Fades `layer`, a track of the song, in or out, if it isn't already.
local function set_layer(layer: Layer, on: boolean)
    if bandstand.layers[layer] ~= on then
        bandstand.layers[layer] = on
        band:set_track_volume(layer, if on then 1 else 0, LAYER_FADE)
    end
end

local function say(judgement: Judgement)
    bandstand.said = judgement
    bandstand.said_for = 0
end

-- Hits the cue on the beat `cue`, bringing in the layers the streak has earned.
local function hit(cue: number, judgement: Judgement)
    bandstand.judged = cue
    bandstand.results[cue] = judgement
    local multiplier = 1 + bandstand.streak // STREAK_PER_MULTIPLIER
    local points = if judgement == "perfect" then PERFECT_POINTS else GOOD_POINTS
    bandstand.score += points * multiplier
    bandstand.streak += 1
    -- A press a little early for the verse's first cue comes as the intro ends, so every hit
    -- counts but the chorus's own.
    if bandstand.section ~= "chorus" and not bandstand.chorus_due then
        bandstand.toward_chorus += 1
    end
    if bandstand.streak >= BASS_STREAK then
        set_layer("bass", true)
    end
    if bandstand.streak >= LEAD_STREAK then
        set_layer("lead", true)
    end
    say(judgement)
end

-- Breaks the streak, fading the layers out: for the cue on the beat `cue`, gone by, or for a
-- press with no cue near it.
local function miss(cue: number?)
    if cue then
        bandstand.judged = cue
        bandstand.results[cue] = "miss"
    end
    bandstand.streak = 0
    bandstand.toward_chorus = 0
    set_layer("bass", false)
    set_layer("lead", false)
    say("miss")
end

-- Follows the song as the mixer played it to the last update: counts the beats that started, and
-- works out how far the song is from its start.
local function follow_song()
    for _, event in band:events() do
        if event.kind == "beat" then
            bandstand.beats += 1
        end
    end
    local section, _, beat = band:position()
    if section and beat then
        bandstand.section = section
        -- Into the beat that started last. Never back: the mixer can have played to the very
        -- start of a beat it has yet to say has started.
        local now = math.max(bandstand.beats - 1, 0) + beat - math.floor(beat)
        bandstand.now = math.max(bandstand.now, now)
    end
end

-- Misses the cues gone by too long ago to hit, at `tempo`.
local function pass_cues(tempo: number)
    local late = GOOD * tempo / 60
    while bandstand.judged + 1 < bandstand.now - late do
        miss(bandstand.judged + 1)
    end
end

-- Judges a press against the next cue, at `tempo`.
local function press(tempo: number)
    -- Presses before the cues come count for nothing.
    if bandstand.now < FIRST_CUE - 1 then
        return
    end
    local cue = bandstand.judged + 1
    local off = math.abs(bandstand.now - cue) * 60 / tempo
    if off <= PERFECT then
        hit(cue, "perfect")
    elseif off <= GOOD then
        hit(cue, "good")
    else
        miss(nil)
    end
end

function update(dt: number)
    follow_song()
    local tempo = band:tempo() or song:tempo()
    bandstand.tempo = tempo
    pass_cues(tempo)
    if wg.input.pressed("tap") then
        press(tempo)
    end
    -- A long streak through the verse plays the chorus from the next bar.
    if bandstand.section ~= "verse" then
        bandstand.chorus_due = false
    elseif bandstand.toward_chorus >= CHORUS_STREAK and not bandstand.chorus_due then
        bandstand.chorus_due = true
        bandstand.toward_chorus = 0
        band:go_to("chorus", "bar")
    end
    for cue in bandstand.results do
        if cue < bandstand.now - BEATS_PER_BAR * 2 then
            bandstand.results[cue] = nil
        end
    end
    bandstand.flash = math.max(bandstand.flash - dt / FLASH_SECONDS, 0)
    bandstand.said_for += dt
    for _, emitter in bandstand.confetti do
        emitter:advance(dt)
    end
end

-- How far into the beat playing the song is, from 0 to 1.
local function into_beat(): number
    return bandstand.now - math.floor(bandstand.now)
end

-- Draws `text` centered on `x`, with its top at `y`.
local function draw_centered(text: string, x: number, y: number, color: string)
    local width = font:measure(text)
    wg.graphics.draw_text(font, text, x - width / 2, y, color)
end

-- Draws a player standing on the stage at `x`, in `shirt`, lit or a silhouette, and returns where
-- their hands are: a pixel up on the beat while they play.
local function draw_player(x: number, shirt: string, lit: boolean): number
    local lift = if lit and into_beat() < 0.25 then 1 else 0
    local y = FLOOR - lift
    local legs = if lit then TROUSERS else SHADOW
    wg.graphics.fill_rect(x - 4, y - 10, 3, 10, legs)
    wg.graphics.fill_rect(x + 1, y - 10, 3, 10, legs)
    wg.graphics.fill_rect(x - 5, y - 22, 10, 12, if lit then shirt else SHADOW)
    wg.graphics.fill_circle(x, y - 27, 4, if lit then SKIN else SHADOW)
    return y - 14
end

local function draw_band()
    -- The bass player, with the bass.
    local bass = bandstand.layers.bass
    local hands = draw_player(BASSIST_X, "#3f8fd8", bass)
    local wood = if bass then "#c0803c" else SHADOW
    wg.graphics.draw_line(BASSIST_X - 6, hands + 2, BASSIST_X + 14, hands - 8, wood, 2)
    wg.graphics.fill_circle(BASSIST_X - 5, hands + 2, 4, wood)

    -- The drummer, behind the kit, which always plays.
    draw_player(DRUMMER_X, "#d8d040", true)
    local drum = FLOOR - 9
    wg.graphics.fill_circle(DRUMMER_X, drum, 9, "#c83c4c")
    wg.graphics.fill_circle(DRUMMER_X, drum, 7, "#f0e4d0")
    wg.graphics.fill_circle(DRUMMER_X - 13, FLOOR - 16, 4, "#c83c4c")
    wg.graphics.fill_rect(DRUMMER_X + 15, FLOOR - 24, 1, 24, TRUSS)
    local tilt = if bandstand.flash > 0.5 then 2 else 0
    wg.graphics.draw_line(DRUMMER_X + 9, FLOOR - 25 + tilt, DRUMMER_X + 22, FLOOR - 25, "#e8c860", 2)

    -- The lead, on a keytar.
    local lead = bandstand.layers.lead
    hands = draw_player(SINGER_X, "#d8508c", lead)
    wg.graphics.fill_rect(SINGER_X - 10, hands - 1, 20, 4, if lead then "#e8e4f4" else SHADOW)
    if lead then
        for key = 0, 4 do
            wg.graphics.fill_rect(SINGER_X - 8 + key * 4, hands - 1, 1, 2, "#1a1a28")
        end
    end
end

-- The truss, its lamps, and the light they shine, brighter as the drums flash them.
local function draw_lights()
    wg.graphics.fill_rect(0, 20, WIDTH, 3, TRUSS)
    for _, lamp in LAMPS do
        wg.graphics.fill_rect(lamp.x - 4, 23, 8, 5, LAMP)
    end
    local level = 0.3 + 0.7 * bandstand.flash
    wg.graphics.set_blend("add")
    for i, lamp in LAMPS do
        local color = LIGHTS[(i + bandstand.bars) % #LIGHTS + 1]
        local beam = wg.graphics.color(color, 0.08 * level)
        for spread = -12, 12, 6 do
            wg.graphics.draw_line(lamp.x, 28, lamp.aim + spread, FLOOR, beam, 4)
        end
        -- A pool of light on the stage: an ellipse, a row of pixels at a time.
        local pool = wg.graphics.color(color, 0.2 * level)
        for row = -4, 4 do
            local half = math.round(22 * math.sqrt(1 - (row / 5) ^ 2))
            wg.graphics.fill_rect(lamp.aim - half, FLOOR + 7 + row, half * 2, 1, pool)
        end
        wg.graphics.fill_circle(lamp.x, 28, 3, wg.graphics.color(color, level))
    end
    wg.graphics.set_blend("normal")
end

-- The lane, its cues coming in from the right to the ring, and what the last press was judged.
local function draw_lane()
    wg.graphics.fill_rect(0, LANE_TOP, WIDTH, 180 - LANE_TOP, LANE)
    wg.graphics.fill_rect(0, LANE_Y, WIDTH, 1, LANE_LINE)
    local playing = bandstand.now >= FIRST_CUE - 1
    local pulse = if playing and into_beat() < 0.15 then 1 else 0
    wg.graphics.outline_circle(RING_X, LANE_Y + 0.5, RING_RADIUS + pulse, RING)
    local first = math.max(FIRST_CUE, math.floor(bandstand.now) - 1)
    local last = math.floor(bandstand.now + (WIDTH - RING_X) / SPACING) + 1
    for cue = first, last do
        local result = bandstand.results[cue]
        if result ~= "perfect" and result ~= "good" then
            local x = RING_X + (cue - bandstand.now) * SPACING
            local radius = if cue % BEATS_PER_BAR == 0 then 5 else 3
            wg.graphics.fill_circle(x, LANE_Y + 0.5, radius, if result == "miss" then MISSED else CUE)
        end
    end
    local said = bandstand.said
    if said and bandstand.said_for < SAY_SECONDS then
        draw_centered(SAYINGS[said], RING_X, LANE_Y + RING_RADIUS + 3, SAYING_COLORS[said])
    end
    if not playing then
        draw_centered("PRESS SPACE ON THE BEAT", WIDTH / 2 + RING_X, LANE_Y + RING_RADIUS + 3, DIM)
    end
end

local function draw_hud()
    wg.graphics.draw_text(font, `SCORE {bandstand.score}`, 4, 5, TEXT)
    if bandstand.streak > 0 then
        draw_centered(`STREAK {bandstand.streak}`, WIDTH / 2, 5, TEXT)
    end
    local tempo = `{math.round(bandstand.tempo)} BPM`
    local width = font:measure(tempo)
    wg.graphics.draw_text(font, tempo, WIDTH - 4 - width, 5, DIM)
end

function draw()
    wg.graphics.clear(BACKGROUND)
    for x = 0, WIDTH - 1, 8 do
        wg.graphics.fill_rect(x, 28, 8, FLOOR - 28, CURTAIN[x // 8 % 2 + 1])
    end
    wg.graphics.fill_rect(0, FLOOR, WIDTH, 15, STAGE)
    wg.graphics.fill_rect(0, FLOOR + 15, WIDTH, LANE_TOP - FLOOR - 15, STAGE_EDGE)
    draw_band()
    draw_lights()
    for _, emitter in bandstand.confetti do
        wg.graphics.draw_particles(emitter)
    end
    draw_lane()
    draw_hud()
end
