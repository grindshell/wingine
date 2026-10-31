# Built-ins

The functions a game defines for the engine to call, and the functions the engine adds to every
script's globals. See [Project structure](../project-structure.md#mainluau) for how a game runs.

## The game's functions

### `update`

```lua
function update(dt: number)
```

Defined by the game, and called by the engine to move the game on: once a frame with `dt`, the
seconds since the frame before, or with an [update rate](../project-structure.md#update_rate),
that many times a second, each time with `dt = 1 / update_rate`. A frame never updates for more
than a quarter of a second, however long it took.

Read input, move things, and play sounds here. Optional.

### `draw`

```lua
function draw()
```

Defined by the game, and called by the engine once a frame, after the frame's updates, to draw
the game. The canvas starts each frame cleared to black, with no camera set. Drawing functions
work only here. Optional.

An error in either function stops the game and shows the error in the window.

## Tasks

A task is a function that runs alongside the game across many frames, pausing with `wait` or
`wait_until`. The engine resumes waiting tasks every update, after `update`, in the order they
started. `wait` and `wait_until` work only in tasks: called from `update`, `draw` or the top level
of a script, they raise an error, since those must run to the end.

### `spawn`

```lua
spawn(f: (...any) -> (), ...: any)
```

Starts `f(...)` as a task, running it right away until it first waits or finishes, then returns.
From then on, the engine resumes it every update until it finishes.

A task that raises an error stops, and the error is logged. The script that spawned it, and every
other task, carries on.

```lua
local function patrol(guard)
    while true do
        guard.x += 16
        wait(1)
        guard.x -= 16
        wait(1)
    end
end

spawn(patrol, guards[1])
spawn(patrol, guards[2])
```

A task can't be stopped from outside. To stop one, have it check a flag and return.

### `wait`

```lua
wait(seconds: number?)
```

Pauses the task for `seconds` of game time, then carries on. Without `seconds`, it waits for the
next update. Waits are counted in updates, so a task waits a whole number of them: at 60 updates
a second, `wait(0.5)` waits 30 updates.

### `wait_until`

```lua
wait_until(condition: () -> boolean)
```

Pauses the task until `condition()` returns true, checking once each update. If it's true already,
the task carries on at once.

```lua
-- Shows a message until the player dismisses it.
local function show(text: string)
    message = text
    -- A press lasts the whole update, so without this, the press that dismissed the message
    -- before would dismiss this one too.
    wait()
    wait_until(function()
        return input.pressed("confirm")
    end)
    message = nil
end

spawn(function()
    show("The door is locked.")
    show("Maybe there's a key somewhere.")
end)
```

## Other built-ins

### `print`

```lua
print(...: any)
```

Logs its arguments, converted to strings and separated by tabs, as a line on standard error
tagged `lua`:

```text
[INFO lua] score	1200	nil
```

### `require`

```lua
require(path: string): any
```

Runs the script at `path`, relative to the script calling `require`, and returns what it returns.
Leave out the `.luau`, and start the path with `./` or `../`:

```lua
local player = require("./player") -- player.luau, next to this script
local bat = require("./enemies/bat") -- enemies/bat.luau
local util = require("../util") -- util.luau, in the folder above
```

A script runs once, the first time it's required; requiring it again returns the same value.
`main.luau` can require `conf.luau` to read the game's settings.

## Luau

Scripts are [Luau](https://luau.org), with its sandboxed standard library. A few things that
differ from Lua 5.x come up often in games:

- Compound assignment: `x += speed * dt`.
- `continue` in loops.
- `if` expressions: `local sign = if x < 0 then -1 else 1`.
- Interpolated strings: `` `score: {score}` ``.
- Optional type annotations, which the engine ignores when running: `function update(dt: number)`.
- `math.clamp(x, min, max)`, `math.sign(x)` and `math.round(x)`.
- Integer division with `//`.

Globals a script defines are seen by every script loaded after it, but see
[Globals and the sandbox](../project-structure.md#globals-and-the-sandbox) before keeping the
game's state in a global table.
