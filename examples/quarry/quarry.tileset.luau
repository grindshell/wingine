return {
    format = 4,
    tile_width = 16,
    tile_height = 16,
    sheets = {
        tiles = {
            path = "kenney_1-bit-pack.png",
            tile_width = 16,
            tile_height = 16,
        },
    },
    tiles = {
        brick = {
            sheet = "tiles",
            cell = { 10, 17 },
            tint = "#8c7460",
            solid = true,
        },
        rock = {
            sheet = "tiles",
            cell = { 11, 15 },
            tint = "#b8a888",
            solid = true,
        },
        rubble = {
            sheet = "tiles",
            cell = { 19, 3 },
            tint = "#4c4450",
        },
        support = {
            sheet = "tiles",
            cell = { 21, 1 },
            tint = "#3c3640",
        },
    },
}
