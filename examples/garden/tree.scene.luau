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
            name = "tree",
            kind = "sprite",
            sheet = "tiles",
            cell = { 0, 1 },
            pivot = { 8, 16 },
            tint = "#6aa84f",
            tags = { "solid" },
        },
    },
}
