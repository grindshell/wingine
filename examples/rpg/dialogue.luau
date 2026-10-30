-- The dialogue box, for descriptions, conversations and menus. `say` and `ask` open it and wait
-- for the player, so they only work in tasks. While a box is open, `update` takes the player's
-- input: the interact key shows the rest of the text, then closes the box, and with choices, up
-- and down pick one first.

local window = require("./window")

local dialogue = {}

local WIDTH = window.resolution.width
local HEIGHT = window.resolution.height

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
-- Space between a choice and its detail.
local DETAIL_GAP = 12
-- How wide the panel with the highlighted choice's note is.
local NOTE_WIDTH = 132

local BORDER = "#f2d3ab"
local PANEL = "#14182c"
local TEXT = "#f4f0e8"
local SPEAKER = "#f2c14e"
local CHOICE = "#8a92b8"

local font = graphics.default_font()
local blip = audio.load("sounds/blip.wav")

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
    -- The notes laid out, by choice, as they're first shown.
    note_layouts: { [number]: any },
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

local function open(text: string, speaker: string?): Box
    local box = {
        text = text,
        speaker = speaker,
        layout = font:layout(text, WIDTH - MARGIN * 2 - PADDING * 2),
        shown = 0,
        note_layouts = {},
        selected = 1,
        closed = false,
        idle = 0,
    }
    dialogue.box = box
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
    wait_for(box)
    return box.selected
end

function dialogue.is_open(): boolean
    return dialogue.box ~= nil
end

local function close(box: Box)
    box.closed = true
    dialogue.box = nil
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
        return
    end
    box.idle += dt
    local choices = box.choices
    if choices then
        local count = #choices
        if input.pressed("up") then
            box.selected = (box.selected - 2) % count + 1
        elseif input.pressed("down") then
            box.selected = box.selected % count + 1
        elseif input.pressed("cancel") and box.cancel then
            box.selected = box.cancel
            close(box)
            return
        end
    end
    if confirm then
        close(box)
    end
end

-- Draws a panel with a border.
local function panel(x: number, y: number, width: number, height: number)
    graphics.fill_rect(x, y, width, height, BORDER)
    graphics.fill_rect(x + 1, y + 1, width - 2, height - 2, PANEL)
end

-- Draws the box's choices in a panel whose bottom-right corner is at `right`, `bottom`, and the
-- highlighted choice's note beside them.
local function draw_choices(box: Box, choices: { string }, right: number, bottom: number)
    local line_height = font:line_height()
    local details = box.details or {}
    local widest = 0
    for i, choice in choices do
        local detail = details[i]
        local detail_width = if detail then DETAIL_GAP + font:measure(detail) else 0
        widest = math.max(widest, font:measure(choice) + detail_width)
    end
    local width = widest + PADDING * 2 + 8
    local height = #choices * line_height + PADDING * 2 - 2
    local left, top = right - width, bottom - height
    panel(left, top, width, height)
    for i, choice in choices do
        local line_top = top + PADDING - 1 + (i - 1) * line_height
        local selected = i == box.selected
        if selected then
            graphics.draw_text(font, ">", left + PADDING, line_top, SPEAKER)
        end
        local color = if selected then TEXT else CHOICE
        graphics.draw_text(font, choice, left + PADDING + 8, line_top, color)
        local detail = details[i]
        if detail then
            local detail_x = right - PADDING - font:measure(detail)
            graphics.draw_text(font, detail, detail_x, line_top, color)
        end
    end

    local note = box.notes and box.notes[box.selected]
    if note then
        local layout = box.note_layouts[box.selected]
        if not layout then
            layout = font:layout(note, NOTE_WIDTH - PADDING * 2)
            box.note_layouts[box.selected] = layout
        end
        local note_height = layout:line_count() * line_height + PADDING * 2 - 2
        local note_left = left - NOTE_WIDTH - 1
        panel(note_left, top, NOTE_WIDTH, note_height)
        graphics.draw_text_layout(layout, note_left + PADDING, top + PADDING - 1, TEXT)
    end
end

function dialogue.draw()
    local box = dialogue.box
    if not box then
        return
    end
    local line_height = font:line_height()
    local lines = math.max(box.layout:line_count(), MIN_LINES)
    local width = WIDTH - MARGIN * 2
    local height = lines * line_height + PADDING * 2 - 2
    local x, y = MARGIN, HEIGHT - MARGIN - height
    panel(x, y, width, height)
    graphics.draw_text_layout(box.layout, x + PADDING, y + PADDING - 1, TEXT, box.shown)

    if box.speaker then
        local name_width = font:measure(box.speaker)
        panel(x, y - line_height - 1, name_width + PADDING * 2, line_height + 2)
        graphics.draw_text(font, box.speaker, x + PADDING, y - line_height + 1, SPEAKER)
    end

    local complete = box.shown >= box.layout:glyph_count()
    if not complete then
        return
    end
    if box.choices then
        draw_choices(box, box.choices, x + width, y - 1)
    elseif box.idle % (BLINK_SECONDS * 2) < BLINK_SECONDS then
        -- A little arrow in the corner, saying there's more to read.
        local right, bottom = x + width - PADDING, y + height - PADDING + 1
        graphics.fill_rect(right - 5, bottom - 3, 5, 1, BORDER)
        graphics.fill_rect(right - 4, bottom - 2, 3, 1, BORDER)
        graphics.fill_rect(right - 3, bottom - 1, 1, 1, BORDER)
    end
end

return dialogue
