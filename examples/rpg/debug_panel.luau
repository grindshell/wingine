-- A window of tools for trying the game out, which F1 shows and hides: where the player is, their
-- hit points and coins, the quest's stage, their items, and jumps between maps. It's laid out
-- with egui, in the game's `gui`, over the game at the window's own resolution.

local battle = require("./battle")
local dialogue = require("./dialogue")
local items = require("./items")
local story = require("./story")

local debug_panel = {}

-- Every kind of item, by name, in order.
local ITEMS = {}
for name in items.kinds do
    table.insert(ITEMS, name)
end
table.sort(ITEMS)

-- `name` with its first letter a capital.
local function capitalized(name: string): string
    return name:sub(1, 1):upper() .. name:sub(2)
end

-- The window, in which `go_to(map)` jumps to one of `maps`.
function debug_panel.gui(ui: Ui, maps: { string }, go_to: (string) -> ())
    ui:window({ title = "Debug", x = 12, y = 12, resizable = false }, function(ui)
        local player = rpg.player
        local level = rpg.level :: { title: string }
        ui:horizontal(function(ui)
            ui:sprite(player.sprite, player.tint, 32, 32)
            local column, row = player.mover:tile()
            ui:label(`{level.title}, at {column}, {row}`)
        end)
        ui:separator()

        rpg.hp = ui:slider("HP", rpg.hp, 1, items.stats().max_hp, 1)
        rpg.coins = math.floor(ui:drag("Coins", rpg.coins, 1, 0, 9999))
        rpg.quest = ui:combo("Quest", rpg.quest, story.STAGES)
        ui:small("Creatures roam from the next map entered, while hunting.")

        ui:collapsing("Items", function(ui)
            ui:grid("items", function(ui)
                for _, name in ITEMS do
                    ui:label(items.kind(name).name)
                    ui:monospace(tostring(items.count(name)))
                    if ui:button("+") then
                        items.give(name)
                    end
                    -- Gear the player is using stays.
                    ui:enabled(items.count(name) > 0 and not items.in_use(name), function(ui)
                        if ui:button("−") then
                            items.take(name)
                        end
                    end)
                    ui:end_row()
                end
            end)
        end)

        -- Jumping away in the middle of an event would leave it half done.
        local busy = rpg.busy > 0 or battle.is_active() or dialogue.is_open()
        ui:enabled(not busy, function(ui)
            ui:horizontal(function(ui)
                ui:label("Go to")
                for _, map in maps do
                    if ui:button(capitalized(map)) then
                        go_to(map)
                    end
                end
            end)
        end)
    end)
end

return debug_panel
