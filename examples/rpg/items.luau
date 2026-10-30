-- Things the player can carry: things to use, such as herbs, and gear to equip. The player's are
-- in `rpg.items`, as a count of each by name, and the gear they've equipped in `rpg.equipped`,
-- by slot. Equipped gear still counts among their items.

local items = {}

-- The slots gear goes in, in the order menus list them, and their names.
items.SLOTS = { "weapon", "shield", "charm" }
items.SLOT_NAMES = { weapon = "Weapon", shield = "Shield", charm = "Charm" }
-- The stats gear can add to, in the order descriptions list them, and their names.
local STATS = { "max_hp", "attack", "defense", "speed" }
local STAT_NAMES = { max_hp = "Max HP", attack = "Attack", defense = "Defense", speed = "Speed" }

export type Item = {
    -- How the item is written in menus and messages.
    name: string,
    -- What it costs in a shop. It sells for half.
    price: number,
    -- What it's like, for menus.
    text: string,
    -- Hit points it restores when used, for things to use.
    heal: number?,
    -- The slot it goes in, and what it adds to the player's stats, for gear.
    slot: string?,
    bonus: { [string]: number }?,
}

-- Every kind of item, by name.
items.kinds = {
    herb = { name = "Herb", price = 4, heal = 15, text = "A bitter healing herb." },
    tonic = { name = "Tonic", price = 12, heal = 40, text = "A tonic that tastes of pine." },
    oak_staff = {
        name = "Oak Staff",
        price = 10,
        slot = "weapon",
        bonus = { attack = 2 },
        text = "A stout length of oak.",
    },
    iron_sword = {
        name = "Iron Sword",
        price = 28,
        slot = "weapon",
        bonus = { attack = 4 },
        text = "A plain, sharp blade from the city.",
    },
    wooden_shield = {
        name = "Wooden Shield",
        price = 12,
        slot = "shield",
        bonus = { defense = 2 },
        text = "Oak boards bound with iron.",
    },
    bronze_shield = {
        name = "Bronze Shield",
        price = 30,
        slot = "shield",
        bonus = { defense = 4 },
        text = "Heavy, and polished to a shine.",
    },
    swift_feather = {
        name = "Swift Feather",
        price = 16,
        slot = "charm",
        bonus = { speed = 3 },
        text = "A grey feather, light as air.",
    },
    ward_charm = {
        name = "Ward Charm",
        price = 20,
        slot = "charm",
        bonus = { max_hp = 10 },
        text = "A carved stone on a cord.",
    },
} :: { [string]: Item }

function items.kind(name: string): Item
    return items.kinds[name] or error(`there's no item called {name}`)
end

-- What the item `name` sells for.
function items.sell_price(name: string): number
    return items.kind(name).price // 2
end

-- What the item does, for menus: what it's like, and what it restores or adds.
function items.describe(name: string): string
    local item = items.kind(name)
    local effects = {}
    if item.heal then
        table.insert(effects, `Restores {item.heal} HP.`)
    end
    for _, stat in STATS do
        local bonus = item.bonus and item.bonus[stat]
        if bonus then
            table.insert(effects, `{STAT_NAMES[stat]} +{bonus}.`)
        end
    end
    return `{item.text} {table.concat(effects, " ")}`
end

-- The names of the items the player has at least one of, in a steady order, if they pass
-- `filter` when it's given.
function items.owned(filter: ((Item) -> boolean)?): { string }
    local owned = {}
    for name, count in rpg.items do
        if count > 0 and (not filter or filter(items.kind(name))) then
            table.insert(owned, name)
        end
    end
    table.sort(owned, function(a, b)
        local price_a, price_b = items.kind(a).price, items.kind(b).price
        return price_a < price_b or price_a == price_b and a < b
    end)
    return owned
end

-- The names of the items the player has that they can use, such as herbs.
function items.usable(): { string }
    return items.owned(function(item)
        return item.heal ~= nil
    end)
end

-- Choices for a menu of the items `names`, and options for `dialogue.ask` that show each one's
-- `detail`, such as its price, and a note on what it does. Callers add their own last choices,
-- such as "Back".
function items.menu(names: { string }, detail: (string) -> string)
    local choices, details, notes = {}, {}, {}
    for i, name in names do
        choices[i] = items.kind(name).name
        details[i] = detail(name)
        notes[i] = items.describe(name)
    end
    return choices, { details = details, notes = notes }
end

-- A detail for `items.menu`: how many of the item the player has.
function items.count_detail(name: string): string
    return `x{items.count(name)}`
end

-- The hit points using the item `name` would restore to someone with `hp` of `max_hp`: none if
-- they're unhurt.
function items.healing(name: string, hp: number, max_hp: number): number
    return math.max(math.min(items.kind(name).heal or 0, max_hp - hp), 0)
end

function items.count(name: string): number
    return rpg.items[name] or 0
end

function items.give(name: string)
    rpg.items[name] = items.count(name) + 1
end

function items.take(name: string)
    rpg.items[name] = math.max(items.count(name) - 1, 0)
end

-- Whether the player is using their only one of the item `name`, as gear.
function items.in_use(name: string): boolean
    local slot = items.kind(name).slot
    return slot ~= nil and rpg.equipped[slot] == name and items.count(name) <= 1
end

-- The player's stats with their gear's bonuses added.
function items.stats(): { [string]: number }
    local stats = table.clone(rpg.stats)
    for _, name in rpg.equipped do
        for stat, bonus in items.kind(name).bonus or {} do
            stats[stat] += bonus
        end
    end
    return stats
end

-- Puts the gear `name` in its slot, in place of whatever was there.
function items.equip(name: string)
    local slot = items.kind(name).slot or error(`{name} isn't gear`)
    rpg.equipped[slot] = name
    rpg.hp = math.min(rpg.hp, items.stats().max_hp)
end

-- Empties the slot `slot`.
function items.unequip(slot: string)
    rpg.equipped[slot] = nil
    rpg.hp = math.min(rpg.hp, items.stats().max_hp)
end

return items
