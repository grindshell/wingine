-- The title screen the game opens on: its name over the edge of the woods at night, and a menu to
-- continue the saved game, if there is one, or start a new one. While it's open, `update` takes
-- the player's input: up and down highlight a choice, and the interact key picks it.

local conf = require("./conf")
local ui = require("./ui")
local world = require("./world")

local title = {}

local WIDTH = conf.resolution.width
local HEIGHT = conf.resolution.height
local TILE = world.TILE

local NAME = "The Stone Shrine"
local TAGLINE = "A tale of Mossbrook"
local SKY = "#101418"
local STAR = "#8a92b8"
local GROUND = "#22382a"
local TREE = "#6aa84f"
-- Where the ground starts, and the trees standing along it: their sprites in the tileset, and
-- how far along they are.
local GROUND_TOP = HEIGHT - 2 * TILE
local TREES = {
    { cell = { 0, 1 }, x = 4 },
    { cell = { 4, 1 }, x = 22 },
    { cell = { 1, 1 }, x = 46 },
    { cell = { 1, 1 }, x = 258 },
    { cell = { 0, 1 }, x = 280 },
    { cell = { 4, 1 }, x = 300 },
}
-- Stars, scattered about the sky.
local STARS = {
    { 14, 12 },
    { 52, 26 },
    { 86, 8 },
    { 118, 64 },
    { 160, 14 },
    { 204, 70 },
    { 236, 20 },
    { 270, 44 },
    { 302, 10 },
    { 30, 96 },
    { 290, 100 },
}
local NAME_TOP = 28
local MENU_TOP = 80
local PADDING = 6

local font = graphics.default_font()
local big = font:scaled(2)

-- The choices, in order, while the title is open.
title.choices = nil :: { string }?
-- The highlighted choice's number.
title.selected = 1

-- Shows the title, with `choices` in its menu.
function title.open(choices: { string })
    title.choices = choices
    title.selected = 1
end

function title.close()
    title.choices = nil
end

function title.is_open(): boolean
    return title.choices ~= nil
end

-- Takes the player's input, and returns the choice they picked, if they picked one.
function title.update(): string?
    local choices = title.choices
    if not choices then
        return nil
    end
    if input.pressed("up") then
        title.selected = (title.selected - 2) % #choices + 1
    elseif input.pressed("down") then
        title.selected = title.selected % #choices + 1
    elseif input.pressed("interact") then
        return choices[title.selected]
    end
    return nil
end

-- Draws `text` with `text_font`, centered across the screen with its top at `y`.
local function centered(text_font, text: string, y: number, color)
    local width = text_font:measure(text)
    graphics.draw_text(text_font, text, math.round((WIDTH - width) / 2), y, color)
end

function title.draw()
    local choices = title.choices
    if not choices then
        return
    end
    graphics.clear(SKY)
    for _, star in STARS do
        graphics.fill_rect(star[1], star[2], 1, 1, STAR)
    end
    graphics.fill_rect(0, GROUND_TOP, WIDTH, HEIGHT - GROUND_TOP, GROUND)
    for _, tree in TREES do
        local sprite = world.sprite(tree.cell[1], tree.cell[2])
        graphics.draw_sprite(sprite, tree.x, GROUND_TOP - TILE + 4, TREE)
    end
    local player = rpg.player
    graphics.draw_sprite(player.sprite, (WIDTH - TILE) / 2, GROUND_TOP - TILE + 6, player.tint)

    centered(big, NAME, NAME_TOP, ui.HIGHLIGHT)
    centered(font, TAGLINE, NAME_TOP + 22, ui.DIM)

    -- The menu, as the dialogue box's are.
    local line_height = font:line_height()
    local widest = 0
    for _, choice in choices do
        widest = math.max(widest, font:measure(choice))
    end
    local width = widest + PADDING * 2 + 8
    local height = #choices * line_height + PADDING * 2 - 2
    local left = math.round((WIDTH - width) / 2)
    ui.panel(left, MENU_TOP, width, height)
    for i, choice in choices do
        local top = MENU_TOP + PADDING - 1 + (i - 1) * line_height
        local selected = i == title.selected
        if selected then
            graphics.draw_text(font, ">", left + PADDING, top, ui.HIGHLIGHT)
        end
        local color = if selected then ui.TEXT else ui.DIM
        graphics.draw_text(font, choice, left + PADDING + 8, top, color)
    end
end

return title
