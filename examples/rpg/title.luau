-- The title screen the game opens on: its name over the edge of the woods at night, and a menu to
-- continue the saved game, if there is one, or start a new one. While it's open, `update` takes
-- the player's input: up and down highlight a choice, and the interact key picks it. The woods are
-- drawn here; the name and the menu are laid out in `title.scene.luau`, whose menu is a panel
-- that fits its choices, copies of its hidden `choice` row.

local conf = require("./conf")
local ui = require("./ui")
local world = require("./world")

local title = {}

local WIDTH = conf.resolution.width
local HEIGHT = conf.resolution.height
local TILE = world.TILE

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

title.scene = scene.load("title.scene.luau")
local menu = title.scene:find("menu")
local template = menu:find("choice")

-- The choices, in order, while the title is open.
title.choices = nil :: { string }?
-- The highlighted choice's number.
title.selected = 1
-- The menu's rows, a copy of the hidden one for each choice.
local rows = {}

-- Shows the title, with `choices` in its menu.
function title.open(choices: { string })
    title.choices = choices
    title.selected = 1
    for _, row in rows do
        row:remove()
    end
    rows = {}
    for i, choice in choices do
        local row = menu:add(template, `choice_{i}`)
        row:set_visible(true)
        row:find("label"):set_text(choice)
        rows[i] = row
    end
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

    -- The highlighted choice is pointed out, as the dialogue box's are.
    for i, row in rows do
        local selected = i == title.selected
        row:find("pointer"):set_visible(selected)
        row:find("label"):set_color(if selected then ui.TEXT else ui.DIM)
    end
    graphics.draw_scene(title.scene)
end

return title
