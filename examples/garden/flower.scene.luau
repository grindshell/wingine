return {
    format = 3,
    sheets = {
        tiles = {
            path = "kenney_1-bit-pack.png",
            tile_width = 16,
            tile_height = 16,
        },
    },
    children = {
        {
            name = "flower",
            kind = "sprite",
            sheet = "tiles",
            cell = { 15, 6 },
            pivot = { 8, 12 },
            tint = "#e8a0b0",
        },
    },
}
