-- The playfield, where the player, enemies and bullets are. Its coordinates are pixels from its
-- top-left corner, which is at `X`, `Y` on the canvas.

local conf = require("./conf")

local field = {
    WIDTH = 288,
    HEIGHT = 336,
    X = 16,
    Y = 12,
    -- How far outside the field a bullet goes before it's removed.
    BULLET_MARGIN = 32,
}

-- Draws whatever comes next in field coordinates.
function field.camera()
    graphics.set_camera(
        conf.resolution.width / 2 - field.X,
        conf.resolution.height / 2 - field.Y
    )
end

-- Whether `x`, `y` is on the field, or within `margin` of it.
function field.contains(x: number, y: number, margin: number?): boolean
    local m = margin or 0
    return x >= -m and x <= field.WIDTH + m and y >= -m and y <= field.HEIGHT + m
end

return field
