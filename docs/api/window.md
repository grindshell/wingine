# `window`

The game's window, and the game in it.

## `window.quit`

```lua
window.quit()
```

Closes the window once the frame has drawn, which ends the game. It works while the game loads, in
`update` and the tasks it resumes, in `gui`, and in [`close_requested`](globals.md#close_requested),
but not in `draw`. A game that saves on the way out writes its save before quitting:

```lua
input.bind("quit", { "Escape" })

function update(dt: number)
    if input.pressed("quit") then
        save.write("progress", progress)
        window.quit()
    end
end
```

## Asking before the window closes

When the player presses the window's close button, the engine calls the game's
[`close_requested`](globals.md#close_requested), if it has one, and the window closes unless it
returns `false`. A game with something to lose returns `false`, asks the player, and calls
`window.quit()` once they've answered:

```lua
local asking = false

function close_requested(): boolean
    asking = true
    return false
end

function update(dt: number)
    if asking and input.key_pressed("Y") then
        window.quit()
    elseif asking and input.key_pressed("N") then
        asking = false
    end
end
```

A game stopped by an error doesn't keep its window open: the close button closes it.

## Covering the screen

A window can cover the screen it's on in two ways:

- **`"borderless"`**: a window without borders as big as the screen, at the screen's own
  resolution. The system goes on showing notifications and other windows over it, and switching
  away from it is instant, so it's what most players want.
- **`"exclusive"`**: the screen given over to the game, at a video mode of the screen's: its own
  size and refresh rate if it has a mode of them, or else the largest and fastest. Switching to it
  and away from it can take a moment, as the display changes mode. Where the system has no such
  thing, as on Wayland and the web, the window is borderless instead.

[`fullscreen`](../project-structure.md#fullscreen) in `conf.luau` says how the window opens, and
a game changes it as it plays, as from a key or its options. The canvas grows with the window,
unless it has a fixed [`resolution`](../project-structure.md#resolution), which is
[scaled](#scaling-a-fixed-resolution) to the screen.

### `window.fullscreen`

```lua
window.fullscreen(): ("borderless" | "exclusive")?
```

Returns how the window covers the screen, or `nil` if it's a window of its own size: as it opened,
as the game last set it, or as the player changed it through the system, such as with macOS's
green button or a browser's Escape. A window that couldn't be exclusive is `"borderless"`.

### `window.set_fullscreen`

```lua
window.set_fullscreen(mode: ("borderless" | "exclusive")?)
```

Makes the window cover the screen as `mode` says, or a window of its own size again for `nil`,
once the frame has drawn. Wingine binds no key to it, so a game binds its own, as Pong binds F11:

```lua
input.bind("fullscreen", { "F11" })

function update(dt: number)
    if input.pressed("fullscreen") and window.fullscreen() then
        window.set_fullscreen(nil)
    elseif input.pressed("fullscreen") then
        window.set_fullscreen("borderless")
    end
end
```

On the web, a page may cover the screen only just after the player presses a key or clicks, so
call it in the `update` that sees the press.

## Scaling a fixed resolution

A game with a fixed [`resolution`](../project-structure.md#resolution) draws on a canvas of that
size, which the engine scales to fill the window, or the screen it covers, one of three ways:

| Scaling | How the canvas fills the window |
|---|---|
| `"whole"` | Scaled up by the largest whole number that fits, centered, with black bars filling the space left over. Every texel is a square of the same pixels, so pixel art stays crisp. The default. |
| `"fit"` | As large as fits, keeping its shape: it fills the window's height or its width, with bars along the other only where the window's shape differs from the canvas's. Texels stay square but differ by a pixel in size: at 4.5 times, a texel is 4 pixels wide in places and 5 in others. |
| `"stretch"` | Stretched to fill the window both ways, with no bars. Texels are as much wider than tall as the window's shape is wider than the canvas's. |

A 320x180 canvas fills a 1920x1080 screen six times over whichever it is. On a 1366x768 laptop,
`"whole"` shows it 4 times over, 1280x720, with bars around it; `"fit"` 4.27 times, filling the
screen's height; and on a 4:3 screen of 1024x768, `"fit"` fills its width, 3.2 times over, with
bars above and below, where `"stretch"` fills it all, its texels taller than they're wide.

[`scaling`](../project-structure.md#scaling) in `conf.luau` says how a game starts, and a game
changes it as it plays, as from its options. Without a fixed resolution, the canvas is the
window's size, and scaling changes nothing. The mouse's position is in canvas pixels however the
canvas is scaled.

### `window.scaling`

```lua
window.scaling(): "whole" | "fit" | "stretch"
```

Returns how a fixed resolution fills the window.

### `window.set_scaling`

```lua
window.set_scaling(scaling: "whole" | "fit" | "stretch")
```

Scales a fixed resolution by `scaling` from the frame that draws next.

These four work while the game loads, in `update` and the tasks it resumes, in `gui`, and in
`close_requested`, but not in `draw`. In [tests](testing.md) and scripted runs, the window's
fullscreen is only noted: its frames stay the size they were.

## `window.reloaded`

```lua
window.reloaded(): any
```

Returns a copy of what the game before this one returned from its
[`reloading`](globals.md#reloading), if the game was
[reloaded](../running-games.md#reloading-as-you-work) in its window, or `nil` when it started
fresh, or the game before had no `reloading`, or it failed. It works at any time, and each call
returns a copy of its own.

A game carries on where it was by making its state as it would to start, then copying what was
kept over it, so fields a new version of the game adds keep their starting values:

```lua
game = { level = 1, coins = 0, lives = 3 }

local kept = window.reloaded()
if kept then
    for key, value in kept do
        game[key] = value
    end
end

function reloading()
    return game
end
```
