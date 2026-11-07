-- The game's settings, which the engine reads before the game starts.
return {
    title = "Wingine - course",
    width = 960,
    height = 540,
    -- The 3D view renders at these pixels too, chunky as the HUD's text is.
    resolution = { width = 480, height = 270 },
    -- The physics world steps once an update, by the same time every update, so a run replays
    -- the same on every machine.
    update_rate = 60,
}
