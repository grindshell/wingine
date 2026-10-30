-- Things the player can carry. The player's are in `rpg.items`, as a count of each by name.

local items = {}

export type Item = {
    -- How the item is written in menus and messages.
    name: string,
    -- Hit points it restores, used in battle.
    heal: number?,
}

-- Every kind of item, by name.
items.kinds = {
    herb = { name = "Herb", heal = 15 },
} :: { [string]: Item }

-- The names of the items in `counts` that there's at least one of, in a steady order.
function items.owned(counts: { [string]: number }): { string }
    local owned = {}
    for name, count in counts do
        if count > 0 then
            table.insert(owned, name)
        end
    end
    table.sort(owned)
    return owned
end

return items
