return {
    format = 4,
    sheets = {
        parts = {
            path = "orchard.png",
            tile_width = 16,
            tile_height = 16,
        },
    },
    children = {
        {
            name = "shadow",
            kind = "sprite",
            sheet = "parts",
            cell = { 6, 1 },
            pivot = { 8, 8 },
        },
        {
            name = "hips",
            kind = "node",
            y = -14,
            children = {
                {
                    name = "leg_b",
                    kind = "sprite",
                    x = -1,
                    sheet = "parts",
                    cell = { 3, 1 },
                    pivot = { 7.5, 3 },
                    tint = "#b4b4b4",
                    children = {
                        {
                            name = "shin_b",
                            kind = "sprite",
                            y = 6,
                            sheet = "parts",
                            cell = { 4, 1 },
                            pivot = { 7.5, 3 },
                            tint = "#b4b4b4",
                        },
                    },
                },
                {
                    name = "body",
                    kind = "node",
                    children = {
                        {
                            name = "arm_b",
                            kind = "sprite",
                            y = -7,
                            sheet = "parts",
                            cell = { 1, 1 },
                            pivot = { 8, 3 },
                            tint = "#b4b4b4",
                            children = {
                                {
                                    name = "forearm_b",
                                    kind = "sprite",
                                    y = 5,
                                    sheet = "parts",
                                    cell = { 2, 1 },
                                    pivot = { 8, 3 },
                                    tint = "#b4b4b4",
                                },
                            },
                        },
                        {
                            name = "chest",
                            kind = "sprite",
                            sheet = "parts",
                            cell = { 0, 1 },
                            pivot = { 8, 11 },
                        },
                        {
                            name = "head",
                            kind = "sprite",
                            y = -8,
                            sheet = "parts",
                            cell = { 0, 0 },
                            pivot = { 8, 14 },
                            children = {
                                {
                                    name = "face",
                                    kind = "sprite",
                                    sheet = "parts",
                                    cell = { 1, 0 },
                                    pivot = { 8, 14 },
                                },
                                {
                                    name = "basket",
                                    kind = "sprite",
                                    y = -11,
                                    sheet = "parts",
                                    cell = { 5, 1 },
                                    pivot = { 8, 14 },
                                    children = {
                                        {
                                            name = "mouth",
                                            kind = "area",
                                            width = 12,
                                            height = 5,
                                            pivot = { 6, 7 },
                                        },
                                    },
                                },
                            },
                        },
                    },
                },
                {
                    name = "leg_f",
                    kind = "sprite",
                    x = 1,
                    sheet = "parts",
                    cell = { 3, 1 },
                    pivot = { 7.5, 3 },
                    children = {
                        {
                            name = "shin_f",
                            kind = "sprite",
                            y = 6,
                            sheet = "parts",
                            cell = { 4, 1 },
                            pivot = { 7.5, 3 },
                        },
                    },
                },
                {
                    name = "body_front",
                    kind = "node",
                    children = {
                        {
                            name = "arm_f",
                            kind = "sprite",
                            y = -7,
                            sheet = "parts",
                            cell = { 1, 1 },
                            pivot = { 8, 3 },
                            children = {
                                {
                                    name = "forearm_f",
                                    kind = "sprite",
                                    y = 5,
                                    sheet = "parts",
                                    cell = { 2, 1 },
                                    pivot = { 8, 3 },
                                },
                            },
                        },
                    },
                },
            },
        },
    },
    clips = {
        blink = {
            duration = 3.6,
            loop = true,
            tracks = {
                {
                    node = "hips/body/head/face",
                    property = "cell",
                    keys = {
                        { 0, { 1, 0 } },
                        { 3.4, { 2, 0 } },
                        { 3.55, { 1, 0 } },
                    },
                },
            },
        },
        catch = {
            duration = 0.45,
            tracks = {
                {
                    node = "hips",
                    property = "y",
                    keys = {
                        { 0, -14 },
                        { 0.12, -12.5 },
                        { 0.3, -14.5 },
                        { 0.45, -14 },
                    },
                },
                {
                    node = "hips/body/head",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                        { 0.15, -0.15 },
                        { 0.45, 0 },
                    },
                },
                {
                    node = "hips/body/head/face",
                    property = "cell",
                    keys = {
                        { 0, { 3, 0 } },
                    },
                },
                {
                    node = "hips/body/head/basket",
                    property = "y",
                    keys = {
                        { 0, -11 },
                        { 0.1, -10 },
                        { 0.22, -12 },
                        { 0.35, -11 },
                    },
                },
                {
                    node = "hips/body/head/basket",
                    property = "scale_x",
                    keys = {
                        { 0, 1 },
                        { 0.1, 1.15 },
                        { 0.22, 0.95 },
                        { 0.35, 1 },
                    },
                },
                {
                    node = "hips/body/head/basket",
                    property = "scale_y",
                    keys = {
                        { 0, 1 },
                        { 0.1, 0.8 },
                        { 0.22, 1.1 },
                        { 0.35, 1 },
                    },
                },
                {
                    node = "hips/body_front/arm_f",
                    property = "rotation",
                    keys = {
                        { 0, -0.05 },
                        { 0.15, -2.7 },
                        { 0.35, -2.7 },
                        { 0.45, -0.05 },
                    },
                },
                {
                    node = "hips/body_front/arm_f/forearm_f",
                    property = "rotation",
                    keys = {
                        { 0, -0.3 },
                        { 0.15, -0.5 },
                        { 0.35, -0.5 },
                        { 0.45, -0.3 },
                    },
                },
                {
                    node = "hips/body/arm_b",
                    property = "rotation",
                    keys = {
                        { 0, 0.1 },
                        { 0.15, -2.9 },
                        { 0.35, -2.9 },
                        { 0.45, 0.1 },
                    },
                },
                {
                    node = "hips/body/arm_b/forearm_b",
                    property = "rotation",
                    keys = {
                        { 0, -0.25 },
                        { 0.15, -0.4 },
                        { 0.35, -0.4 },
                        { 0.45, -0.25 },
                    },
                },
            },
        },
        idle = {
            duration = 2.4,
            loop = true,
            tracks = {
                {
                    node = "hips",
                    property = "y",
                    keys = {
                        { 0, -14 },
                        { 1.2, -13.5 },
                        { 2.4, -14 },
                    },
                },
                {
                    node = "hips/leg_b",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                    },
                },
                {
                    node = "hips/leg_b/shin_b",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                    },
                },
                {
                    node = "hips/leg_f",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                    },
                },
                {
                    node = "hips/leg_f/shin_f",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                    },
                },
                {
                    node = "hips/body",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                        { 1.2, 0.03 },
                        { 2.4, 0 },
                    },
                },
                {
                    node = "hips/body_front",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                        { 1.2, 0.03 },
                        { 2.4, 0 },
                    },
                },
                {
                    node = "hips/body/arm_b",
                    property = "rotation",
                    keys = {
                        { 0, 0.1 },
                        { 1.2, 0.18 },
                        { 2.4, 0.1 },
                    },
                },
                {
                    node = "hips/body/arm_b/forearm_b",
                    property = "rotation",
                    keys = {
                        { 0, -0.25 },
                        { 1.2, -0.35 },
                        { 2.4, -0.25 },
                    },
                },
                {
                    node = "hips/body_front/arm_f",
                    property = "rotation",
                    keys = {
                        { 0, -0.05 },
                        { 1.2, 0.03 },
                        { 2.4, -0.05 },
                    },
                },
                {
                    node = "hips/body_front/arm_f/forearm_f",
                    property = "rotation",
                    keys = {
                        { 0, -0.3 },
                        { 1.2, -0.4 },
                        { 2.4, -0.3 },
                    },
                },
                {
                    node = "hips/body/head",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                        { 1.2, -0.04 },
                        { 2.4, 0 },
                    },
                },
                {
                    node = "hips/body/head/basket",
                    property = "x",
                    keys = {
                        { 0, 0 },
                    },
                },
                {
                    node = "hips/body/head/basket",
                    property = "y",
                    keys = {
                        { 0, -11 },
                    },
                },
                {
                    node = "hips/body/head/basket",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                    },
                },
                {
                    node = "hips/body/head/basket",
                    property = "scale_x",
                    keys = {
                        { 0, 1 },
                    },
                },
                {
                    node = "hips/body/head/basket",
                    property = "scale_y",
                    keys = {
                        { 0, 1 },
                    },
                },
            },
        },
        miss = {
            duration = 0.9,
            tracks = {
                {
                    node = "hips",
                    property = "y",
                    keys = {
                        { 0, -14 },
                        { 0.3, -13 },
                        { 0.9, -14 },
                    },
                },
                {
                    node = "hips/body/head",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                        { 0.15, 0.2 },
                        { 0.35, -0.2 },
                        { 0.55, 0.2 },
                        { 0.75, -0.1 },
                        { 0.9, 0 },
                    },
                },
                {
                    node = "hips/body/head/face",
                    property = "cell",
                    keys = {
                        { 0, { 4, 0 } },
                    },
                },
            },
        },
        slump = {
            duration = 1,
            tracks = {
                {
                    node = "hips",
                    property = "y",
                    keys = {
                        { 0, -14 },
                        { 0.35, -11 },
                        { 0.6, -2 },
                        { 0.75, -3 },
                        { 1, -2.5 },
                    },
                },
                {
                    node = "hips/leg_f",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                        { 0.6, -1.5 },
                    },
                },
                {
                    node = "hips/leg_f/shin_f",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                        { 0.6, 0.25 },
                    },
                },
                {
                    node = "hips/leg_b",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                        { 0.6, -1.4 },
                    },
                },
                {
                    node = "hips/leg_b/shin_b",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                        { 0.6, 0.3 },
                    },
                },
                {
                    node = "hips/body",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                        { 0.6, 0.35 },
                        { 1, 0.3 },
                    },
                },
                {
                    node = "hips/body_front",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                        { 0.6, 0.35 },
                        { 1, 0.3 },
                    },
                },
                {
                    node = "hips/body/head",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                        { 0.6, 0.35 },
                    },
                },
                {
                    node = "hips/body/head/face",
                    property = "cell",
                    keys = {
                        { 0, { 4, 0 } },
                    },
                },
                {
                    node = "hips/body_front/arm_f",
                    property = "rotation",
                    keys = {
                        { 0, -0.05 },
                        { 0.6, -0.6 },
                    },
                },
                {
                    node = "hips/body_front/arm_f/forearm_f",
                    property = "rotation",
                    keys = {
                        { 0, -0.3 },
                        { 0.6, -0.4 },
                    },
                },
                {
                    node = "hips/body/arm_b",
                    property = "rotation",
                    keys = {
                        { 0, 0.1 },
                        { 0.6, -0.4 },
                    },
                },
                {
                    node = "hips/body/arm_b/forearm_b",
                    property = "rotation",
                    keys = {
                        { 0, -0.25 },
                        { 0.6, -0.3 },
                    },
                },
                {
                    node = "hips/body/head/basket",
                    property = "x",
                    keys = {
                        { 0, 0 },
                        { 0.8, 7 },
                    },
                },
                {
                    node = "hips/body/head/basket",
                    property = "y",
                    keys = {
                        { 0, -11 },
                        { 0.8, -6 },
                    },
                },
                {
                    node = "hips/body/head/basket",
                    property = "rotation",
                    keys = {
                        { 0, 0 },
                        { 0.5, 0.6 },
                        { 0.8, 1.4 },
                    },
                },
            },
        },
        walk = {
            duration = 0.4,
            loop = true,
            tracks = {
                {
                    node = "hips",
                    property = "y",
                    keys = {
                        { 0, -13.5 },
                        { 0.1, -14.5 },
                        { 0.2, -13.5 },
                        { 0.3, -14.5 },
                        { 0.4, -13.5 },
                    },
                },
                {
                    node = "hips/leg_f",
                    property = "rotation",
                    keys = {
                        { 0, -0.6 },
                        { 0.2, 0.6 },
                        { 0.4, -0.6 },
                    },
                },
                {
                    node = "hips/leg_f/shin_f",
                    property = "rotation",
                    keys = {
                        { 0, 0.1 },
                        { 0.1, 0.15 },
                        { 0.2, 0.35 },
                        { 0.3, 1 },
                        { 0.4, 0.1 },
                    },
                },
                {
                    node = "hips/leg_b",
                    property = "rotation",
                    keys = {
                        { 0, 0.6 },
                        { 0.2, -0.6 },
                        { 0.4, 0.6 },
                    },
                },
                {
                    node = "hips/leg_b/shin_b",
                    property = "rotation",
                    keys = {
                        { 0, 0.35 },
                        { 0.1, 1 },
                        { 0.2, 0.1 },
                        { 0.3, 0.15 },
                        { 0.4, 0.35 },
                    },
                },
                {
                    node = "hips/body",
                    property = "rotation",
                    keys = {
                        { 0, 0.08 },
                    },
                },
                {
                    node = "hips/body_front",
                    property = "rotation",
                    keys = {
                        { 0, 0.08 },
                    },
                },
                {
                    node = "hips/body_front/arm_f",
                    property = "rotation",
                    keys = {
                        { 0, 0.45 },
                        { 0.2, -0.45 },
                        { 0.4, 0.45 },
                    },
                },
                {
                    node = "hips/body_front/arm_f/forearm_f",
                    property = "rotation",
                    keys = {
                        { 0, -0.15 },
                        { 0.2, -0.55 },
                        { 0.4, -0.15 },
                    },
                },
                {
                    node = "hips/body/arm_b",
                    property = "rotation",
                    keys = {
                        { 0, -0.45 },
                        { 0.2, 0.45 },
                        { 0.4, -0.45 },
                    },
                },
                {
                    node = "hips/body/arm_b/forearm_b",
                    property = "rotation",
                    keys = {
                        { 0, -0.55 },
                        { 0.2, -0.15 },
                        { 0.4, -0.55 },
                    },
                },
                {
                    node = "hips/body/head",
                    property = "rotation",
                    keys = {
                        { 0, -0.06 },
                    },
                },
                {
                    node = "hips/body/head/basket",
                    property = "rotation",
                    keys = {
                        { 0, 0.05 },
                        { 0.2, -0.05 },
                        { 0.4, 0.05 },
                    },
                },
            },
        },
    },
}
