-- Characters: the player and the people and animals they meet, each walking the grid with a
-- mover from the engine.

local characters = {}

-- Seconds a character takes to walk one tile, unless it says otherwise.
local STEP_TIME = 0.3

export type Character = {
    name: string,
    -- The sprite it stands still as.
    sprite: any,
    -- The sprite mirrored, for facing left.
    sprite_left: any,
    -- The animation it walks with, if it has one, and mirrored. It shows a frame mid-stride, then
    -- the standing sprite, over each tile.
    walk: any?,
    walk_left: any?,
    -- Seconds it's been walking without stopping, for its animation.
    walk_time: number,
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
    -- Whether the player is talking to it, which stops it wandering. Fights count.
    talking: boolean,
    -- For a creature from `foes.luau`: its kind, its number among its map's foes, and when it
    -- will fight again after the player got away from it.
    foe: string?,
    foe_number: number?,
    calm_until: number?,
    -- Whether it has noticed the player, which shows above it.
    alert: boolean?,
    -- Whether it's been taken off the map for good.
    removed: boolean?,
}

-- A character standing on `options.tile`. `options` has the fields of `Character` that don't
-- change, apart from `sprite_left` and its walk, and the tile, the way it faces, how long it
-- takes to walk a tile, and the sprite it shows mid-stride, if it has one.
function characters.new(options): Character
    local column, row = options.tile[1], options.tile[2]
    local facing = options.facing or "down"
    local step_time = options.step_time or STEP_TIME
    local walk = nil
    if options.stride then
        walk = graphics.new_animation({ options.stride, options.sprite }, step_time / 2)
    end
    local character = {
        name = options.name,
        sprite = options.sprite,
        sprite_left = options.sprite:flipped_x(),
        walk = walk,
        walk_left = walk and walk:flipped_x(),
        walk_time = 0,
        tint = options.tint,
        mover = grid.new_mover(column, row, step_time),
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

-- Moves the character on by `dt` seconds. Its walk carries on from step to step, and starts over
-- once it stops.
function characters.update(character: Character, dt: number)
    if character.mover:is_moving() then
        character.walk_time += dt
    else
        character.walk_time = 0
    end
    character.mover:update(dt)
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

-- Turns `character` to face `other`, along whichever axis they're further apart on.
function characters.face_toward(character: Character, other: Character)
    local column, row = character.mover:tile()
    local other_column, other_row = other.mover:tile()
    local dx, dy = other_column - column, other_row - row
    if dx == 0 and dy == 0 then
        return
    elseif math.abs(dx) >= math.abs(dy) then
        characters.face(character, if dx > 0 then "right" else "left")
    else
        characters.face(character, if dy > 0 then "down" else "up")
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

-- The sprite the character shows now: a frame of its walk while it's walking, and otherwise the
-- one it stands still as.
function characters.sprite(character: Character)
    local walk = if character.facing_left then character.walk_left else character.walk
    if walk and character.mover:is_moving() then
        return walk:frame(character.walk_time)
    end
    return if character.facing_left then character.sprite_left else character.sprite
end

function characters.draw(character: Character, map)
    local sprite = characters.sprite(character)
    local x, y = characters.position(character, map)
    graphics.draw_sprite(sprite, x, y, character.tint)
end

return characters
