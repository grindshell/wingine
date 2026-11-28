-- An editor plugin: a screen of the editor listing every kind of item the game has, as
-- `items.luau` defines them, to compare their prices and what they do side by side. The editor
-- runs it; the game never ships it, as export leaves out folders whose names start with `.`.

local items = require("../../items")

-- The kinds of item by name, cheapest first, as a shop lists them.
local function by_price(): { string }
    local names = {}
    for name in items.kinds do
        table.insert(names, name)
    end
    table.sort(names, function(a, b)
        local price_a, price_b = items.kinds[a].price, items.kinds[b].price
        return price_a < price_b or price_a == price_b and a < b
    end)
    return names
end

local function gui(ui: Ui)
    ui:heading("Every Item")
    ui:separator()
    ui:scroll(function(ui)
        ui:grid("items", function(ui)
            for _, heading in { "Item", "Price", "Slot", "What it does" } do
                ui:label(heading)
            end
            ui:end_row()
            for _, name in by_price() do
                local item = items.kinds[name]
                ui:label(item.name)
                ui:label(tostring(item.price))
                ui:label(if item.slot then items.SLOT_NAMES[item.slot] else "-")
                ui:label(items.describe(name))
                ui:end_row()
            end
        end)
    end)
end

return {
    name = "Items",
    api = 1,
    screens = {
        items = { title = "Items", gui = gui },
    },
}
