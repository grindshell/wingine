-- The world: the kinds of tile it's made of, the levels built from the scenes in `maps/`, and
-- what's where on them.
--
-- A level's scene holds a tilemap, `map`, of the tiles in `world.tileset.luau`, in three layers:
-- `floor`, which holds the kind of ground in each cell, such as `grass`, as its color; `ground`,
-- the ground's sprite, which varies from cell to cell; and `objects`, what stands on the ground,
-- such as trees. Tiles with sprites are named for their kind and which of its sprites they show,
-- such as `tree.2`. Every kind has a description, which the player reads by examining a cell.
-- The scene also marks out places, cells with a description, an event, or something to find of
-- their own; exits, cells that lead to another map; characters; and foes, creatures from
-- `foes.luau` that the game adds while they're about. Each is a node tagged for what it is, on
-- the cell it's about, whose props say the rest.

local characters = require("./characters")
local foes = require("./foes")

local world = {}

world.TILE = 16
local TILE = world.TILE

local sheet = graphics.load_sprite_sheet("kenney_1-bit-pack.png", TILE, TILE)

-- The tileset again, with its people and creatures mid-stride: raised a pixel on stretched legs.
-- The rest is unchanged, so anything else walks without a stride.
local strides = graphics.load_sprite_sheet("strides.png", TILE, TILE)

-- The sprite at `column`, `row` of the tileset.
function world.sprite(column: number, row: number)
    return sheet:sprite(column, row)
end

-- The sprite at `column`, `row` of the tileset, mid-stride.
function world.stride(column: number, row: number)
    return strides:sprite(column, row)
end

-- Each kind of tile: the tiles it varies between from cell to cell, as the tileset names them,
-- with `false` for no sprite; and what examining it says, or several things to vary from cell to
-- cell. Kinds of ground have a tile of their own on the floor, named for them.
type Kind = {
    tiles: { string | false },
    text: string | { string },
}

local KINDS: { [string]: Kind } = {
    -- Ground.
    grass = {
        tiles = { false, false, "grass.3", "grass.4", "grass.5" },
        text = {
            "Soft grass, still damp with dew.",
            "A beetle trundles between the blades of grass.",
            "Clover. None of it has four leaves.",
        },
    },
    flowers = {
        tiles = { "flowers.1", "flowers.2" },
        text = { "Wildflowers, yellow and white.", "Bees drift from flower to flower." },
    },
    path = {
        tiles = { false, "path.2", false, "path.4" },
        text = {
            "A dirt path, packed hard by generations of feet.",
            "Cart wheels have worn ruts into the path.",
        },
    },
    water = {
        tiles = { false, false, "water.3" },
        text = {
            "The pond is clear and cold. Little fish dart away from your shadow.",
            "Lily pads drift on the water.",
        },
    },
    paving = {
        tiles = { "paving.1", "paving.2" },
        text = "Pale paving stones, swept clean, with moss in the cracks.",
    },

    -- Objects.
    tree = {
        tiles = { "tree.1", "tree.2", "tree.3" },
        text = {
            "An old oak. Someone carved their initials into the bark long ago.",
            "A tall pine. Its needles carpet the ground.",
            "A woodpecker has been busy with this tree.",
        },
    },
    bush = {
        tiles = { "bush.1" },
        text = "A thick bush, heavy with berries. They look sour.",
    },
    dead_tree = {
        tiles = { "dead_tree.1", "dead_tree.2" },
        text = { "A dead tree, grey and leafless.", "This tree's bark has peeled away in strips." },
    },
    fence = {
        tiles = { "fence.1" },
        text = {
            "A picket fence, freshly whitewashed.",
            "A sturdy fence. One post leans a little.",
        },
    },
    gate = {
        tiles = { "gate.1" },
        text = "The north gate is latched shut. Elder Rowan keeps the key.",
    },
    gate_open = {
        tiles = { "gate_open.1" },
        text = "The north gate stands open. Beyond it, the path climbs toward Shrine Hill.",
    },
    house = {
        tiles = { "house.1", "house.2", "house.3" },
        text = {
            "A cozy cottage. Smoke curls from the chimney.",
            "A cottage with a flower box under every window.",
            "The shutters are closed. Someone inside is snoring.",
        },
    },
    elder_house = {
        tiles = { "elder_house.1" },
        text = "Elder Rowan's house. Bundles of dried herbs hang by the door.",
    },
    shop = {
        tiles = { "shop.1" },
        text = "Mossbrook Goods, the village shop. Hazel keeps it stocked with all sorts.",
    },
    sign = {
        tiles = { "sign.1" },
        text = "A signpost. The writing has faded away.",
    },
    well = {
        tiles = { "well.1" },
        text = "The village well. The rope is new; the stones are very old.",
    },
    grave = {
        tiles = { "grave.1", "grave.2", "grave.3" },
        text = {
            "A weathered headstone. The name has worn away.",
            "Fresh flowers lie before this headstone.",
            "“Here lies Old Bram, who never lost at cards.”",
        },
    },
    barrel = {
        tiles = { "barrel.1" },
        text = "A barrel of apples. The ones on top are bruised.",
    },
    wheat = {
        tiles = { "wheat.1" },
        text = "Golden wheat, nearly ready for harvest.",
    },
    bench = {
        tiles = { "bench.1" },
        text = "A bench, worn smooth by years of gossip.",
    },
    torii = {
        tiles = { "torii.1" },
        text = "A gateway painted red, marking the way into sacred ground.",
    },
    shrine = {
        tiles = { "shrine.1" },
        text = "The Stone Shrine: a small hall of grey stone under a roof of moss.",
    },
    chimes = {
        tiles = { "chimes.1" },
        text = "Wind chimes hang from the eaves, perfectly still, though the wind tugs at your "
            .. "sleeves.",
    },
    seal_stone = {
        tiles = { "seal_stone.1" },
        text = "The seal stone, split in two.",
    },
    candle = {
        tiles = { "candle.1" },
        text = "A candle, burnt down to a stub. The wax is cold.",
    },
    statue = {
        tiles = { "statue.1" },
        text = "A stone guardian, worn by the weather. Its eyes seem to follow the path.",
    },
    bones = {
        tiles = { "bones.1" },
        text = "Small bones, picked clean.",
    },
}

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

