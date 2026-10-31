-- The game's sprites, and the kinds of bullet it fires.
--
-- Characters come from Kenney's 1-bit Pack (<https://kenney.nl/assets/1-bit-pack>, CC0), white
-- on transparent, so they're tinted when drawn. `bullets.png` was drawn for this game: in an 8x8
-- grid, pellets in eight colors along the top, then the hitbox and a spark; in a 16x16 grid,
-- orbs in row 1, rice in row 2 (pointing right, so it turns to the way it's heading), and the
-- player's shot in row 3.

local art = {}

local tiles = graphics.load_sprite_sheet("kenney_1-bit-pack.png", 16, 16)
local small = graphics.load_sprite_sheet("bullets.png", 8, 8)
local large = graphics.load_sprite_sheet("bullets.png", 16, 16)

art.player = tiles:sprite(31, 2)
art.ghost = tiles:sprite(27, 8)
art.bat = tiles:sprite(26, 8)
art.octopus = tiles:sprite(25, 8)
art.boss = tiles:sprite(27, 2)
art.hitbox = small:sprite(0, 1)
art.spark = small:sprite(1, 1)
art.shot = large:sprite(0, 3)

art.COLORS = { "red", "orange", "yellow", "green", "cyan", "blue", "purple", "white" }

type Look = { sprite: any, radius: number, rotates: boolean }

-- Each shape's sprites, by color, how close a bullet must come to hit, and whether it turns to
-- the way it's heading.
local SHAPES = {
    { name = "pellet", sprite = function(i) return small:sprite(i, 0) end, radius = 2.5 },
    { name = "orb", sprite = function(i) return large:sprite(i, 1) end, radius = 5 },
    { name = "rice", sprite = function(i) return large:sprite(i, 2) end, radius = 2.5, rotates = true },
}

-- Bullet kinds by shape and color, such as `art.kinds.rice.blue`: numbers a bullet system
-- returns when they're registered, in this order.
art.kinds = {} :: { [string]: { [string]: number } }
local looks: { Look } = {}
for _, shape in SHAPES do
    art.kinds[shape.name] = {}
    for i, color in art.COLORS do
        table.insert(looks, {
            sprite = shape.sprite(i - 1),
            radius = shape.radius,
            rotates = shape.rotates == true,
        })
        art.kinds[shape.name][color] = #looks
    end
end

-- Registers every kind of bullet with `system`, so that the numbers in `art.kinds` mean the same
-- in every system.
function art.register(system)
    for number, look in looks do
        local kind = system.kind(look.sprite, look.radius, look.rotates)
        assert(kind == number, "bullet kinds must have the same numbers in every system")
    end
end

return art
