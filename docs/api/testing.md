# Tests

What tests have that the game doesn't: [`test`](#test), [`sim`](#sim),
[`assert_equal`](#assert_equal) and [`assert_near`](#assert_near). Tests are in files named
`*.test.luau` anywhere in the game's folder, which `wingine test` runs: see
[Testing a game in Luau](../running-games.md#testing-a-game-in-luau) for how.

```lua
test("the ball waits for a serve", function()
    sim.run_for(1)
    assert_equal(pong.phase, "ready")
    sim.tap("Space")
    assert_equal(pong.phase, "rally")
end)
```

A test file runs in the game's VM, after the game's `main.luau`, lent what `update` is lent. So
it reads and changes the game's globals as they are at the time, calls the engine's functions,
and `require`s the game's modules by their paths from the test file, getting the same ones the
game has. These four are a test file's own: the game doesn't see them.

`wait` and `wait_until` work in tests as they do in tasks, running a frame at a time while they
wait.

- [`test`](#test)
- [`sim`](#sim): [time](#time), [keys](#keys), [actions](#actions), [the mouse](#the-mouse),
  [gamepads](#gamepads), [the interface](#the-interface), [the window](#the-window),
  [screenshots](#screenshots)
- [`assert_equal`](#assert_equal), [`assert_near`](#assert_near)

## `test`

```lua
test(name: string, run: () -> (), options: TestOptions?)
```

Declares a test called `name`, which runs `run` against a fresh copy of the game once the file
has loaded. Each test of a file gets its own: the game's `main.luau` has run, but no frame has,
its saves start empty, and nothing one test does reaches the next. Names are unique in a file, and
tests are declared only as their file loads, at its top level.

`options` is a table with any of:

| Option | What it does | Default |
|---|---|---|
| `time_limit` | Seconds of game time the test may run. Past it, the test fails, saying where it was. | 60 |

```lua
test("the computer wins a whole match", function()
    sim.tap("Space")
    wait_until(function() return pong.phase == "over" end)
    assert_equal(pong.winner, "right")
end, { time_limit = 300 })
```

A test fails if it raises an error, as a failed assertion does, saying where; if the game stops
with an error in a frame it runs; or if it runs past its time limit.

## `sim`

Drives the game as a player would, and runs its frames, at a fixed 60 a second. Frames run only
when a test asks: nothing moves while the test's own code runs.

Keys, mouse buttons and gamepad buttons are named as [`input`](input.md) names them: `"Space"`,
`"left"`, `"South"`. A mouse button left out is `"left"`. A function that can't do what it's
asked, such as pressing an action the game hasn't bound, raises an error where the test called it.

### Time

#### `sim.step`

```lua
sim.step(frames: number?)
```

Runs `frames` frames, 1 by default, each updating the game and drawing it.

#### `sim.run_for`

```lua
sim.run_for(seconds: number)
```

Runs `seconds` of game time, rounded to whole frames of a sixtieth of a second.

#### `sim.frame_count`

```lua
sim.frame_count(): number
```

Returns how many frames have run.

### Keys

#### `sim.press`, `sim.release`

```lua
sim.press(key: Key)
sim.release(key: Key)
```

Holds `key` down, until it's released. As a system does, the last key held down repeats, after
half a second, 30 times a second. Keys type what they would on a US keyboard, so the game's
[`input.text`](input.md#inputtext) has them.

#### `sim.tap`

```lua
sim.tap(key: Key)
```

Presses `key`, runs a frame, and lets it go.

#### `sim.hold`

```lua
sim.hold(key: Key, seconds: number)
```

Holds `key` down for `seconds` of game time.

#### `sim.type_text`

```lua
sim.type_text(text: string)
```

Types `text` without pressing keys, as a keyboard of any layout or an input method would, into
the game's `input.text`, or a field of its interface that has focus.

### Actions

```lua
sim.press_action(action: string)
sim.release_action(action: string)
sim.tap_action(action: string)
sim.hold_action(action: string, seconds: number)
```

Press, release, tap and hold what the game bound `action` to with
[`input.bind`](input.md#inputbind): its first key, or if it has none, its first mouse button, or
if it has none of those either, its first gamepad button, on the first connected gamepad (or the
one it's bound to). So a test presses what the game binds, wherever the game binds it. An action
bound to nothing, or only to the buttons of gamepads that aren't connected, is an error.

### The mouse

```lua
sim.move_mouse(x: number, y: number)
sim.move_mouse_by(x: number, y: number)
sim.press_mouse(button: MouseButton?)
sim.release_mouse(button: MouseButton?)
sim.click(x: number, y: number, button: MouseButton?)
sim.scroll(x: number, y: number)
```

Moves the cursor to `x`, `y` in canvas pixels, and holds a button down or lets it go.
`sim.click` moves the cursor, presses the button, runs a frame, and lets it go. `sim.scroll`
turns the mouse wheel where the cursor is, `y` notches away from the player and `x` tilted right,
as [`input.wheel`](input.md#inputwheel) has them, for the next update.

`sim.move_mouse_by` moves the mouse itself rather than the cursor, `x` right and `y` toward the
player, in the units of [`input.mouse_motion`](input.md#inputmouse_motion), which returns it in the
next update. As in a window, the game sees it only while it has
[captured the mouse](input.md#capturing-the-mouse), which a test's game does as it would in a
window, though there's no cursor to hide; otherwise it does nothing. So a test of a game that
looks around with the mouse does what captures it, then moves it:

```lua
test("the mouse turns the view", function()
    sim.click(160, 90)
    sim.move_mouse_by(200, 0)
    sim.step()
    assert(game.yaw < 0, "turned right")
end)
```

While the mouse is captured, `sim.move_mouse` moves a cursor the game doesn't see until it
releases the mouse.

### Gamepads

#### `sim.connect_gamepad`

```lua
sim.connect_gamepad(name: string?): number
```

Connects a gamepad, and returns its number: 1 for the first, and one more for each after. A game
starts with none.

#### `sim.disconnect_gamepad`

```lua
sim.disconnect_gamepad(gamepad: number)
```

Disconnects it, letting go of everything held on it.

#### Buttons

```lua
sim.press_button(gamepad: number, button: GamepadButton, strength: number?)
sim.release_button(gamepad: number, button: GamepadButton)
sim.tap_button(gamepad: number, button: GamepadButton)
sim.hold_button(gamepad: number, button: GamepadButton, seconds: number)
```

Press, release, tap and hold a connected gamepad's button. `strength`, from 0 to 1, is how far
it's pushed, as a trigger can be: all the way by default. A stick's direction, such as
`"LeftStickLeft"`, pushes the stick that way.

#### `sim.move_stick`

```lua
sim.move_stick(gamepad: number, stick: "left" | "right", x: number, y: number)
```

Moves a stick to `x`, `y`, each from -1 to 1, with `y` down: `0, 0` is its middle.

### The interface

With the engine's `egui` feature, which is on by default.

#### `sim.click_gui`

```lua
sim.click_gui(label: string)
```

Clicks the middle of the game's interface's widget labelled `label`: a button's text, or what a
field is labelled by. It must have been laid out in the frame before, and a window shows from the
frame after it opens, so step a frame or two first. It runs the frames that complete the click,
so a button has been pressed when it returns.

#### `sim.gui_rect`

```lua
sim.gui_rect(label: string): Rect?
```

Returns where the widget labelled `label` was in the last frame, in canvas pixels, or `nil`.

### The window

```lua
sim.resize(width: number, height: number)
sim.request_close(): boolean
sim.has_quit(): boolean
```

`sim.resize` resizes the window, in pixels; it starts the size `conf.luau` gives it.
`sim.request_close` presses its close button, calling the game's
[`close_requested`](globals.md#close_requested), and returns whether the window would close.
`sim.has_quit` tells whether the game called [`window.quit`](window.md#windowquit).

### Screenshots

Screenshots are of the latest frame, as the window would show it, at its size. Their paths are
relative to the test file.

#### `sim.screenshot`

```lua
sim.screenshot(path: string)
```

Saves the latest frame as a PNG at `path`.

#### `sim.matches_screenshot`

```lua
sim.matches_screenshot(path: string, options: ScreenshotOptions?): (boolean, string?)
```

Returns whether the latest frame matches the PNG at `path`, and if it doesn't, what differs, so
that `assert` fails with it:

```lua
test("the court before the first serve", function()
    sim.step()
    assert(sim.matches_screenshot("screenshots/ready.png"))
end)
```

`options` is a table with any of:

| Option | What it does | Default |
|---|---|---|
| `tolerance` | How much a channel of a pixel may differ, from 0 to 255. GPUs blend a little differently, so frames rendered on different ones differ by 1 or 2. | 2 |
| `pixels` | How many pixels may differ by more, as where GPUs disagree about textures in perspective. | 0 |

A frame that doesn't match, or has no reference to match, is saved in the game's `.test-output`
folder, with a picture of where it differs in magenta, and the message says where. Each run
clears that folder as it starts. Run with `--update`, a frame that doesn't match is saved at
`path` instead, as the reference, and matches.

## `assert_equal`

```lua
assert_equal(actual: any, expected: any, message: string?)
```

Fails the test unless `actual` equals `expected`: tables by their fields, as deep as they go, and
anything else as `==` compares it. The error says what each was, after `message` if it's given:

```text
pong.test.luau:12: the phase: expected "rally", got "ready"
```

## `assert_near`

```lua
assert_near(actual: number, expected: number, tolerance: number?, message: string?)
```

Fails the test unless `actual` is within `tolerance` of `expected`: 1e-6 by default. Use it for
positions and speeds, which arithmetic rounds.

```lua
-- Pong's paddle moves 160 pixels a second.
local start = pong.left.y
sim.hold("Down", 0.25)
assert_near(pong.left.y, start + 40, 1e-3)
```

Luau's own `assert(value, message)` fails the test unless `value` is true.
