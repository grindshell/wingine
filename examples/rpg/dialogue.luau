-- The dialogue box, for descriptions, conversations and menus. `say` and `ask` open it and wait
-- for the player, so they only work in tasks. While a box is open, `update` takes the player's
-- input: the interact key shows the rest of the text, then closes the box, and with choices, a
-- menu picks one first: up and down, or the mouse, highlight a choice, and the interact key or a
-- click picks it.
--
-- The box is drawn here, revealing its text a glyph at a time. A question's choices are laid out
-- in `dialogue.scene.luau`, over the box's right end: a panel that fits them, copies of its hidden
-- `choice` row beside copies of its `detail`, and to its left, the highlighted choice's note.

local look = require("./look")
local conf = require("./conf")

local dialogue = {}

local WIDTH = conf.resolution.width
local HEIGHT = conf.resolution.height

-- Glyphs revealed per second, and how often text blips as it appears.
local REVEAL_RATE = 45
local GLYPHS_PER_BLIP = 3
-- The box spans the screen, this far in from each side and the bottom.
local MARGIN = 4
local PADDING = 6
-- The box is at least this many lines tall, so short lines don't make it jump about.
local MIN_LINES = 2
-- Seconds between flashes of the marker that says there's more.
local BLINK_SECONDS = 0.4

local font = graphics.default_font()
local blip = audio.load("sounds/blip.wav")

dialogue.scene = scene.load("dialogue.scene.luau")
local question = dialogue.scene:find("question")
local note = question:find("note")
local labels = question:find("choices/labels")
local details = question:find("choices/details")
local choice_template = labels:find("choice")
local detail_template = details:find("detail")
-- The rows of the question being asked, a copy of the hidden ones for each choice.
local rows = {}
local detail_rows = {}

-- Settings for `ask`, all optional.
export type Options = {
    -- Who's asking, shown above the box.
    speaker: string?,
    -- False to make the cancel key do nothing, rather than pick the last choice.
    cancel: false?,
    -- A short detail for each choice, such as a price, shown at the right of its line.
    details: { string }?,
    -- A note for each choice, such as what an item does, shown beside the choices while it's
    -- highlighted.
    notes: { string }?,
}

type Box = {
    text: string,
    speaker: string?,
    layout: any,
    -- Glyphs shown so far, counting up as the text is revealed.
    shown: number,
    choices: { string }?,
    details: { string }?,
    notes: { string }?,
    -- The menu of the choices, once they're shown.
    menu: Menu?,
    -- The choice the cancel key picks, if it picks one.
    cancel: number?,
    -- The highlighted choice, then the one picked.
    selected: number,
    closed: boolean,
    -- Seconds the box has been fully revealed, for blinking the marker.
    idle: number,
}

-- The open box, if any.
dialogue.box = nil :: Box?

-- How tall a box of `layout` is.
local function box_height(layout: any): number
    local lines = math.max(layout:line_count(), MIN_LINES)
    return lines * font:line_height() + PADDING * 2 - 2
end

local function open(text: string, speaker: string?): Box
    local box = {
        text = text,
        speaker = speaker,
        layout = font:layout(text, WIDTH - MARGIN * 2 - PADDING * 2),
        shown = 0,
        selected = 1,
        closed = false,
        idle = 0,
    }
    dialogue.box = box
    question:set_visible(false)
    return box
end

local function wait_for(box: Box)
    wait_until(function()
        return box.closed
    end)
end

-- Shows `text`, said by `speaker` if given, and waits until the player closes it.
function dialogue.say(text: string, speaker: string?)
    wait_for(open(text, speaker))
end

-- Lays out the box's choices in the scene, with their details, for its menu to pick from once the
-- text is revealed.
local function lay_out_choices(box: Box, choices: { string })
    for i, row in rows do
        row:remove()
        detail_rows[i]:remove()
    end
    rows, detail_rows = {}, {}
    local box_details = box.details
    for i, choice in choices do
        local row = labels:add(choice_template, `choice_{i}`)
        row:set_visible(true)
        row:find("label"):set_text(choice)
        rows[i] = row
        local detail = details:add(detail_template, `detail_{i}`)
        detail:set_visible(true)
        detail:set_text(box_details and box_details[i] or "")
        detail_rows[i] = detail
    end
    details:set_visible(box_details ~= nil)
    -- Its bottom-right corner just over the box's top-right one.
    question:set_position(-MARGIN, -(MARGIN + box_height(box.layout) + 1))
