-- The world: the kinds of tile it's made of, the levels built from the maps in `maps/`, and
-- what's where on them.
--
-- A map draws its level as rows of characters, one per cell, which `LEGEND` turns into a kind
-- of ground and, on top of it, an optional object. Every kind has a description, which the
-- player reads by examining a cell. Maps can also have places, cells with a description or an
-- event of their own; exits, cells that lead to another map; characters; and foes, creatures
-- from `foes.luau` that the game adds while they're about.

local characters = require("./characters")
local foes = require("./foes")

local world = {}

world.TILE = 16
local TILE = world.TILE

local sheet = graphics.load_sprite_sheet("kenney_1-bit-pack.png", TILE, TILE)

-- The sprite at `column`, `row` of the tileset.
function world.sprite(column: number, row: number)
    return sheet:sprite(column, row)
end

-- A sprite that's solid white, for the color under ground tiles.
local FILL = world.sprite(8, 5)

local GRASS = "#22382a"
local DIRT = "#4e3d2c"
local WATER = "#21406a"
local PAVING = "#34353e"

-- Each kind of tile: its sprite, given as a column and row of the tileset, or several to vary
-- from cell to cell (`false` for none); its tint; whether it blocks the way; and what examining
-- it says, or several things to vary from cell to cell. Ground kinds have a `fill`, the color
-- of the cell under their sprite.
type Kind = {
    sprites: { { number } | false },
    tint: string?,
    solid: boolean?,
    fill: string?,
    text: string | { string },
}

local KINDS: { [string]: Kind } = {
    -- Ground.
    grass = {
        fill = GRASS,
        sprites = { false, false, { 1, 0 }, { 5, 0 }, { 7, 0 } },
        tint = "#3e6b3f",
        text = {
            "Soft grass, still damp with dew.",
            "A beetle trundles between the blades of grass.",
            "Clover. None of it has four leaves.",
        },
    },
    flowers = {
        fill = GRASS,
        sprites = { { 15, 6 }, { 16, 6 } },
        tint = "#e0c070",
        text = { "Wildflowers, yellow and white.", "Bees drift from flower to flower." },
    },
    path = {
        fill = DIRT,
        sprites = { false, { 2, 0 }, false, { 7, 0 } },
        tint = "#6e5a42",
        text = {
            "A dirt path, packed hard by generations of feet.",
            "Cart wheels have worn ruts into the path.",
        },
    },
    water = {
        fill = WATER,
        sprites = { false, false, { 7, 0 } },
        tint = "#4f7fb0",
        solid = true,
        text = {
            "The pond is clear and cold. Little fish dart away from your shadow.",
            "Lily pads drift on the water.",
        },
    },
    paving = {
        fill = PAVING,
        sprites = { { 4, 0 }, { 3, 0 } },
        tint = "#4a4b56",
        text = "Pale paving stones, swept clean, with moss in the cracks.",
    },

    -- Objects.
    tree = {
        sprites = { { 0, 1 }, { 1, 1 }, { 4, 1 } },
        tint = "#6aa84f",
        solid = true,
        text = {
            "An old oak. Someone carved their initials into the bark long ago.",
            "A tall pine. Its needles carpet the ground.",
            "A woodpecker has been busy with this tree.",
        },
    },
    bush = {
        sprites = { { 5, 2 } },
        tint = "#4f8f45",
        solid = true,
        text = "A thick bush, heavy with berries. They look sour.",
    },
    dead_tree = {
        sprites = { { 1, 2 }, { 6, 2 } },
        tint = "#8a8070",
        solid = true,
        text = { "A dead tree, grey and leafless.", "This tree's bark has peeled away in strips." },
    },
    fence = {
        sprites = { { 1, 3 } },
        tint = "#c8b8a0",
        solid = true,
        text = {
            "A picket fence, freshly whitewashed.",
            "A sturdy fence. One post leans a little.",
        },
    },
    gate = {
        sprites = { { 3, 3 } },
        tint = "#b08a5a",
        solid = true,
        text = "The north gate is latched shut. Elder Rowan keeps the key.",
    },
    gate_open = {
        sprites = { { 4, 3 } },
        tint = "#b08a5a",
        text = "The north gate stands open. Beyond it, the path climbs toward Shrine Hill.",
    },
    house = {
        sprites = { { 0, 19 }, { 1, 19 }, { 1, 20 } },
        tint = "#d09060",
        solid = true,
        text = {
            "A cozy cottage. Smoke curls from the chimney.",
            "A cottage with a flower box under every window.",
            "The shutters are closed. Someone inside is snoring.",
        },
    },
    elder_house = {
        sprites = { { 1, 21 } },
        tint = "#d8b070",
        solid = true,
        text = "Elder Rowan's house. Bundles of dried herbs hang by the door.",
    },
    shop = {
        sprites = { { 5, 20 } },
        tint = "#e0a050",
        solid = true,
        text = "Mossbrook Goods. A note on the door says: “Closed for restocking. Back soon!”",
    },
    sign = {
        sprites = { { 0, 7 } },
        tint = "#c8a070",
        solid = true,
        text = "A signpost. The writing has faded away.",
    },
    well = {
        sprites = { { 4, 14 } },
        tint = "#a0a8b8",
        solid = true,
        text = "The village well. The rope is new; the stones are very old.",
    },
    grave = {
        sprites = { { 0, 14 }, { 1, 14 } },
        tint = "#9098a0",
        solid = true,
        text = {
            "A weathered headstone. The name has worn away.",
            "Fresh flowers lie before this headstone.",
            "“Here lies Old Bram, who never lost at cards.”",
        },
    },
    barrel = {
        sprites = { { 13, 13 } },
        tint = "#b07848",
        solid = true,
        text = "A barrel of apples. The ones on top are bruised.",
    },
    wheat = {
        sprites = { { 12, 6 } },
        tint = "#d8b850",
        solid = true,
        text = "Golden wheat, nearly ready for harvest.",
    },
    bench = {
        sprites = { { 7, 4 } },
        tint = "#a87848",
        solid = true,
        text = "A bench, worn smooth by years of gossip.",
    },
    torii = {
        sprites = { { 21, 11 } },
        tint = "#d04a3a",
        text = "A gateway painted red, marking the way into sacred ground.",
    },
    shrine = {
        sprites = { { 3, 19 } },
        tint = "#9a9aa8",
        solid = true,
        text = "The Stone Shrine: a small hall of grey stone under a roof of moss.",
    },
    chimes = {
        sprites = { { 22, 10 } },
        tint = "#c8c8a0",
        solid = true,
        text = "Wind chimes hang from the eaves, perfectly still, though the wind tugs at your "
            .. "sleeves.",
    },
    seal_stone = {
        sprites = { { 18, 6 } },
        tint = "#b0b0bc",
        solid = true,
        text = "The seal stone, split in two.",
    },
    candle = {
        sprites = { { 3, 15 } },
        tint = "#e8d8a0",
        solid = true,
        text = "A candle, burnt down to a stub. The wax is cold.",
    },
    statue = {
        sprites = { { 19, 6 } },
        tint = "#8a8a96",
        solid = true,
        text = "A stone guardian, worn by the weather. Its eyes seem to follow the path.",
    },
    bones = {
        sprites = { { 32, 12 } },
        tint = "#d8d0c0",
        text = "Small bones, picked clean.",
    },
}

