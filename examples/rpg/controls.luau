-- The game's controls, which the player chooses on a screen of their own from the title's menu.
-- Each action shows its keys, the first of which is the one the game names, as in "Space:
-- Examine". Picking an action waits for a key or gamepad button (`input.pressed_controls`) to put
-- first in place of the one it had first; a key another action has goes there in exchange, so no
-- two actions share one, and Escape stops waiting without changing anything. What's chosen lasts,
-- in the save `controls`, which the game reads as it starts. The screen is laid out in
-- `controls.scene.luau`: a panel of rows copied from its hidden `row`.

local look = require("./look")

local controls = {}

-- The actions the player chooses keys for, in the order the screen lists them, with what the
-- screen calls them and what they're bound to unless the player chooses otherwise.
controls.ACTIONS = {
    { action = "up", label = "Up", defaults = { "Up", "W" } },
    { action = "down", label = "Down", defaults = { "Down", "S" } },
    { action = "left", label = "Left", defaults = { "Left", "A" } },
    { action = "right", label = "Right", defaults = { "Right", "D" } },
    { action = "interact", label = "Interact", defaults = { "Space", "Enter", "E" } },
    { action = "cancel", label = "Back", defaults = { "Escape", "Backspace" } },
    { action = "menu", label = "Bag", defaults = { "Tab", "I" } },
}
-- The rows after the actions': one that puts back the defaults, and one that closes the screen.
local DEFAULTS = "Defaults"
local DONE = "Done"
-- The key that stops waiting for one.
local STOP = "Escape"

-- The save they're kept in, and its version: saves of other versions aren't read.
local SAVE = "controls"
local VERSION = 1

-- What each action is bound to, by name, as `input.bind` takes it.
controls.bound = {} :: { [string]: { string } }
-- The action waiting for its new key, if one is.
controls.waiting = nil :: string?
-- Whether the screen is open.
controls.showing = false

controls.scene = scene.load("controls.scene.luau")
local panel = controls.scene:find("menu")
local template = panel:find("row")
local menu = ui.menu(panel, { confirm = "interact" })
-- The screen's rows: one for each action, then Defaults and Done.
local rows = {}
for i, entry in controls.ACTIONS do
    local row = panel:add(template, `row_{i}`)
    row:set_visible(true)
    row:find("label"):set_text(entry.label)
    rows[i] = row
end
for _, label in { DEFAULTS, DONE } do
    local row = panel:add(template, `row_{#rows + 1}`)
    row:set_visible(true)
    row:find("label"):set_text(label)
    table.insert(rows, row)
end

-- A copy of `list`.
local function copied(list: { string }): { string }
    return table.clone(list)
end

-- Binds every action as `controls.bound` says.
local function apply()
    for _, entry in controls.ACTIONS do
        input.bind(entry.action, controls.bound[entry.action])
    end
end

-- Puts back the default controls.
local function reset()
    for _, entry in controls.ACTIONS do
        controls.bound[entry.action] = copied(entry.defaults)
    end
    apply()
end

-- Reads the controls the player chose, if they saved any this version of the game reads, and binds
-- them; or else the defaults.
function controls.load()
    reset()
    local ok, data = pcall(save.read, SAVE)
    local readable = ok and type(data) == "table" and data.version == VERSION
    if not readable or type(data.bound) ~= "table" then
        return
    end
    local chosen = {}
    for _, entry in controls.ACTIONS do
        local list = data.bound[entry.action]
        -- Names the engine doesn't know are an error, which a save from elsewhere could hold.
        if type(list) ~= "table" or #list == 0 or not pcall(input.bind, entry.action, list) then
            reset()
            return
        end
        chosen[entry.action] = copied(list)
    end
    controls.bound = chosen
    apply()
end

local function store()
    save.write(SAVE, { version = VERSION, bound = controls.bound })
end

-- Gives `action` `control` first, in place of the control it had first, which takes `control`'s
-- place, among `action`'s others or in the action that had it; if none had it, it goes. So no two
-- actions share a control, and none is left with none.
function controls.choose(action: string, control: string)
    local mine = controls.bound[action]
    local replaced = mine[1]
    if replaced == control then
        return
    end
    local at = table.find(mine, control)
    if at then
        mine[at] = replaced
    else
        for other, theirs in controls.bound do
            local found = table.find(theirs, control)
            if other ~= action and found then
                theirs[found] = replaced
            end
        end
    end
    mine[1] = control
    apply()
    store()
end

-- The first control bound to `action`, as the game names it to the player. It works in `draw`
-- too, where `input` doesn't, since it reads what this module bound.
function controls.first(action: string): string
    local bound = controls.bound[action]
    return bound and bound[1] or "?"
end

function controls.open()
    controls.showing = true
    controls.waiting = nil
    menu:select(1)
end

function controls.is_open(): boolean
    return controls.showing
end

-- The highlighted row's number.
function controls.selected(): number
    return menu:selected() or 1
end

-- Takes the player's input while the screen is open: picking an action, then the control it waits
-- for, or Defaults, or backing out.
function controls.update()
    local waiting = controls.waiting
    if waiting then
        local pressed = input.pressed_controls()[1]
        if pressed == STOP then
            controls.waiting = nil
        elseif pressed then
            controls.choose(waiting, pressed)
            controls.waiting = nil
        end
        return
    end
    local happened, row = menu:update()
    if happened == "cancelled" then
        controls.showing = false
    elseif happened == "picked" and row then
        local entry = controls.ACTIONS[row]
        if entry then
            controls.waiting = entry.action
        elseif row == #controls.ACTIONS + 1 then
            reset()
            save.remove(SAVE)
        else
            controls.showing = false
        end
    end
end

function controls.draw()
    if not controls.showing then
        return
    end
    local selected = menu:selected()
    for i, row in rows do
        local highlighted = i == selected
        row:find("pointer"):set_visible(highlighted)
        row:find("label"):set_color(if highlighted then look.TEXT else look.DIM)
        local entry = controls.ACTIONS[i]
        local keys = row:find("keys")
        if entry and controls.waiting == entry.action then
            keys:set_text("Press a key…")
            keys:set_color(look.HIGHLIGHT)
        elseif entry then
            keys:set_text(table.concat(controls.bound[entry.action], ", "))
            keys:set_color(if highlighted then look.TEXT else look.DIM)
        end
    end
    graphics.draw_scene(controls.scene)
end

return controls
