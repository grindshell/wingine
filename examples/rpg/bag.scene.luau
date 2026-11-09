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
            name = "stats",
            kind = "panel",
            x = 4,
            y = 4,
            sheet = "frame",
            cell = { 0, 0 },
            slice = 2,
            padding = { 6, 5, 6, 5 },
            children = {
                {
                    name = "line",
                    kind = "node",
                    visible = false,
                    children = {
                        {
                            name = "name",
                            kind = "text",
                            text = "",
                            color = "#8a92b8",
                        },
                        {
                            name = "value",
                            kind = "text",
                            x = 52,
                            text = "",
                            color = "#f4f0e8",
                        },
                    },
                },
            },
        },
    },
}
