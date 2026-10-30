-- The player's bag, opened with the menu key: their stats, which gear they've equipped, and
-- what they carry. From it they change their gear and use items. `bag.visit` runs it as part
-- of a task, through menus in the dialogue box, with the stats in a panel beside them.

local dialogue = require("./dialogue")
local items = require("./items")
local ui = require("./ui")

local bag = {}

local PADDING = 6
-- The stats panel's corner, and the width of its column of names.
local PANEL_X, PANEL_Y = 4, 4
local NAMES_WIDTH = 52
local NOTHING = "—"

-- Stats raised by gear.
local RAISED = "#8fd16a"

local font = graphics.default_font()

-- Whether the bag is open.
bag.open = false

local function name_of(item: string?): string
    return if item then items.kind(item).name else NOTHING
end

-- Lets the player pick gear for one slot after another, until they go back.
local function change_gear()
    while true do
        local equipped = {}
        for i, slot in items.SLOTS do
            equipped[i] = name_of(rpg.equipped[slot])
        end
        local slots = {}
        for i, slot in items.SLOTS do
            slots[i] = items.SLOT_NAMES[slot]
        end
        table.insert(slots, "Back")
        local picked = dialogue.ask("Change which?", slots, { details = equipped })
        if picked > #items.SLOTS then
            return
        end

        local slot = items.SLOTS[picked]
        local owned = items.owned(function(item)
            return item.slot == slot
        end)
        local choices, options = items.menu(owned, function(name)
            return if rpg.equipped[slot] == name then "on" else ""
        end)
        table.insert(choices, "Nothing")
        table.insert(choices, "Back")
        local question = `Equip which {string.lower(items.SLOT_NAMES[slot])}?`
        local which = dialogue.ask(question, choices, options)
        if which <= #owned then
            items.equip(owned[which])
        elseif which == #owned + 1 then
            items.unequip(slot)
        end
    end
end

-- Lets the player use items, until they go back.
local function use_items()
    while true do
        local owned = items.usable()
        if #owned == 0 then
            dialogue.say("You don't have anything to use.")
            return
        end
        local choices, options = items.menu(owned, items.count_detail)
        table.insert(choices, "Back")
        local picked = dialogue.ask("Use which item?", choices, options)
        if picked > #owned then
            return
        end
        local name = owned[picked]
        local healed = items.healing(name, rpg.hp, items.stats().max_hp)
        if healed == 0 then
            dialogue.say("You're not hurt.")
        else
            items.take(name)
            rpg.hp += healed
            dialogue.say(`You used a {items.kind(name).name} and recovered {healed} HP.`)
        end
    end
end

-- The bag, open until the player closes it.
function bag.visit()
    bag.open = true
    local ok, err = pcall(function()
        while true do
            local choice = dialogue.ask("Your bag.", { "Gear", "Items", "Close" })
            if choice == 1 then
                change_gear()
            elseif choice == 2 then
                use_items()
            else
                return
            end
        end
    end)
    bag.open = false
    if not ok then
        error(err, 0)
    end
end

-- Draws the player's stats, while the bag is open.
function bag.draw()
    if not bag.open then
        return
    end
    local stats = items.stats()
    local line = function(name: string, value: string, raised: boolean?)
        return { name = name, value = value, raised = raised }
    end
    local lines = {
        line("HP", `{rpg.hp}/{stats.max_hp}`, stats.max_hp > rpg.stats.max_hp),
        line("Attack", tostring(stats.attack), stats.attack > rpg.stats.attack),
        line("Defense", tostring(stats.defense), stats.defense > rpg.stats.defense),
        line("Speed", tostring(stats.speed), stats.speed > rpg.stats.speed),
    }
    for _, slot in items.SLOTS do
        table.insert(lines, line(items.SLOT_NAMES[slot], name_of(rpg.equipped[slot])))
    end
    table.insert(lines, line("Coins", tostring(rpg.coins)))

    local widest = 0
    for _, entry in lines do
        widest = math.max(widest, font:measure(entry.value))
    end
    local line_height = font:line_height()
    local width = NAMES_WIDTH + widest + PADDING * 2
    local height = #lines * line_height + PADDING * 2 - 2
    ui.panel(PANEL_X, PANEL_Y, width, height)
    for i, entry in lines do
        local y = PANEL_Y + PADDING - 1 + (i - 1) * line_height
        graphics.draw_text(font, entry.name, PANEL_X + PADDING, y, ui.DIM)
        local color = if entry.raised then RAISED else ui.TEXT
        graphics.draw_text(font, entry.value, PANEL_X + PADDING + NAMES_WIDTH, y, color)
    end
end

return bag
