-- Shrine Hill, north of the village, with the Stone Shrine at the top. See `village.luau` for
-- what maps hold.
return {
    name = "shrine",
    title = "Shrine Hill",
    -- The shrine has gone quiet.
    music = false,
    rows = {
        "TTTTTTTTTTTTTTTTTTTTTTTT",
        "TT..................DDTT",
        "TT..D..____YAY____....TT",
        "TT..T..___________.t..TT",
        "TT.T...___c_O_c___....TT",
        "TT.....________%__....TT",
        "TT...D.___________T...TT",
        "TTT....___________....TT",
        "TT.......P..:.DPt...T.TT",
        "TT.........D:t...T...DTT",
        "TT..........I.........TT",
        "TT..........:...D.t...TT",
        "TT..........:.....D...TT",
        "TT...T......:.....D...TT",
        "TTD.....T...I.........TT",
        "TT..........:.....D.t.TT",
        "TT..........:.........TT",
        "TT..........:D.t..DTt.TT",
        "TT..........:......D..TT",
        "TTTTTTTTTTTT:TTTTTTTTTTT",
    },
    places = {
        { at = { 12, 4 }, event = "seal_stone" },
    },
    exits = {
        { at = { 12, 19 }, to = "village", arrive = { 20, 1 }, facing = "down" },
    },
    -- Creatures from `foes.luau` that roam here during the hunt. They fight the player on
    -- catching them: standing beside them.
    foes = {
        {
            foe = "bat",
            at = { 8, 12 },
            facing = "right",
            route = {
                "right", "right", "right", "right", "right", "right", 1,
                "left", "left", "left", "left", "left", "left", 1,
            },
        },
        {
            foe = "wisp",
            at = { 6, 10 },
            route = { "down", "down", 1.5, "up", "up", 1.5 },
        },
        -- Guarding the seal stone.
        { foe = "gloamfang", at = { 12, 6 }, facing = "down" },
    },
}
