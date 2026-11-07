-- The player's bag, opened with the menu key: their stats, which gear they've equipped, and
-- what they carry. From it they change their gear and use items. `bag.visit` runs it as part
-- of a task, through menus in the dialogue box, with the stats in a panel beside them, laid out
-- in `bag.scene.luau`: a panel that fits its lines, copies of its hidden `line`.

local dialogue = require("./dialogue")
local items = require("./items")
local ui = require("./ui")

local bag = {}

local NOTHING = "—"

-- Stats raised by gear.
local RAISED = "#8fd16a"

bag.scene = scene.load("bag.scene.luau")

-- The panel's lines by what they show: the stats, then each slot's gear, then coins.
local lines = {}
do
    local panel = bag.scene:find("stats")
    local template = panel:find("line")
    local function line(key: string, name: string)
        local copy = panel:add(template, key)
        copy:set_visible(true)
        copy:find("name"):set_text(name)
        lines[key] = copy:find("value")
    end
    line("hp", "HP")
    line("attack", "Attack")
    line("defense", "Defense")
    line("speed", "Speed")
    for _, slot in items.SLOTS do
        line(slot, items.SLOT_NAMES[slot])
    end
    line("coins", "Coins")
end

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
    local function show(key: string, value: string, raised: boolean?)
        lines[key]:set_text(value)
        lines[key]:set_color(if raised then RAISED else ui.TEXT)
    end
    show("hp", `{rpg.hp}/{stats.max_hp}`, stats.max_hp > rpg.stats.max_hp)
    show("attack", tostring(stats.attack), stats.attack > rpg.stats.attack)
    show("defense", tostring(stats.defense), stats.defense > rpg.stats.defense)
    show("speed", tostring(stats.speed), stats.speed > rpg.stats.speed)
    for _, slot in items.SLOTS do
        show(slot, name_of(rpg.equipped[slot]))
    end
    show("coins", tostring(rpg.coins))
    graphics.draw_scene(bag.scene)
end

return bag
