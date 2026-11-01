-- The player's progress, kept in the save `progress` between runs of the game: the quest's stage,
-- what they carry, where they are, and what they've changed on each level they've been to. The
-- game saves after every event, such as a conversation, a battle or a trip between maps, and
-- Continue on the title screen picks up from there.

local world = require("./world")

local progress = {}

-- The save it's kept in.
local SAVE = "progress"
-- What's saved changes as the game does: saves of other versions aren't read.
local VERSION = 1

-- What's saved: plain data, as saves hold.
export type Data = {
    version: number,
    quest: string,
    hp: number,
    coins: number,
    items: { [string]: number },
    equipped: { [string]: string },
    -- The map the player is on, the cell they're on and which way they face.
    map: string,
    at: { number },
    facing: string,
    -- For each level the player has been to, by name, the foes beaten there by number, and the
    -- cells of the places whose hidden things they've found.
    levels: { [string]: { defeated: { number }, found: { string } } },
}

-- The saved progress, if there is any that this version of the game reads. A save that can't be
-- read is left for a new game to replace.
function progress.load(): Data?
    local ok, data = pcall(save.read, SAVE)
    if not ok then
        print(`The saved game can't be read: {data}`)
        return nil
    end
    if type(data) ~= "table" or data.version ~= VERSION then
        return nil
    end
    return data
end

-- Saves the player's progress, replacing what was saved before.
function progress.save()
    local level = rpg.level :: world.Level
    local column, row = rpg.player.mover:tile()
    local levels = {}
    for name, built in rpg.levels do
        local defeated, found = {}, {}
        for number in built.defeated do
            table.insert(defeated, number)
        end
        for cell in built.found do
            table.insert(found, cell)
        end
        table.sort(defeated)
        table.sort(found)
        levels[name] = { defeated = defeated, found = found }
    end
    local data: Data = {
        version = VERSION,
        quest = rpg.quest,
        hp = rpg.hp,
        coins = rpg.coins,
        items = rpg.items,
        equipped = rpg.equipped,
        map = level.name,
        at = { column, row },
        facing = rpg.player.mover:facing(),
        levels = levels,
    }
    save.write(SAVE, data)
end

-- Puts back the progress in `data`: the player's state, and each level they'd been to, built from
-- its map with `build`, with what had changed there marked.
function progress.restore(data: Data, build: (string) -> world.Level)
    rpg.quest = data.quest
    rpg.hp = data.hp
    rpg.coins = data.coins
    rpg.items = data.items
    rpg.equipped = data.equipped
    for name, saved in data.levels do
        local level = build(name)
        for _, number in saved.defeated do
            level.defeated[number] = true
        end
        for _, cell in saved.found do
            level.found[cell] = true
        end
    end
end

return progress
