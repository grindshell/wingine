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
