-- The game's settings, which the engine reads before the game starts.
return {
    title = "Wingine - quarry",
    width = 960,
    height = 540,
    -- Pixel art, scaled up by whole pixels.
    resolution = { width = 480, height = 270 },
    -- The physics world steps once an update, by the same time every update, so a run replays
    -- the same on every machine.
    update_rate = 60,
}
