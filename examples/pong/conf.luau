-- The game's settings, which the engine reads before the game starts. `main.luau` uses the
-- resolution as the size of the field.
return {
    title = "Wingine - pong",
    -- 16 pixels square, which the window's title bar and taskbar button show scaled by whole
    -- numbers, and an exported game's executable too, on Windows.
    icon = "icon.png",
    resolution = { width = 320, height = 180 },
    -- Fills 720p, 1080p and 1440p screens four, six and eight times over, and others as far as
    -- it fits, by fractions, so a laptop's 1366x768 shows it 4.27 times over, filling its height.
    scaling = "fit",
}
