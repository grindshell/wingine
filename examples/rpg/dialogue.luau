-- The dialogue box, for descriptions and conversations. `say` and `ask` open it and wait for the
-- player, so they only work in tasks. While a box is open, `update` takes the player's input:
-- the interact key shows the rest of the text, then closes the box, and with choices, up and
-- down pick one first.

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

local BORDER = "#f2d3ab"
local PANEL = "#14182c"
local TEXT = "#f4f0e8"
local SPEAKER = "#f2c14e"
local CHOICE = "#8a92b8"

local font = graphics.default_font()
local blip = audio.load("sounds/blip.wav")

type Box = {
    text: string,
    speaker: string?,
    layout: any,
    -- Glyphs shown so far, counting up as the text is revealed.
    shown: number,
    choices: { string }?,
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

local function open(text: string, speaker: string?, choices: { string }?, cancel: number?): Box
    local box = {
        text = text,
        speaker = speaker,
        layout = font:layout(text, WIDTH - MARGIN * 2 - PADDING * 2),
        shown = 0,
        choices = choices,
        cancel = cancel,
        selected = 1,
        closed = false,
        idle = 0,
    }
    dialogue.box = box
    return box
end

-- Shows `text`, said by `speaker` if given, and waits until the player closes it.
function dialogue.say(text: string, speaker: string?)
    local box = open(text, speaker)
    wait_until(function()
        return box.closed
    end)
end

-- Shows `text` and `choices`, and waits for the player to pick one. Returns its number. The
-- cancel key picks the last choice, so it should be the one that backs out, unless `cancel` is
-- false, when the cancel key does nothing.
function dialogue.ask(text: string, choices: { string }, speaker: string?, cancel: false?): number
    local box = open(text, speaker, choices, if cancel == false then nil else #choices)
    wait_until(function()
        return box.closed
    end)
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
    local choices = box.choices
    if choices then
        local widest = 0
        for _, choice in choices do
            widest = math.max(widest, font:measure(choice))
        end
        local choices_width = widest + PADDING * 2 + 8
        local choices_height = #choices * line_height + PADDING * 2 - 2
        local left = x + width - choices_width
        local top = y - choices_height - 1
        panel(left, top, choices_width, choices_height)
        for i, choice in choices do
            local line_top = top + PADDING - 1 + (i - 1) * line_height
            local selected = i == box.selected
            if selected then
                graphics.draw_text(font, ">", left + PADDING, line_top, SPEAKER)
            end
            local color = if selected then TEXT else CHOICE
            graphics.draw_text(font, choice, left + PADDING + 8, line_top, color)
        end
    elseif box.idle % (BLINK_SECONDS * 2) < BLINK_SECONDS then
        -- A little arrow in the corner, saying there's more to read.
        local right, bottom = x + width - PADDING, y + height - PADDING + 1
        graphics.fill_rect(right - 5, bottom - 3, 5, 1, BORDER)
        graphics.fill_rect(right - 4, bottom - 2, 3, 1, BORDER)
        graphics.fill_rect(right - 3, bottom - 1, 1, 1, BORDER)
    end
end

return dialogue
