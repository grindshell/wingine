-- What's drawn around and over the field: the frame, the side panel, the boss's health and spell
-- card, messages, and the screens between games.

local boss = require("./boss")
local conf = require("./conf")
local field = require("./field")
local stress = require("./stress")

local hud = {}

local WIDTH, HEIGHT = conf.resolution.width, conf.resolution.height
local PANEL_X = field.X + field.WIDTH + 16
local LINE = 10

local font = graphics.default_font()
local big = font:scaled(2)

hud.BORDER = graphics.color("#0b0d1a")
local EDGE = graphics.color("#3a405c")
local TEXT = graphics.color("#f4f0e8")
local DIM = graphics.color("#8a92b8")
local GOLD = graphics.color("#f2c14e")
local SHADE = graphics.color({ r = 0, g = 0, b = 0, a = 0.6 })
local HEALTH = graphics.color("#ff7b9c")
local PLOT_LINE = graphics.color("#ff4048")
-- The stress test's colors for each bullet system.
hud.SYSTEM_COLORS = {
    Luau = graphics.color("#f2c14e"),
    native = graphics.color("#4de0e0"),
}

-- `n` with commas between groups of three digits, such as "1,234,567".
function hud.number(n: number): string
    local digits = string.format("%d", math.floor(n))
    local grouped = digits:reverse():gsub("(%d%d%d)", "%1,"):reverse()
    return (grouped:gsub("^,", ""))
end

-- Draws `text` centered on `x`, with its top at `y`.
function hud.centered(text_font, text: string, x: number, y: number, color)
    local width = text_font:measure(text)
    graphics.draw_text(text_font, text, x - width / 2, y, color)
end

-- Covers whatever was drawn outside the field with the border, and outlines the field.
function hud.frame()
    local right = field.X + field.WIDTH
    local bottom = field.Y + field.HEIGHT
    graphics.fill_rect(0, 0, WIDTH, field.Y, hud.BORDER)
    graphics.fill_rect(0, bottom, WIDTH, HEIGHT - bottom, hud.BORDER)
    graphics.fill_rect(0, field.Y, field.X, field.HEIGHT, hud.BORDER)
    graphics.fill_rect(right, field.Y, WIDTH - right, field.HEIGHT, hud.BORDER)
    graphics.fill_rect(field.X - 1, field.Y - 1, field.WIDTH + 2, 1, EDGE)
    graphics.fill_rect(field.X - 1, bottom, field.WIDTH + 2, 1, EDGE)
    graphics.fill_rect(field.X - 1, field.Y, 1, field.HEIGHT, EDGE)
    graphics.fill_rect(right, field.Y, 1, field.HEIGHT, EDGE)
end

-- A label on the left of the panel and its value on the right, on row `row`.
local function row(row: number, label: string, value: string, color)
    local y = field.Y + 4 + row * LINE
    graphics.draw_text(font, label, PANEL_X, y, DIM)
    local width = font:measure(value)
    graphics.draw_text(font, value, WIDTH - 12 - width, y, color or TEXT)
end

-- How long the frame before took to update, draw and render, in milliseconds.
local function timings(first: number)
    local stats = graphics.frame_stats()
    row(first, "Bullets", hud.number(shmup.bullets.count()))
    row(first + 1, "System", shmup.system, hud.SYSTEM_COLORS[shmup.system])
    row(first + 2, "Update", string.format("%.2f ms", stats.update * 1000))
    row(first + 3, "Draw", string.format("%.2f ms", stats.draw * 1000))
    row(first + 4, "Render", string.format("%.2f ms", stats.render * 1000))
end

