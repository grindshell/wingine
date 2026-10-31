-- The stress test: bullets pour out until a frame takes longer than a 60th of a second to update,
-- draw and render, and a plot shows how long frames take against how many bullets there are,
-- for each bullet system tried. Bullets move, turn and are checked against the player's place
-- as they are in the game, so the plot shows what the game could afford.

local art = require("./art")
local field = require("./field")
local patterns = require("./patterns")
local player = require("./player")

local stress = {}

-- Bullets a second the target count grows by, at least, and the share of itself it grows by a
-- second once that's more, so a system that manages many bullets gets there as quickly.
local RAMP = 4000
local GROWTH = 0.1
-- The time a frame has, in milliseconds.
stress.BUDGET = 1000 / 60
-- Frames whose median time is over the budget mean the limit is reached.
local WINDOW = 30
-- Once the limit is reached, the bullets settle to this share of it.
local SETTLE = 0.8
-- Bullets in each ring spawned, which come from this far around the middle.
local RING = 64
local SPREAD = 60
-- Bullets to a point on the plot.
local BIN = 500
local KINDS = { art.kinds.pellet.cyan, art.kinds.rice.purple, art.kinds.orb.red, art.kinds.pellet.yellow }

export type Curve = {
    -- The total and number of frame times measured with each bin's bullets, by bin.
    bins: { [number]: { total: number, frames: number } },
    -- The bullets at which frames went over the budget, once they have.
    limit: number?,
    -- The most bullets measured.
    most: number,
}

-- Starts a run with the running bullet system, forgetting its last one. Other systems' curves
-- stay on the plot.
function stress.start()
    local state = shmup.stress or { curves = {} }
    shmup.stress = state
    state.target = 0
    -- The times of the last frames, and the bullets they had.
    state.recent = {}
    state.recent_counts = {}
    state.last_count = 0
    state.curves[shmup.system] = { bins = {}, limit = nil, most = 0 }
    shmup.bullets.clear()
end

local function median(values: { number }): number
    local sorted = table.clone(values)
    table.sort(sorted)
    return sorted[math.ceil(#sorted / 2)]
end

-- Records that a frame with `count` bullets took `time` milliseconds.
local function record(curve: Curve, count: number, time: number)
    local bin = math.floor(count / BIN)
    local entry = curve.bins[bin]
    if not entry then
        entry = { total = 0, frames = 0 }
        curve.bins[bin] = entry
    end
    entry.total += time
    entry.frames += 1
    curve.most = math.max(curve.most, count)
end

-- How long the frame before took in milliseconds, as though it had one update: frames that run
-- two updates, or none, shouldn't count double or nothing.
function stress.frame_time(): number
    local stats = graphics.frame_stats()
    local updates = math.max(shmup.last_frame_updates, 1)
    return (stats.update / updates + stats.draw + stats.render) * 1000
end

function stress.update(dt: number)
    local state = shmup.stress
    local system = shmup.bullets
    local curve = state.curves[shmup.system]
    local time = stress.frame_time()
    if state.last_count > 0 and time > 0 then
        record(curve, state.last_count, time)
        table.insert(state.recent, time)
        table.insert(state.recent_counts, state.last_count)
        if #state.recent > WINDOW then
            table.remove(state.recent, 1)
            table.remove(state.recent_counts, 1)
        end
        if not curve.limit and #state.recent == WINDOW and median(state.recent) > stress.BUDGET then
            -- The bullets of the frames measured, as they grew over them, rather than the most.
            curve.limit = median(state.recent_counts)
            state.target = math.floor(curve.limit * SETTLE)
            -- For runs without a window, which can't see the plot.
            print(`stress test: frames went over the budget at {curve.limit} bullets in {shmup.system}`)
        end
    end
    if not curve.limit then
        state.target += math.max(RAMP, state.target * GROWTH) * dt
    end
    while system.count() < state.target do
        local x = field.WIDTH / 2 + patterns.random(-SPREAD, SPREAD)
        local y = field.HEIGHT / 2 + patterns.random(-SPREAD, SPREAD)
        local kind = KINDS[math.random(#KINDS)]
        local angle = patterns.random(0, 2 * math.pi)
        patterns.ring(kind, x, y, RING, angle, patterns.random(15, 40), 0, patterns.random(-0.3, 0.3))
    end
    system.step(dt)
    system.collide(player.START_X, player.START_Y, player.HITBOX, player.GRAZE)
    state.last_count = system.count()
end

-- A round number at least `value`, for the end of an axis: 1, 2 or 5 times a power of ten.
local function round_up(value: number): number
    local power = 10 ^ math.floor(math.log10(math.max(value, 1)))
    for _, step in { 1, 2, 5, 10 } do
        if value <= step * power then
            return step * power
        end
    end
    return 10 * power
end

-- The plot's scale: the most bullets and milliseconds its axes show.
function stress.scale(): (number, number)
    local most = 1000
    for _, curve in shmup.stress.curves do
        most = math.max(most, curve.most)
    end
    return round_up(most), stress.BUDGET * 2
end

-- Plots each curve in the area `x`, `y`, `width` by `height`, in `colors` by system, with a line
-- at the budget.
function stress.draw_plot(x: number, y: number, width: number, height: number, colors, line)
    local most_bullets, most_time = stress.scale()
    local budget_y = y + height - stress.BUDGET / most_time * height
    for dash = 0, width - 1, 4 do
        graphics.fill_rect(x + dash, budget_y, 2, 1, line)
    end
    for name, curve in shmup.stress.curves do
        local color = colors[name]
        for bin, entry in curve.bins do
            local average = entry.total / entry.frames
            local px = x + (bin + 0.5) * BIN / most_bullets * width
            local py = y + height - math.min(average / most_time, 1) * height
            graphics.fill_rect(px - 1, py - 1, 2, 2, color)
        end
    end
end

return stress
