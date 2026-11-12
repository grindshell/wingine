-- A cellar in 2.5D, written entirely in Luau: the hero walks a dark cellar tile by tile and lights
-- its candles by standing on them. The cellar is a scene, `cellar.scene.luau`, which the editor
-- edits: a 3D view holding a floor, walls of cuboids tagged `solid`, barrels, crates and a table in
-- the way, models made in Blender tagged `solid` and `prop`, candles tagged `candle`, and the hero.
-- The candles and the hero are sprites standing among them, billboards that turn to face the
-- camera, which follows the hero from above and behind. Each candle holds a `flame`, a light
-- that's out until the candle is lit, and then lights the cellar around it. Walk with the arrow keys or WASD, and point
-- at a prop with the mouse to name it.
--
-- A tile is a world unit, with columns along `x` and rows along `z`, so up the map is away from
-- the camera, and each node of the view stands on the tile its position is in.
--
-- The game's state is the global `cellar`, for tests.
--
-- Run with `cargo run -- examples/cellar`.

local font = graphics.default_font()
local level = scene.load("cellar.scene.luau")
local view = level:find("view") :: SceneNode
local hero = view:find("hero") :: SceneNode

-- Seconds a step takes.
local STEP_TIME = 0.18
-- How tall the hero is, as the scene has them.
local _, HERO_HEIGHT = hero:size()
-- Where the camera is from the hero, whom it looks at.
local CAMERA_OFFSET = vector.create(0, 7, 3.5)

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

type Prop = { node: SceneNode, kind: string, column: number, row: number }
type Candle = { node: SceneNode, flame: SceneNode, column: number, row: number, lit: boolean }

-- The column and row of the tile a node of the view stands on.
local function tile_of(node: SceneNode): (number, number)
    local position = node:world_position_3d()
    return math.floor(position.x), math.floor(position.z)
end

-- The tiles nothing can walk onto, by `column,row`.
local solid: { [string]: boolean } = {}
for _, node in view:tagged("solid") do
    local column, row = tile_of(node)
    solid[`{column},{row}`] = true
end

local candles: { Candle } = {}
for _, node in view:tagged("candle") do
    local column, row = tile_of(node)
    -- Its light, which the scene has lit, so the editor shows what it lights.
    local flame = node:find("flame") :: SceneNode
    flame:set_visible(false)
    table.insert(candles, { node = node, flame = flame, column = column, row = row, lit = false })
end

local props: { Prop } = {}
for _, node in view:tagged("prop") do
    local column, row = tile_of(node)
    -- Named for what they are, numbered when there are several, such as `barrel_2`.
    local kind = string.gsub(node:name(), "_%d+$", "")
    table.insert(props, { node = node, kind = kind, column = column, row = row })
end

-- Whether the hero can't walk onto the tile: a wall or a prop. Walls surround the cellar.
local function is_blocked(column: number, row: number): boolean
    return solid[`{column},{row}`] == true
end

-- Where the middle of the tile at `column`, `row` is, on the floor.
local function tile_middle(column: number, row: number): vector
    return vector.create(column + 0.5, 0, row + 0.5)
end

local start_column, start_row = tile_of(hero)

cellar = {
    mover = grid.new_mover(start_column, start_row, STEP_TIME),
    level = level,
    -- The level's 3D view.
    world = view,
    hero = hero,
    candles = candles,
    -- Each prop's node, kind, column and row.
    props = props,
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

-- The camera following the hero, and the view's lighting.
function cellar.view()
    local target = cellar.hero_position()
    return {
        camera = { position = target + CAMERA_OFFSET, target = target },
        lighting = view:lighting(),
    }
end

-- The prop the mouse points at, the nearest whose box the ray under it goes through, or nil.
function cellar.pointed_prop(): Prop?
    local x, y = input.mouse_position()
    if x == nil or y == nil then
        return nil
    end
    for _, node in view:pick(x, y) do
        for _, prop in cellar.props do
            if prop.node == node then
                return prop
            end
        end
    end
    return nil
end

function update(dt: number)
    cellar.time += dt
    view:set_camera(cellar.view().camera)
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
            candle.flame:set_visible(true)
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
    view:set_camera(cellar.view().camera)
    for i, candle in cellar.candles do
        if candle.lit then
            -- Flickering, each candle in its own time.
            local glow = 0.9 + 0.1 * math.sin(cellar.time * 9 + i * 2)
            candle.node:set_tint(graphics.color({ r = glow, g = glow * 0.85, b = glow * 0.45 }))
            candle.flame:set_light({ energy = glow })
        end
    end
    hero:set_position_3d(cellar.hero_position())
    local width, height = hero:size()
    hero:set_size(if cellar.facing_left then -math.abs(width) else math.abs(width), height)
    graphics.draw_scene(level)

    -- The HUD, over the view.
    local canvas_width = graphics.size()
    graphics.fill_rect(0, 0, canvas_width, 14, "#00000080")
    graphics.draw_text(font, `Candles lit: {cellar.lit}/{#cellar.candles}`, 4, 3, "#ffffff")
    local pointed = cellar.pointed
    if pointed then
        -- Its name, just over its top.
        local low, high = pointed.node:bounds_3d()
        if low and high then
            local top = vector.create((low.x + high.x) / 2, high.y + 0.15, (low.z + high.z) / 2)
            local x, y = view:project(top)
            if x and y then
                local name_width = font:measure(pointed.kind)
                graphics.draw_text(font, pointed.kind, x - name_width / 2, y - 10, "#e0d8c8")
            end
        end
    end
    if cellar.done then
        -- Over the hero's head, wherever they are on the screen.
        local x, y = view:project(cellar.hero_position() + vector.create(0, HERO_HEIGHT + 0.2, 0))
        if x and y then
            local message = "Every candle is lit!"
            local text_width = font:measure(message)
            graphics.draw_text(font, message, x - text_width / 2, y - 10, "#ffd870")
        end
    end
end