-- What each character in a map's rows holds: a kind of ground, and an object on top of it.
local LEGEND: { [string]: { string } } = {
    ["."] = { "grass" },
    ["*"] = { "flowers" },
    [":"] = { "path" },
    ["~"] = { "water" },
    ["_"] = { "paving" },
    ["T"] = { "grass", "tree" },
    ["t"] = { "grass", "bush" },
    ["D"] = { "grass", "dead_tree" },
    ["F"] = { "grass", "fence" },
    ["G"] = { "path", "gate" },
    ["H"] = { "grass", "house" },
    ["E"] = { "grass", "elder_house" },
    ["S"] = { "grass", "shop" },
    ["s"] = { "grass", "sign" },
    ["W"] = { "path", "well" },
    ["R"] = { "grass", "grave" },
    ["B"] = { "grass", "barrel" },
    ["w"] = { "grass", "wheat" },
    ["b"] = { "path", "bench" },
    ["I"] = { "path", "torii" },
    ["A"] = { "paving", "shrine" },
    ["Y"] = { "paving", "chimes" },
    ["O"] = { "paving", "seal_stone" },
    ["c"] = { "paving", "candle" },
    ["P"] = { "grass", "statue" },
    ["%"] = { "paving", "bones" },
}

-- Every level's map shares one tileset, holding a tile for each sprite of each kind, and one for
-- each fill.
local tileset = tilemap.new_tileset(TILE, TILE)
-- The tiles of each kind, one per sprite, or `false` where it has none.
local tiles: { [string]: { number | false } } = {}
-- The tile for each fill, solid or not.
local fills: { [string]: number } = {}

