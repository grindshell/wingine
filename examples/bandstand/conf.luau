-- The game's settings, which the engine reads before the game starts.
return {
    title = "Wingine - bandstand",
    width = 640,
    height = 360,
    -- Pixel art, scaled up by whole pixels.
    resolution = { width = 320, height = 180 },
    -- Updates come at the same steps every run, so the song's beats land on the same ones.
    update_rate = 60,
}
