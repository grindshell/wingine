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
            name = "question",
            kind = "stack",
            anchor = { 1, 1 },
            visible = false,
            direction = "row",
            gap = 1,
            children = {
                {
                    name = "note",
                    kind = "panel",
                    sheet = "frame",
                    cell = { 0, 0 },
                    slice = 2,
                    width = 132,
                    padding = { 6, 5, 6, 5 },
                    children = {
                        {
                            name = "text",
                            kind = "text",
                            text = "",
                            wrap = 120,
                            color = "#f4f0e8",
                        },
                    },
                },
                {
                    name = "choices",
                    kind = "panel",
                    sheet = "frame",
                    cell = { 0, 0 },
                    slice = 2,
                    direction = "row",
                    gap = 12,
                    padding = { 6, 5, 6, 5 },
                    children = {
                        {
                            name = "labels",
                            kind = "stack",
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
                        {
                            name = "details",
                            kind = "stack",
                            children = {
                                {
                                    name = "detail",
                                    kind = "text",
                                    anchor = { 1, 0 },
                                    visible = false,
                                    text = "",
                                    color = "#8a92b8",
                                },
                            },
                        },
                    },
                },
            },
        },
    },
}
