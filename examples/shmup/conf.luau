-- The game's settings, which the engine reads before the game starts. `main.luau` uses the
-- resolution too.
return {
    title = "Wingine - shmup",
    width = 960,
    height = 720,
    -- Scales by whole numbers: 2x in the window it opens with, 3x full screen at 1080p.
    resolution = { width = 480, height = 360 },
    -- Bullet hells play by the frame: the game updates 60 times a second, whatever the display.
    update_rate = 60,
    -- Bullets in Rust, when the library is there: see `examples/native-danmaku`. Without it, the
    -- game runs its bullets in Luau.
    libraries = { { name = "danmaku", optional = true } },
}
