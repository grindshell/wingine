-- Characters: the player and the people and animals they meet, each walking the grid with a
-- mover from the engine.

local characters = {}

-- Seconds a character takes to walk one tile, unless it says otherwise.
local STEP_TIME = 0.3

export type Character = {
    name: string,
    sprite: any,
    -- The sprite mirrored, for facing left.
    sprite_left: any,
    tint: string,
    mover: any,
    facing_left: boolean,
    -- The way it faces when left alone, to turn back to after talking.
    home_facing: string,
    -- The name of its conversation in `story.talk`, if it has one.
    talk: string?,
    -- What examining it says, if it has no conversation.
    text: string?,
    -- Directions to walk and seconds to wait, in a loop, if it wanders.
    route: { string | number }?,
    -- Whether the player is talking to it, which stops it wandering.
    talking: boolean,
}

-- A character standing on `options.tile`. `options` has the fields of `Character` that don't
-- change, apart from `sprite_left`, and the tile, the way it faces, and how long it takes to
-- walk a tile.
function characters.new(options): Character
    local column, row = options.tile[1], options.tile[2]
    local facing = options.facing or "down"
    local character = {
        name = options.name,
        sprite = options.sprite,
        sprite_left = options.sprite:flipped_x(),
        tint = options.tint,
        mover = grid.new_mover(column, row, options.step_time or STEP_TIME),
        facing_left = false,
        home_facing = facing,
        talk = options.talk,
        text = options.text,
        route = options.route,
        talking = false,
    }
    characters.face(character, facing)
    return character
end

-- Turns to face `direction`. Sprites face right, and are mirrored to face left; facing up or
-- down keeps whichever way it faced before.
function characters.face(character: Character, direction: string)
    character.mover:face(direction)
    if direction == "left" then
        character.facing_left = true
    elseif direction == "right" then
        character.facing_left = false
    end
end

-- Turns toward `direction` and starts walking a tile that way, unless `blocked` says it can't.
-- Returns whether it started.
function characters.step(character: Character, direction: string, blocked): boolean
    characters.face(character, direction)
    return character.mover:step(direction, blocked)
end

-- The tile in front of the character.
function characters.facing_tile(character: Character): (number, number)
    local column, row = character.mover:tile()
    return grid.neighbor(character.mover:facing(), column, row)
end

-- Where the character is in the world, snapped to whole pixels so it doesn't jitter against
-- the map.
function characters.position(character: Character, map): (number, number)
    local x, y = map:tile_to_world(character.mover:position())
    return math.round(x), math.round(y)
end

function characters.draw(character: Character, map)
    local sprite = if character.facing_left then character.sprite_left else character.sprite
    local x, y = characters.position(character, map)
    graphics.draw_sprite(sprite, x, y, character.tint)
end

return characters