for name, kind in KINDS do
    local variants = {}
    for i, at in kind.sprites do
        local sprite = at and world.sprite(at[1], at[2])
        local tile = { sprite = sprite, tint = kind.tint, solid = kind.solid }
        variants[i] = sprite and tileset:add(tile)
    end
    tiles[name] = variants
    if kind.fill then
        local fill = kind.fill .. (if kind.solid then "!" else "")
        if not fills[fill] then
            fills[fill] = tileset:add({ sprite = FILL, tint = kind.fill, solid = kind.solid })
        end
    end
end

-- Cheap deterministic noise, so cells vary but each level looks the same every time. Products
-- stay below 2^53, where numbers are exact.
local function noise(column: number, row: number): number
    local h = bit32.bxor(column * 73856093 % 2 ^ 32, row * 19349663 % 2 ^ 32)
    h = bit32.bxor(h, bit32.rshift(h, 13)) * 1597 % 2 ^ 32
    return bit32.bxor(h, bit32.rshift(h, 16))
end

-- One of `choices`, picked by the cell `column`, `row`.
local function pick<T>(choices: { T }, column: number, row: number): T
    return choices[noise(column, row) % #choices + 1]
end

local function key(column: number, row: number): string
    return `{column},{row}`
end

export type Level = {
    name: string,
    -- Shown as the player enters.
    title: string,
    -- The music that plays here, or `false` for silence.
    music: string | false,
    map: any,
    -- The map's layers: the color of the ground, the ground's sprites, and objects.
    floor: number,
    ground: number,
    objects: number,
    -- Each cell, by `key`, and the kinds in it.
    cells: { [string]: { column: number, row: number, ground: string, object: string? } },
    places: { [string]: any },
    exits: { [string]: any },
    -- Everyone on the level, the player included while they're here.
    characters: { characters.Character },
    -- The map's foes, and which of them, by number, have been beaten.
    foes: { any },
    defeated: { [number]: boolean },
}

-- Sets the ground in cell `column`, `row` to the kind `name`.
function world.set_ground(level: Level, column: number, row: number, name: string)
    local kind = KINDS[name] or error(`there's no kind of tile called {name}`)
    local fill = kind.fill or error(`{name} isn't a kind of ground`)
    level.map:set(level.floor, column, row, fills[fill .. (if kind.solid then "!" else "")])
    level.map:set(level.ground, column, row, pick(tiles[name], column, row) or nil)
    level.cells[key(column, row)].ground = name
end

-- Puts an object of the kind `name` in cell `column`, `row`, or with `nil`, clears it.
function world.set_object(level: Level, column: number, row: number, name: string?)
    local tile = nil
    if name then
        tile = pick(tiles[name] or error(`there's no kind of tile called {name}`), column, row)
    end
    level.map:set(level.objects, column, row, tile or nil)
    level.cells[key(column, row)].object = name
end

-- Replaces every object of the kind `from` on the level with one of the kind `to`.
function world.replace(level: Level, from: string, to: string?)
    for _, cell in level.cells do
        if cell.object == from then
            world.set_object(level, cell.column, cell.row, to)
        end
    end
end

-- Puts a character on the level, as `options` from a map describe it, and sets it wandering if
-- it has a route.
local function place(level: Level, options): characters.Character
    local character = characters.new({
        name = options.name,
        sprite = world.sprite(options.sprite[1], options.sprite[2]),
        tint = options.tint,
        tile = options.at,
        facing = options.facing,
        talk = options.talk,
        text = options.text,
        route = options.route,
    })
    table.insert(level.characters, character)
    if character.route then
        spawn(world.wander, level, character)
    end
    return character
end

-- The level that `data`, a map from `maps/`, describes.
function world.build(data): Level
    local rows = data.rows
    local columns = #rows[1]
    local map = tilemap.new(tileset, columns, #rows)
    local level: Level = {
        name = data.name,
        title = data.title,
        music = data.music,
        map = map,
        floor = map:add_layer(),
        ground = map:add_layer(),
        objects = map:add_layer(),
        cells = {},
        places = {},
        exits = {},
        characters = {},
        foes = data.foes or {},
        defeated = {},
    }
    for r, line in rows do
        local row = r - 1
        if #line ~= columns then
            error(`row {row} of {data.name} is {#line} cells wide, not {columns}`)
        end
        for column = 0, columns - 1 do
            local symbol = string.sub(line, column + 1, column + 1)
            local holds = LEGEND[symbol]
                or error(`{data.name} has an unknown symbol "{symbol}" at {column}, {row}`)
            level.cells[key(column, row)] = { column = column, row = row, ground = holds[1] }
            world.set_ground(level, column, row, holds[1])
            if holds[2] then
                world.set_object(level, column, row, holds[2])
            end
        end
    end
    for _, place in data.places or {} do
        level.places[key(place.at[1], place.at[2])] = place
    end
    for _, exit in data.exits or {} do
        level.exits[key(exit.at[1], exit.at[2])] = exit
    end
    for _, options in data.characters or {} do
        place(level, options)
    end
    return level
end

-- Puts the level's foes that haven't been beaten, and aren't here already, on the map.
function world.add_foes(level: Level)
    for number, options in level.foes do
        local here = false
        for _, character in level.characters do
            here = here or character.foe_number == number
        end
        if not level.defeated[number] and not here then
            local kind = foes[options.foe]
                or error(`there's no kind of creature called {options.foe}`)
            local character = place(level, {
                name = kind.title,
                sprite = kind.sprite,
                tint = kind.tint,
                at = options.at,
                facing = options.facing,
                route = options.route,
            })
            character.foe = options.foe
            character.foe_number = number
        end
    end
end

-- Takes `character`, one of the level's foes, off the map for good.
function world.defeat(level: Level, character: characters.Character)
    level.defeated[character.foe_number :: number] = true
    character.removed = true
    world.remove(level, character)
end

-- Takes every foe on the level off the map for good.
function world.clear_foes(level: Level)
    for _, character in table.clone(level.characters) do
        if character.foe then
            world.defeat(level, character)
        end
    end
end

-- A foe standing next to `character`, ready to fight, if there is one. Foes the player got away
-- from leave them be until `now` reaches their `calm_until`.
function world.foe_beside(level: Level, character: characters.Character, now: number)
    local column, row = character.mover:tile()
    for _, other in level.characters do
        local c, r = other.mover:tile()
        local beside = math.abs(c - column) + math.abs(r - row) == 1
        local calm = now < (other.calm_until or 0)
        if other.foe and beside and not other.mover:is_moving() and not calm then
            return other
        end
    end
    return nil
end

function world.add(level: Level, character: characters.Character)
    table.insert(level.characters, character)
end

function world.remove(level: Level, character: characters.Character)
    local i = table.find(level.characters, character)
    if i then
        table.remove(level.characters, i)
    end
end

-- Whoever stands on cell `column`, `row`, or is stepping onto it, if anyone is.
function world.character_at(level: Level, column: number, row: number): characters.Character?
    for _, character in level.characters do
        local c, r = character.mover:tile()
        if c == column and r == row then
            return character
        end
    end
    return nil
end

function world.place_at(level: Level, column: number, row: number)
    return level.places[key(column, row)]
end

function world.exit_at(level: Level, column: number, row: number)
    return level.exits[key(column, row)]
end

-- Whether there's an object in cell `column`, `row`.
function world.has_object(level: Level, column: number, row: number): boolean
    local cell = level.cells[key(column, row)]
    return cell ~= nil and cell.object ~= nil
end

-- What examining cell `column`, `row` says: its place's text if it has one, otherwise its
-- object's description, otherwise its ground's. Outside the map, there's nothing to see.
function world.describe(level: Level, column: number, row: number): string?
    local place = world.place_at(level, column, row)
    if place and place.text then
        return place.text
    end
    local cell = level.cells[key(column, row)]
    if not cell then
        return nil
    end
    local text = KINDS[cell.object or cell.ground].text
    return if type(text) == "table" then pick(text, column, row) else text
end

-- Whether `character` may not step onto cell `column`, `row` of the level: something solid is
-- there, or someone else is.
function world.is_blocked(level: Level, column: number, row: number, character): boolean
    if level.map:is_solid(column, row) then
        return true
    end
    local there = world.character_at(level, column, row)
    return there ~= nil and there ~= character
end

-- A function for `characters.step`, telling whether `character` is blocked from a cell.
function world.blocker(level: Level, character)
    return function(column: number, row: number): boolean
        return world.is_blocked(level, column, row, character)
    end
end

-- Walks `character` along its route over and over, as a task, until it's taken off the map. It
-- stops while the player talks to it, and waits for its way to clear when someone's in it.
-- Characters only move while the player is on their level, so it waits out the player's visits
-- elsewhere too.
function world.wander(level: Level, character: characters.Character)
    local blocked = world.blocker(level, character)
    local function ready(): boolean
        return character.removed or not character.talking and not character.mover:is_moving()
    end
    while not character.removed do
        for _, move in character.route or {} do
            if type(move) == "number" then
                wait(move)
            else
                wait_until(ready)
                while not character.removed and not characters.step(character, move, blocked) do
                    wait(0.5)
                    wait_until(ready)
                end
            end
            if character.removed then
                return
            end
        end
    end
end

return world