-- The kind of tile a tile with a sprite, such as `tree.2`, is.
local function kind_of(tile: string): string
    return string.match(tile, "^(.*)%.%d+$") or error(`{tile} isn't a tile with a sprite`)
end

export type Level = {
    name: string,
    -- Shown as the player enters.
    title: string,
    -- The music that plays here, or `false` for silence.
    music: string | false,
    -- The level's scene, its tilemap, and the tilemap's layers: the color of the ground, the
    -- ground's sprites, and objects.
    scene: any,
    map: any,
    floor: any,
    ground: any,
    objects: any,
    -- Each cell, by `key`, and the kinds in it.
    cells: { [string]: { column: number, row: number, ground: string, object: string? } },
    places: { [string]: any },
    exits: { [string]: any },
    -- Everyone on the level, the player included while they're here.
    characters: { characters.Character },
    -- The map's foes, and which of them, by number, have been beaten.
    foes: { any },
    defeated: { [number]: boolean },
    -- The places whose hidden things the player has found.
    found: { [any]: boolean },
}

-- Sets the ground in cell `column`, `row` to the kind `name`.
function world.set_ground(level: Level, column: number, row: number, name: string)
    local kind = KINDS[name] or error(`there's no kind of tile called {name}`)
    level.floor:set_tile(column, row, name)
    level.ground:set_tile(column, row, pick(kind.tiles, column, row) or nil)
    level.cells[key(column, row)].ground = name
end

-- Puts an object of the kind `name` in cell `column`, `row`, or with `nil`, clears it.
function world.set_object(level: Level, column: number, row: number, name: string?)
    local tile = nil
    if name then
        local kind = KINDS[name] or error(`there's no kind of tile called {name}`)
        tile = pick(kind.tiles, column, row)
    end
    level.objects:set_tile(column, row, tile or nil)
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
local function put_character(level: Level, options): characters.Character
    local character = characters.new({
        name = options.name,
        sprite = world.sprite(options.sprite[1], options.sprite[2]),
        stride = world.stride(options.sprite[1], options.sprite[2]),
        tint = options.tint,
        tile = options.at,
        facing = options.facing,
        talk = options.talk,
        text = options.text,
        route = options.route,
    })
    table.insert(level.characters, character)
    -- An empty route would loop forever without waiting.
    if character.route and #character.route > 0 then
        spawn(world.wander, level, character)
    end
    return character
end

-- The level named `name` that `scene`, the root of a scene from `maps/`, describes.
function world.build(name: string, scene): Level
    local props = scene:props()
    local map = scene:find("map")
    local level: Level = {
        name = name,
        title = props.title,
        music = props.music,
        scene = scene,
        map = map,
        floor = map:find("floor"),
        ground = map:find("ground"),
        objects = map:find("objects"),
        cells = {},
        places = {},
        exits = {},
        characters = {},
        foes = {},
        defeated = {},
        found = {},
    }
    local columns, rows = map:grid_size()
    for row = 0, rows - 1 do
        for column = 0, columns - 1 do
            local object = level.objects:tile(column, row)
            level.cells[key(column, row)] = {
                column = column,
                row = row,
                ground = level.floor:tile(column, row)
                    or error(`{name} has no ground at {column}, {row}`),
                object = if object then kind_of(object) else nil,
            }
        end
    end
    -- The cell a marker is on.
    local function cell(marker): (number, number)
        return map:world_to_tile(marker:world_position())
    end
    for _, place in scene:tagged("place") do
        level.places[key(cell(place))] = place:props()
    end
    for _, exit in scene:tagged("exit") do
        level.exits[key(cell(exit))] = exit:props()
    end
    for _, marker in scene:tagged("character") do
        local options = marker:props()
        options.at = { cell(marker) }
        put_character(level, options)
    end
    for _, marker in scene:tagged("foe") do
        local options = marker:props()
        options.at = { cell(marker) }
        table.insert(level.foes, options)
    end
    return level
end

-- The music playing, if any.
local playing: string? = nil

-- Plays the level's music, crossfading from whatever was playing over `fade` seconds, unless
-- it's playing already. A level without music fades to silence.
function world.play_music(level: Level, fade: number)
    local music = level.music or nil
    if music == playing then
        return
    end
    if music then
        audio.play_music(music, fade)
    else
        audio.stop_music(fade)
    end
    playing = music
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
            local character = put_character(level, {
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
