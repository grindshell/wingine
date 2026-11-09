return {
    format = 3,
    sheets = {
        frame = {
            path = "frame.png",
            tile_width = 5,
            tile_height = 5,
        },
    },
    children = {
        {
            name = "heading",
            kind = "text",
            y = 10,
            anchor = { 0.5, 0 },
            text = "Controls",
            font_scale = 2,
            align = "center",
            color = "#f2c14e",
        },
        {
            name = "menu",
            kind = "panel",
            y = 32,
            anchor = { 0.5, 0 },
            sheet = "frame",
            cell = { 0, 0 },
            slice = 2,
            padding = { 6, 5, 6, 5 },
            children = {
                {
                    name = "row",
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
                        {
                            name = "keys",
                            kind = "text",
                            x = 64,
                            text = "",
                            color = "#8a92b8",
                        },
                    },
                },
            },
        },
        {
            name = "hint",
            kind = "text",
            y = 160,
            anchor = { 0.5, 0 },
            text = "Pick one, then press its new key. Escape stops.",
            align = "center",
            color = "#8a92b8",
        },
    },
}
