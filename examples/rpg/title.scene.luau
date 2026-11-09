return {
    format = 4,
    sheets = {
        frame = {
            path = "frame.png",
            tile_width = 5,
            tile_height = 5,
        },
    },
    children = {
        {
            name = "name",
            kind = "text",
            y = 28,
            anchor = { 0.5, 0 },
            text = "The Stone Shrine",
            font = "title.font.luau",
            align = "center",
            color = "#f2c14e",
        },
        {
            name = "tagline",
            kind = "text",
            y = 50,
            anchor = { 0.5, 0 },
            text = "A tale of Mossbrook",
            align = "center",
            color = "#8a92b8",
        },
        {
            name = "menu",
            kind = "panel",
            y = 80,
            anchor = { 0.5, 0 },
            sheet = "frame",
            cell = { 0, 0 },
            slice = 2,
            padding = { 6, 5, 6, 5 },
            children = {
                {
                    name = "choice",
                    kind = "node",
                    visible = false,
                    children = {
                        {
                            name = "pointer",
                            kind = "text",
                            text = ">",
                            color = "#f2c14e",
                        },
                        {
                            name = "label",
                            kind = "text",
                            x = 8,
                            text = "",
                            color = "#8a92b8",
                        },
                    },
                },
            },
        },
    },
}
