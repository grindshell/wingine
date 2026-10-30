-- Mossbrook, the village where the game begins. The symbols in `rows` are listed in `LEGEND`, in
-- `world.luau`. Cells are a column and a row, counting from 0 at the top-left.
return {
    name = "village",
    title = "Mossbrook",
    music = "sounds/music.ogg",
    rows = {
        "TTTTTTTTTTTTTTTTTTTT:TTTTTTTTTTTTTTTTTTT",
        "TTTTTTTTTTTTTTTTTTT.:.TTTTTTTTTTTTTTTTTT",
        "TFFFFFFFFFFFFFFFFFFFGFFFFFFFFFFFFFFFFFFT",
        "T.....*..........*..:s...**.T.........*T",
        "TT...*..............:.........R.R.R.R..T",
        "T.....H.....*..E....:..................T",
        "T..........*.*.::::::..........R.R*R...T",
        "T...*..T.*..........:............*.....T",
        "T.........H.......*.:.*..T.........*...T",
        "T.*..T....:....t..:::::..BSB.*.HT.H.*..T",
        "T.**......:......b::W::...:s*..:..:....T",
        "T......H..:......*:::::.*.:T..*:.T:....T",
        "T......:..:.........:.....:....:..:..*.T",
        "T......:..:.........:.....:...*:..:..*.T",
        "T..::::::::::::::::::::::::::::::::::..T",
        "T*..........:...:...:.t...*..T:.....*..T",
        "T...........:...:...:.........:........T",
        "T...~~~~~...H...Ht..:*t.TFFFFF.FFFFF*.*T",
        "T..~~~~~~~..........:....FwwwwwwwwwF..*T",
        "T..~~~~~~~..........:....FwwwwwwwwwF...T",
        "T..~~~~~~~.*..*.....:....FwwwwwwwwwF...T",
        "T....~~~~...........:t...FwwwwwwwwwF...T",
        "T..........*..T.....:....FFFFFFFFFFF...T",
        "T...........*......*:..................T",
        "TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT",
        "TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT",
    },
    -- Cells with descriptions of their own, events from `story.events`, or coins or an item to
    -- find the first time they're examined, and what's said then.
    places = {
        {
            at = { 21, 3 },
            text = "The signpost reads: “North: Shrine Hill. Mind the path at night.”",
        },
        { at = { 27, 10 }, event = "shop_sign" },
        {
            at = { 6, 5 },
            text = "Your cottage. It's too nice a day to stay indoors.",
            find = {
                coins = 10,
                text = "Your cottage. You duck inside for your savings, from the jar by the door: "
                    .. "10 coins.",
            },
        },
        {
            at = { 15, 9 },
            text = "A thick bush, heavy with berries. They look sour.",
            find = { coins = 5, text = "Something glints in the bush. You fish out 5 coins." },
        },
        {
            at = { 17, 10 },
            text = "A bench, worn smooth by years of gossip.",
            find = { coins = 3, text = "Someone's left 3 coins on the bench. Finders keepers." },
        },
        { at = { 20, 10 }, event = "well" },
    },
    -- Cells that lead to another map, arriving on a cell there facing a direction.
    exits = {
        { at = { 20, 0 }, to = "shrine", arrive = { 12, 18 }, facing = "up" },
    },
    -- Characters: their sprite's column and row in the tileset, where they stand, and either a
    -- conversation from `story.talk` or a description. A route is a loop of directions to walk
    -- and seconds to wait.
    characters = {
        {
            name = "Elder Rowan",
            sprite = { 24, 0 },
            tint = "#c8b8e8",
            at = { 16, 7 },
            facing = "down",
            talk = "elder",
        },
        {
            name = "Maple",
            sprite = { 25, 1 },
            tint = "#9fd0c8",
            at = { 24, 14 },
            facing = "right",
            talk = "maple",
            route = {
                "right", "right", "right", "right", "right", 2,
                "left", "left", "left", "left", "left", 2,
            },
        },
        {
            name = "Hazel",
            sprite = { 26, 1 },
            tint = "#e8a0b0",
            at = { 25, 10 },
            facing = "down",
            talk = "hazel",
        },
        {
            name = "Pip",
            sprite = { 30, 3 },
            tint = "#f2c14e",
            at = { 11, 18 },
            facing = "left",
            talk = "pip",
        },
        {
            name = "a duck",
            sprite = { 25, 7 },
            tint = "#f4f0e8",
            at = { 6, 16 },
            facing = "right",
            text = "The duck quacks at you, unimpressed.",
            route = { "right", "right", 1.5, "left", "left", 1.5 },
        },
        {
            name = "a cat",
            sprite = { 30, 7 },
            tint = "#e0a060",
            at = { 28, 11 },
            facing = "left",
            text = "The shopkeeper's cat is asleep in a patch of sun. Best not to wake it.",
        },
    },
}
