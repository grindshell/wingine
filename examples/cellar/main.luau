-- A cellar in 2.5D, written entirely in Luau: the hero walks a dark cellar tile by tile and lights
-- its candles by standing on them. The cellar is meshes in a 3D view, walls of cubes on a floor
-- built as a mesh of its own, with barrels, crates and a table in the way, models made in Blender,
-- and the hero and the candles are sprites standing among them, billboards that turn to face the
-- camera, which follows the hero from above and behind. Walk with the arrow keys or WASD, and
-- point at a prop with the mouse to name it.
--
-- The game's state is the global `cellar`, for tests.
--
-- Run with `cargo run -- examples/cellar`.

local TILE = 16
local tiles = graphics.load_sprite_sheet("kenney_1-bit-pack.png", TILE, TILE)
local HERO = tiles:sprite(27, 0)
local CANDLE = tiles:sprite(3, 15)
local font = graphics.default_font()

-- The cellar, a row of tiles to a string: `#` is a wall, `c` a candle, `b` a barrel, `x` a crate,
-- `t` a table, and `@` where the hero starts. A tile is a world unit, with columns along `x` and
-- rows along `z`, so up the map is away from the camera.
local MAP = {
    "###########",
    "#cx.b#...c#",
    "#.#.##.#t.#",
    "#...c..#x.#",
    "#b#####c#.#",
    "#....@....#",
    "###########",
}

-- The props, by the characters that place them, and their models, made in Blender, each standing
-- on its origin.
local PROPS = { b = "barrel", x = "crate", t = "table" }
local models: { [string]: Model } = {}
for _, kind in PROPS do
    models[kind] = graphics.load_model(`models/{kind}.glb`)
end

-- Seconds a step takes.
local STEP_TIME = 0.18
-- The hero's height, the candles' and the walls', in world units: walls are low enough that the
-- hero shows over them.
local HERO_HEIGHT = 0.9
local CANDLE_HEIGHT = 1
-- Where a candle stands from the middle of its tile.
local CANDLE_OFFSET = vector.create(0.2, 0, -0.3)
local WALL_HEIGHT = 0.6
-- Where the camera is from the hero, whom it looks at.
local CAMERA_OFFSET = vector.create(0, 7, 3.5)

local WALL = graphics.color("#7a7480")
local FLOOR = graphics.color("#4a4038")
local UNLIT = graphics.color("#a09890")
local LIGHTING = { direction = vector.create(-0.3, -1, -0.5), color = "#b0a8a0", ambient = "#505060" }

local DIRECTIONS = { "up", "down", "left", "right" }
for _, direction in DIRECTIONS do
    local keys = {
        up = { "Up", "W" },
        down = { "Down", "S" },
        left = { "Left", "A" },
        right = { "Right", "D" },
    }
    input.bind(direction, keys[direction])
end

type Prop = { kind: string, column: number, row: number }

local walls = {}
local candles = {}
local props: { Prop } = {}
local start_column, start_row = 0, 0
for row, line in MAP do
    for column = 1, #line do
        local cell = string.sub(line, column, column)
        -- Columns and rows count from 0, as the grid's do.
        local at = { column = column - 1, row = row - 1 }
        if cell == "#" then
            table.insert(walls, at)
        elseif cell == "c" then
            table.insert(candles, { column = at.column, row = at.row, lit = false })
        elseif PROPS[cell] then
            table.insert(props, { kind = PROPS[cell], column = at.column, row = at.row })
        elseif cell == "@" then
            start_column, start_row = at.column, at.row
        end
    end
end

-- The character of the tile at `column`, `row`, or `#` off the map.
local function cell_at(column: number, row: number): string
    local line = MAP[row + 1]
    if line == nil or column < 0 or column >= #line then
        return "#"
    end
    return string.sub(line, column + 1, column + 1)
end

-- Whether the tile at `column`, `row` is a wall, or off the map.
local function is_wall(column: number, row: number): boolean
    return cell_at(column, row) == "#"
end

-- Whether the hero can't walk onto the tile: a wall, a prop, or off the map.
local function is_blocked(column: number, row: number): boolean
    local cell = cell_at(column, row)
    return cell == "#" or PROPS[cell] ~= nil
end

-- The floor: a quad facing up for every tile that isn't a wall, alternately a little lighter and
-- darker, as one mesh.
local function floor_mesh()
    local vertices, indices = {}, {}
    for row, line in MAP do
        for column = 1, #line do
            if not is_wall(column - 1, row - 1) then
                local x, z = column - 1, row - 1
                local color = if (x + z) % 2 == 0 then "#ffffff" else "#d0d0d0"
                local first = #vertices
                for _, corner in { { 0, 1 }, { 1, 1 }, { 1, 0 }, { 0, 0 } } do
                    table.insert(vertices, {
                        position = vector.create(x + corner[1], 0, z + corner[2]),
                        color = color,
                    })
                end
                for _, index in { 1, 2, 3, 1, 3, 4 } do
                    table.insert(indices, first + index)
                end
            end
        end
    end
    return graphics.new_mesh({ vertices = vertices, indices = indices })