end

-- Shows `text` and `choices`, and waits for the player to pick one. Returns its number. The
-- cancel key picks the last choice, so it should be the one that backs out, unless
-- `options.cancel` is false.
function dialogue.ask(text: string, choices: { string }, options: Options?): number
    local settings: Options = options or {}
    local box = open(text, settings.speaker)
    box.choices = choices
    box.details = settings.details
    box.notes = settings.notes
    box.cancel = if settings.cancel == false then nil else #choices
    lay_out_choices(box, choices)
    wait_for(box)
    return box.selected
end

function dialogue.is_open(): boolean
    return dialogue.box ~= nil
end

local function close(box: Box)
    box.closed = true
    dialogue.box = nil
    question:set_visible(false)
end

-- Shows the box's choices, once its text is revealed, and returns the menu that picks one.
local function show_choices(box: Box): Menu
    question:set_visible(true)
    local menu = ui.menu(labels, { confirm = "interact" })
    box.menu = menu
    return menu
end

function dialogue.update(dt: number)
    local box = dialogue.box
    if not box then
        return
    end
    local total = box.layout:glyph_count()
    local confirm = input.pressed("interact")
    if box.shown < total then
        if confirm then
            box.shown = total
        else
            local blips = box.shown // GLYPHS_PER_BLIP
            box.shown = math.min(box.shown + dt * REVEAL_RATE, total)
            if box.shown // GLYPHS_PER_BLIP > blips then
                audio.play(blip)
            end
        end
        if box.shown >= total and box.choices then
            show_choices(box)
        end
        return
    end
    box.idle += dt
    if not box.choices then
        if confirm then
            close(box)
        end
        return
    end
    local menu = box.menu or show_choices(box)
    local happened, choice = menu:update()
    box.selected = menu:selected() or box.selected
    if happened == "picked" then
        box.selected = choice
        close(box)
    elseif happened == "cancelled" and box.cancel then
        box.selected = box.cancel
        close(box)
    end
end

-- Points out the highlighted choice, as the title's menu does, and shows its note.
local function show_selected(box: Box)
    for i, row in rows do
        local selected = i == box.selected
        row:find("pointer"):set_visible(selected)
        local color = if selected then look.TEXT else look.DIM
        row:find("label"):set_color(color)
        detail_rows[i]:set_color(color)
    end
    local text = box.notes and box.notes[box.selected]
    note:set_visible(text ~= nil)
    if text then
        local note_text = note:find("text")
        if note_text:text() ~= text then
            note_text:set_text(text)
        end
    end
end

function dialogue.draw()
    local box = dialogue.box
    if not box then
        return
    end
    local line_height = font:line_height()
    local width = WIDTH - MARGIN * 2
    local height = box_height(box.layout)
    local x, y = MARGIN, HEIGHT - MARGIN - height
    look.panel(x, y, width, height)
    graphics.draw_text_layout(box.layout, x + PADDING, y + PADDING - 1, look.TEXT, box.shown)

    if box.speaker then
        local name_width = font:measure(box.speaker)
        look.panel(x, y - line_height - 1, name_width + PADDING * 2, line_height + 2)
        graphics.draw_text(font, box.speaker, x + PADDING, y - line_height + 1, look.HIGHLIGHT)
    end

    if box.menu then
        show_selected(box)
        graphics.draw_scene(dialogue.scene)
        return
    end
    local complete = box.shown >= box.layout:glyph_count()
    if complete and not box.choices and box.idle % (BLINK_SECONDS * 2) < BLINK_SECONDS then
        -- A little arrow in the corner, saying there's more to read.
        local right, bottom = x + width - PADDING, y + height - PADDING + 1
        graphics.fill_rect(right - 5, bottom - 3, 5, 1, look.BORDER)
        graphics.fill_rect(right - 4, bottom - 2, 3, 1, look.BORDER)
        graphics.fill_rect(right - 3, bottom - 1, 1, 1, look.BORDER)
    end
end

return dialogue
