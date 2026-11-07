-- What's drawn around and over the field, laid out in `hud.scene.luau`: the frame, the side panel,
-- the boss's health and spell card, messages, and the title screen's menu. `hud.draw` fills its
-- nodes in from the game's state, shows the ones the game's mode needs and draws it, in canvas
-- pixels, so the mouse can point at the menu's items.

local boss = require("./boss")
local stress = require("./stress")

local hud = {}

local TEXT = graphics.color("#f4f0e8")
local DIM = graphics.color("#8a92b8")
local PLOT_LINE = graphics.color("#ff4048")
-- The stress test's colors for each bullet system.
hud.SYSTEM_COLORS = {
    Luau = graphics.color("#f2c14e"),
    native = graphics.color("#4de0e0"),
}

hud.scene = scene.load("hud.scene.luau")

-- The node at `path` from the scene's top, found the first time it's asked for.
local found = {}
local function node(path: string)
    local at = found[path]
    if not at then
        at = hud.scene:find(path) or error(`hud.scene.luau has no node {path}`)
        found[path] = at
    end
    return at
end

-- The title menu's items, areas whose props say what they start, and what each is called.
hud.items = node("field/title/menu"):children()
local item_names = {}
for i, item in hud.items do
    item_names[i] = item:find("label"):text()
end

-- `n` with commas between groups of three digits, such as "1,234,567".
function hud.number(n: number): string
    local digits = string.format("%d", math.floor(n))
    local grouped = digits:reverse():gsub("(%d%d%d)", "%1,"):reverse()
    return (grouped:gsub("^,", ""))
end

-- The number of the menu item at `x`, `y` on the canvas, if there's one there.
function hud.item_at(x: number, y: number): number?
    for i, item in hud.items do
        if item:contains(x, y) then
            return i
        end
    end
    return nil
end

-- Sets the value of the panel's row `path`, and its color if given.
local function row(path: string, value: string, color: number?)
    local value_node = node(`panel/rows/{path}/value`)
    value_node:set_text(value)
    if color then
        value_node:set_color(color)
    end
end

-- How long the frame before took to update, draw and render, in milliseconds. The rows stack
-- under the play's, or with the stress test's hidden, first.
local function timings()
    local frame = graphics.frame_stats()
    row("stats/bullets", hud.number(shmup.bullets.count()))
    row("stats/system", shmup.system, hud.SYSTEM_COLORS[shmup.system])
    row("stats/update", string.format("%.2f ms", frame.update * 1000))
    row("stats/draw", string.format("%.2f ms", frame.draw * 1000))
    row("stats/render", string.format("%.2f ms", frame.render * 1000))
end

local function stars(count: number): string
    return if count > 0 then string.rep("*", count) else "-"
end

-- The stress test's numbers, and its plot's scale.
local function fill_stress_panel()
    timings()
    row("stress/work", string.format("%.2f ms", stress.frame_time()))
    local rows = node("panel/rows/stress/curves"):children()
    for _, curve_row in rows do
        curve_row:set_visible(false)
    end
    local i = 1
    for name, curve in shmup.stress.curves do
        rows[i]:set_visible(true)
        rows[i]:find("label"):set_text(name)
        local limit = if curve.limit then hud.number(curve.limit) else "..."
        row(`stress/curves/{i}`, limit, hud.SYSTEM_COLORS[name])
        i += 1
    end
    local most_bullets, most_time = stress.scale()
    node("panel/rows/stress/plot/most_time"):set_text(string.format("%.0f ms", most_time))
    node("panel/rows/stress/plot/most_bullets"):set_text(hud.number(most_bullets))
end

local function fill_panel()
    local stressing = shmup.mode == "stress"
    for _, part in { "rows/play", "help/play" } do
        node(`panel/{part}`):set_visible(not stressing)
    end
    for _, part in { "rows/stress", "help/stress" } do
        node(`panel/{part}`):set_visible(stressing)
    end
    if stressing then
        fill_stress_panel()
        return
    end
    timings()
    row("play/score", string.format("%010d", math.floor(shmup.score)))
    row("play/graze", hud.number(shmup.graze))
    row("play/lives", stars(shmup.player.lives))
    row("play/bombs", stars(shmup.player.bombs))
end

-- The boss's health along the top of the field, and its spell card's number, time, name and
-- bonus.
local function fill_boss()
    local enemy = shmup.boss
    node("field/boss"):set_visible(shmup.mode == "play" and enemy ~= nil)
    if not enemy then
        return
    end
    node("field/boss/health"):set_scale(enemy.hp / enemy.max_hp, 1)
    local card = shmup.card
    node("field/boss/card"):set_visible(card ~= nil)
    node("field/boss/time"):set_visible(card ~= nil)
    node("field/boss/name"):set_visible(card ~= nil and card.name ~= nil)
    node("field/boss/bonus"):set_visible(card ~= nil and card.name ~= nil)
    if not card then
        return
    end
    node("field/boss/card"):set_text(`{card.number}/{#boss.CARDS}`)
    node("field/boss/time"):set_text(string.format("%d", math.ceil(math.max(card.time_left, 0))))
    if card.name then
        node("field/boss/name"):set_text(card.name)
        local bonus = if card.failed then "Bonus failed" else `Bonus {hud.number(boss.bonus(card))}`
        node("field/boss/bonus"):set_text(bonus)
    end
end

-- The title screen's menu, with the selected item pointed out.
local function fill_title()
    node("field/title"):set_visible(shmup.mode == "title")
    for i, item in hud.items do
        local label = item:find("label")
        local selected = i == shmup.selected
        label:set_text(if selected then `> {item_names[i]} <` else item_names[i])
        label:set_color(if selected then TEXT else DIM)
    end
    local system = node("field/title/system")
    system:set_text(`Bullets in {shmup.system} (Tab to switch)`)
    system:set_color(hud.SYSTEM_COLORS[shmup.system])
    node("field/title/no_library"):set_visible(danmaku == nil)
end

-- The headline and the line under it to show across the middle of the field, if any.
local function message(): (string?, string?)
    if shmup.mode == "over" then
        return "Game over", `Score {hud.number(shmup.score)} - Z to go on`
    elseif shmup.mode == "clear" then
        return "Stage clear!", `Score {hud.number(shmup.score)} - Z to go on`
    elseif shmup.mode == "play" and shmup.paused then
        return "Paused", "Esc to go on, X to quit"
    elseif shmup.mode == "play" and shmup.message then
        return shmup.message.headline, shmup.message.detail
    end
    return nil, nil
end

-- Draws the HUD over the field, in canvas pixels.
function hud.draw()
    fill_panel()
    fill_boss()
    fill_title()
    local headline, detail = message()
    node("field/banner"):set_visible(headline ~= nil)
    if headline then
        node("field/banner/headline"):set_text(headline)
        node("field/banner/detail"):set_text(detail or "")
    end
    graphics.draw_scene(hud.scene)
    if shmup.mode == "stress" then
        local plot = node("panel/rows/stress/plot"):bounds()
        stress.draw_plot(plot.x, plot.y, plot.width, plot.height, hud.SYSTEM_COLORS, PLOT_LINE)
    end
end

return hud