end

local wall_mesh = graphics.new_cuboid(1, WALL_HEIGHT, 1)
local floor = floor_mesh()

-- Where the middle of the tile at `column`, `row` is, on the floor.
local function tile_middle(column: number, row: number): vector
    return vector.create(column + 0.5, 0, row + 0.5)
end

cellar = {
    mover = grid.new_mover(start_column, start_row, STEP_TIME),
    candles = candles,
    -- Each prop's kind, column and row, and the models by kind.
    props = props,
    models = models,
    -- The prop the mouse points at, if any.
    pointed = nil :: Prop?,
    -- How many candles are lit, and whether they all are.
    lit = 0,
    done = false,
    -- Which way the hero last walked across: their sprite faces it.
    facing_left = false,
    time = 0,
}

-- The hero's feet, between tiles while they walk.
function cellar.hero_position(): vector
    local column, row = cellar.mover:position()
    return tile_middle(column, row)
end

function cellar.view()
    local target = cellar.hero_position()
    return {
        camera = { position = target + CAMERA_OFFSET, target = target },
        lighting = LIGHTING,
    }
end

-- The prop the mouse points at, the nearest whose box the ray under it goes through, or nil.
function cellar.pointed_prop(): Prop?
    local x, y = input.mouse_position()
    if x == nil or y == nil then
        return nil
    end
    local origin, direction = graphics.ray(cellar.view(), x, y)
    local nearest: Prop?, nearest_distance = nil, math.huge
    for _, prop in cellar.props do
        local low, high = models[prop.kind]:bounds()
        local at = tile_middle(prop.column, prop.row)
        local distance = graphics.hit_box(origin, direction, low + at, high + at)
        if distance and distance < nearest_distance then
            nearest, nearest_distance = prop, distance
        end
    end
    return nearest
end

function update(dt: number)
    cellar.time += dt
    cellar.pointed = cellar.pointed_prop()
    local mover = cellar.mover
    mover:update(dt)
    if mover:is_moving() then
        return
    end
    local column, row = mover:tile()
    for _, candle in cellar.candles do
        if not candle.lit and candle.column == column and candle.row == row then
            candle.lit = true
            cellar.lit += 1
            cellar.done = cellar.lit == #cellar.candles
        end
    end
    for _, direction in DIRECTIONS do
        if input.held(direction) then
            if direction == "left" or direction == "right" then
                cellar.facing_left = direction == "left"
            end
            mover:step(direction, is_blocked)
            break
        end
    end
end

function draw()
    graphics.clear("#101014")
    local view = cellar.view()
    graphics.begin_3d(view)
    graphics.draw_mesh(floor, vector.zero, { tint = FLOOR })
    for _, wall in walls do
        local middle = tile_middle(wall.column, wall.row) + vector.create(0, WALL_HEIGHT / 2, 0)
        graphics.draw_mesh(wall_mesh, middle, { tint = WALL })
    end
    for _, prop in cellar.props do
        graphics.draw_model(models[prop.kind], tile_middle(prop.column, prop.row))
    end
    for i, candle in cellar.candles do
        local tint = UNLIT
        if candle.lit then
            -- Flickering, each candle in its own time.
            local glow = 0.9 + 0.1 * math.sin(cellar.time * 9 + i * 2)
            tint = graphics.color({ r = glow, g = glow * 0.85, b = glow * 0.45 })
        end
        -- Toward the back of its tile, so the hero standing on it is in front, not in the same
        -- place, where neither would hide the other evenly.
        local position = tile_middle(candle.column, candle.row) + CANDLE_OFFSET
        graphics.draw_billboard(CANDLE, position, tint, nil, CANDLE_HEIGHT)
    end
    local hero = if cellar.facing_left then HERO:flipped_x() else HERO
    graphics.draw_billboard(hero, cellar.hero_position(), nil, nil, HERO_HEIGHT)

    -- The HUD, over the view.
    local width = graphics.size()
    graphics.fill_rect(0, 0, width, 14, "#00000080")
    graphics.draw_text(font, `Candles lit: {cellar.lit}/{#cellar.candles}`, 4, 3, "#ffffff")
    local pointed = cellar.pointed
    if pointed then
        -- Its name, just over its top.
        local _, high = models[pointed.kind]:bounds()
        local top = tile_middle(pointed.column, pointed.row) + vector.create(0, high.y + 0.15, 0)
        local x, y = graphics.project(view, top)
        if x and y then
            local name_width = font:measure(pointed.kind)
            graphics.draw_text(font, pointed.kind, x - name_width / 2, y - 10, "#e0d8c8")
        end
    end
    if cellar.done then
        -- Over the hero's head, wherever they are on the screen.
        local x, y = graphics.project(view, cellar.hero_position() + vector.create(0, HERO_HEIGHT + 0.2, 0))
        if x and y then
            local message = "Every candle is lit!"
            local text_width = font:measure(message)
            graphics.draw_text(font, message, x - text_width / 2, y - 10, "#ffd870")
        end
    end
end
