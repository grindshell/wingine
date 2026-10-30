-- The game's settings, which the engine reads before the game starts. The game's modules use
-- the resolution to lay out the screen.
return {
    title = "Wingine - rpg",
    -- 20 by 11 tiles, scaling by whole numbers to fill 720p, 1080p and 1440p screens.
    resolution = { width = 320, height = 180 },
}