function hud.panel()
    graphics.draw_text(big, "SHMUP", PANEL_X, field.Y + 4, GOLD)
    if shmup.mode == "stress" then
        hud.stress_panel()
        return
    end
    row(3, "Score", string.format("%010d", math.floor(shmup.score)))
    row(4, "Graze", hud.number(shmup.graze))
    local p = shmup.player
    local function stars(count: number): string
        return if count > 0 then string.rep("*", count) else "-"
    end
    row(5, "Lives", stars(p.lives), GOLD)
    row(6, "Bombs", stars(p.bombs), GOLD)
    timings(8)
    local help = { "Arrows  move", "Shift   focus", "Z       shoot", "X       bomb", "Tab     bullets", "Esc     pause" }
    for i, line in help do
        graphics.draw_text(font, line, PANEL_X, field.Y + field.HEIGHT - (#help - i + 1) * LINE, DIM)
    end
end

-- The stress test's numbers, and its plot.
function hud.stress_panel()
    timings(3)
    row(8, "Work", string.format("%.2f ms", stress.frame_time()))
    local first = 10
    for name, curve in shmup.stress.curves do
        local limit = if curve.limit then hud.number(curve.limit) else "..."
        row(first, name, limit, hud.SYSTEM_COLORS[name])
        first += 1
    end
    local x, y, width, height = PANEL_X + 4, field.Y + 150, WIDTH - PANEL_X - 16, 110
    graphics.fill_rect(x - 1, y + height, width + 2, 1, EDGE)
    graphics.fill_rect(x - 1, y, 1, height, EDGE)
    stress.draw_plot(x, y, width, height, hud.SYSTEM_COLORS, PLOT_LINE)
    local most_bullets, most_time = stress.scale()
    graphics.draw_text(font, string.format("%.0f ms", most_time), x + 2, y - LINE, DIM)
    graphics.draw_text(font, "16.7", x + 2, y + height / 2 - LINE, PLOT_LINE)
    hud.centered(font, hud.number(most_bullets), x + width - 16, y + height + 2, DIM)
    graphics.draw_text(font, "0", x - 1, y + height + 2, DIM)
    local help = { "Tab  other system", "R    run again", "Esc  back" }
    for i, line in help do
        graphics.draw_text(font, line, PANEL_X, field.Y + field.HEIGHT - (#help - i + 1) * LINE, DIM)
    end
end

-- The boss's health along the top of the field, and its spell card's name, time and bonus.
-- Drawn in field coordinates.
function hud.boss()
    local enemy = shmup.boss
    if not enemy then
        return
    end
    local share = enemy.hp / enemy.max_hp
    graphics.fill_rect(4, 4, field.WIDTH - 8, 3, SHADE)
    graphics.fill_rect(4, 4, (field.WIDTH - 8) * share, 3, HEALTH)
    local card = shmup.card
    if not card then
        return
    end
    local time = string.format("%d", math.ceil(math.max(card.time_left, 0)))
    graphics.draw_text(font, time, field.WIDTH - 4 - font:measure(time), 10, TEXT)
    graphics.draw_text(font, `{card.number}/{#boss.CARDS}`, 4, 10, DIM)
    if card.name then
        local width = font:measure(card.name)
        graphics.draw_text(font, card.name, field.WIDTH - 4 - width, 20, GOLD)
        local bonus = if card.failed then "Bonus failed" else `Bonus {hud.number(boss.bonus(card))}`
        graphics.draw_text(font, bonus, field.WIDTH - 4 - font:measure(bonus), 30, DIM)
    end
end

-- A headline and a line under it, in a band across the middle of the field. Drawn in field
-- coordinates.
function hud.banner(headline: string, detail: string?)
    local height = 2 * LINE + (if detail then LINE + 4 else 0) + 12
    local top = field.HEIGHT / 2 - height / 2
    graphics.fill_rect(0, top, field.WIDTH, height, SHADE)
    hud.centered(big, headline, field.WIDTH / 2, top + 6, TEXT)
    if detail then
        hud.centered(font, detail, field.WIDTH / 2, top + 6 + 2 * LINE + 4, DIM)
    end
end

-- The title screen's menu, over the field. Drawn in field coordinates.
function hud.title(items: { string }, selected: number)
    hud.centered(big, "WINGINE SHMUP", field.WIDTH / 2, 90, GOLD)
    hud.centered(font, "A bullet hell in Luau, with or without Rust", field.WIDTH / 2, 116, DIM)
    for i, item in items do
        local color = if i == selected then TEXT else DIM
        local text = if i == selected then `> {item} <` else item
        hud.centered(font, text, field.WIDTH / 2, 160 + i * 16, color)
    end
    local system = `Bullets in {shmup.system} (Tab to switch)`
    hud.centered(font, system, field.WIDTH / 2, 240, hud.SYSTEM_COLORS[shmup.system])
    if not danmaku then
        hud.centered(font, "No danmaku library: bullets run in Luau", field.WIDTH / 2, 252, DIM)
    end
    hud.centered(font, "Arrows to choose, Z to start", field.WIDTH / 2, 300, DIM)
end

return hud
