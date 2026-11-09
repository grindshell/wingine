-- The game's settings, which the engine reads before the game starts.
return {
    title = "Wingine - bonfire",
    width = 640,
    height = 360,
    -- Pixel art, scaled up by whole pixels.
    resolution = { width = 320, height = 180 },
    -- Particles move by the same steps every update, so a run looks the same on every machine.
    update_rate = 60,
}
