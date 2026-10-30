-- The creatures the player fights, by kind. Maps place them, and `battle.luau` fights them.
--
-- Each has a sprite (a column and row of the tileset) and tint; what it's called in messages, and
-- in its panel; its stats; the coins it leaves, and an item if it drops one; what's said as the
-- battle starts and as it's beaten; and its moves, which it picks from at random in proportion
-- to their weights.
--
-- A move does damage if it has a power, and misses as often as its accuracy (from 0 to 1, 1 by
-- default) says. A move with an effect raises or lowers a stat by a stage instead: "raise_attack",
-- "raise_defense" or "lower_defense", the last on the player.

export type Move = {
    name: string,
    power: number?,
    accuracy: number?,
    effect: string?,
    weight: number,
}

export type Foe = {
    called: string,
    title: string,
    sprite: { number },
    tint: string,
    hp: number,
    attack: number,
    defense: number,
    speed: number,
    coins: number,
    drops: string?,
    -- Whether it's a boss, which the player can't run from.
    boss: boolean?,
    intro: string,
    defeat: string,
    moves: { Move },
}

local foes: { [string]: Foe } = {
    bat = {
        called = "the shade bat",
        title = "Shade Bat",
        sprite = { 26, 8 },
        tint = "#b07cd6",
        hp = 14,
        attack = 5,
        defense = 3,
        speed = 7,
        coins = 6,
        intro = "A shade bat swoops out of the dark!",
        defeat = "The shade bat flaps away over the trees.",
        moves = {
            { name = "Bite", power = 6, weight = 3 },
            { name = "Screech", effect = "lower_defense", weight = 1 },
        },
    },
    wisp = {
        called = "the murk wisp",
        title = "Murk Wisp",
        sprite = { 27, 8 },
        tint = "#7fd0c0",
        hp = 18,
        attack = 5,
        defense = 5,
        speed = 4,
        coins = 9,
        drops = "herb",
        intro = "A murk wisp drifts out from behind a dead tree!",
        defeat = "The murk wisp gutters out like a candle.",
        moves = {
            { name = "Chill", power = 7, accuracy = 0.9, weight = 3 },
            { name = "Fade", effect = "raise_defense", weight = 1 },
        },
    },
    gloamfang = {
        called = "Gloamfang",
        title = "Gloamfang",
        sprite = { 29, 8 },
        tint = "#a070d0",
        hp = 36,
        attack = 7,
        defense = 5,
        speed = 5,
        coins = 30,
        boss = true,
        intro = "Gloamfang uncoils from the shadow of the seal stone!",
        defeat = "Gloamfang shrieks and sinks back into the crack in the stone.",
        moves = {
            { name = "Claw", power = 8, weight = 3 },
            { name = "Crush", power = 11, accuracy = 0.8, weight = 2 },
            { name = "Howl", effect = "raise_attack", weight = 1 },
        },
    },
}

return foes
