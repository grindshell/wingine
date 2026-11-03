# `input`

The keyboard, mouse and gamepads. Everything in `input` works while the game loads and in
`update` and its tasks, but not in `draw`: read input in `update`, and remember what `draw` needs.

A key or button is **pressed** in the update after it goes down, **released** in the update after
it goes up, and **held** while it's down. Each press and release is seen by exactly one update, so
none is lost when frames come faster or slower than updates, and a key tapped between two updates
counts as both pressed and released. Keys held down don't repeat their presses.

## Actions

Rather than ask about keys and buttons, games usually bind named actions, such as `"jump"`, to
them, and ask about the actions. Then the controls live in one place, an action can have several
keys and buttons, and native libraries can read the same actions.

```lua
input.bind("jump", { "Space", "W", "Up", "South" })
input.bind("confirm", { "Enter", "Space", "E", "South", "Start" })

function update(dt: number)
    if input.pressed("jump") then
        player.jump()
    end
end
```

Bound to a stick's directions as well as keys, an action has a **strength**, how far it's pushed,
and two actions make an **axis**, which a stick moves through every value between and keys move
in whole steps:

```lua
input.bind("left", { "A", "Left", "DPadLeft", "LeftStickLeft" })
input.bind("right", { "D", "Right", "DPadRight", "LeftStickRight" })

function update(dt: number)
    -- -1 all the way left, 1 all the way right, and with the stick, anywhere between.
    player.x += input.axis("left", "right") * WALK_SPEED * dt
end
```

### `input.bind`

```lua
input.bind(action: string, controls: { Key | GamepadButton }, gamepad: number?)
```

Binds `action` to `controls`, [keys](#key-names) and [gamepad buttons](#button-names), replacing
what it was bound to before. An action is held, pressed or released when any of its controls is.

Without `gamepad`, the buttons are any gamepad's. With it, only that gamepad's buttons count, as
when each player has a gamepad of their own, while the keys are the same either way:

```lua
input.bind("p1_jump", { "W", "South" }, 1)
input.bind("p2_jump", { "Up", "South" }, 2)
```

### `input.held`

```lua
input.held(action: string): boolean
```

Returns whether anything bound to `action` is down. An action with nothing bound is never held.

### `input.pressed`

```lua
input.pressed(action: string): boolean
```

Returns whether anything bound to `action` was pressed for this update.

### `input.released`

```lua
input.released(action: string): boolean
```

Returns whether anything bound to `action` was released for this update.

### `input.strength`

```lua
input.strength(action: string): number
```

Returns how far `action` is pushed, from 0 to 1: 1 while one of its keys is down, and otherwise as
far as the furthest of its buttons. A trigger or a stick's direction can be anywhere between; other
buttons are 0 or 1. An action with nothing bound is 0.

### `input.axis`

```lua
input.axis(negative: string, positive: string): number
```

Returns how far `positive` is pushed less how far `negative` is, from -1 to 1. Bound to keys, it's
-1, 0 or 1; bound to a stick's two directions along one axis, it's where the stick is along it.

## Keys

### `input.key_held`

```lua
input.key_held(key: Key): boolean
```

Returns whether `key` is down.

### `input.key_pressed`

```lua
input.key_pressed(key: Key): boolean
```

Returns whether `key` was pressed for this update.

### `input.key_released`

```lua
input.key_released(key: Key): boolean
```

Returns whether `key` was released for this update.

### Key names

Keys are physical: each is named for the key in its place on a US QWERTY keyboard, whatever the
player's layout. So `"W"`, `"A"`, `"S"` and `"D"` are always in the same place under the left hand,
and on a French AZERTY keyboard, `"W"` is the key labelled Z. Names can be written in any case.

| Keys | Names |
|---|---|
| Letters | `"A"` to `"Z"` |
| Digits above the letters | `"Digit0"` to `"Digit9"` |
| Arrows | `"Up"`, `"Down"`, `"Left"`, `"Right"` |
| Editing | `"Space"`, `"Enter"`, `"Escape"`, `"Tab"`, `"Backspace"`, `"Insert"`, `"Delete"` |
| Navigation | `"Home"`, `"End"`, `"PageUp"`, `"PageDown"` |
| Modifiers | `"LeftShift"`, `"RightShift"`, `"LeftControl"`, `"RightControl"`, `"LeftAlt"`, `"RightAlt"` |
| Punctuation | `"Minus"`, `"Equal"`, `"Comma"`, `"Period"`, `"Slash"`, `"Semicolon"`, `"Quote"`, `"Backquote"`, `"Backslash"`, `"LeftBracket"`, `"RightBracket"` |
| Function keys | `"F1"` to `"F12"` |
| Number pad | `"Numpad0"` to `"Numpad9"`, `"NumpadEnter"` |

Shift, Control and Alt each have a key on either side, so to accept both, bind an action to both:

```lua
input.bind("focus", { "LeftShift", "RightShift" })
```

An unknown name, such as `"Shift"`, is an error.

## Gamepads

Gamepads are numbered from 1, in the order they're first connected. A gamepad that's disconnected
keeps its number, and gets it back when it's connected again; everything held on it is released
when it goes. Gamepads connected before the game starts are there when `main.luau` runs.

The engine reads gamepads only while its window has focus, as it does the keyboard. Without a
window, as in tests and [scripted runs](../running-games.md#checking-a-game-without-a-window),
there are only simulated gamepads.

### `input.gamepads`

```lua
input.gamepads(): { number }
```

Returns the numbers of the gamepads connected now, in order.

```lua
for _, gamepad in input.gamepads() do
    print(gamepad, input.gamepad_name(gamepad))
end
```

### `input.gamepad_name`

```lua
input.gamepad_name(gamepad: number): string?
```

Returns the name its driver gives gamepad `gamepad`, such as `"Xbox Controller"`, or `nil` if no
gamepad has had that number.

### `input.button_held`

```lua
input.button_held(button: GamepadButton, gamepad: number?): boolean
```

Returns whether `button` is down on `gamepad`, or without `gamepad`, on any gamepad.

### `input.button_pressed`

```lua
input.button_pressed(button: GamepadButton, gamepad: number?): boolean
```

Returns whether `button` was pressed for this update on `gamepad`, or on any gamepad.

### `input.button_released`

```lua
input.button_released(button: GamepadButton, gamepad: number?): boolean
```

Returns whether `button` was released for this update on `gamepad`, or on any gamepad.

### `input.button_strength`

```lua
input.button_strength(button: GamepadButton, gamepad: number?): number
```

Returns how far `button` is pushed on `gamepad`, from 0 to 1, or without `gamepad`, the furthest it
is on any gamepad.

### Button names

Buttons are named for where they are, in the layout of an Xbox controller, whatever the gamepad.
The four face buttons are named for the compass: `"South"` is the bottom one, which is A on an Xbox
controller, Cross on a PlayStation's and B on a Nintendo's. Names can be written in any case, and
none is also a key's name.

| Buttons | Names |
|---|---|
| Face buttons | `"South"`, `"East"`, `"West"`, `"North"` |
| Bumpers (LB and RB, L1 and R1) | `"LeftShoulder"`, `"RightShoulder"` |
| Triggers (LT and RT, L2 and R2) | `"LeftTrigger"`, `"RightTrigger"` |
| The middle (View and Menu, Share and Options) | `"Select"`, `"Start"`, and `"Guide"` for the Xbox or PS button, which the system often takes for itself |
| Pressing a stick in | `"LeftThumb"`, `"RightThumb"` |
| D-pad | `"DPadUp"`, `"DPadDown"`, `"DPadLeft"`, `"DPadRight"` |
| Left stick | `"LeftStickUp"`, `"LeftStickDown"`, `"LeftStickLeft"`, `"LeftStickRight"` |
| Right stick | `"RightStickUp"`, `"RightStickDown"`, `"RightStickLeft"`, `"RightStickRight"` |

### Triggers and sticks

Triggers can be pushed part of the way, and each direction of a stick is pushed as far as the stick
is that way: a stick held up and to the left pushes `"LeftStickUp"` and `"LeftStickLeft"` both, a
little over 0.7 each. Like any button, they're held from halfway, and stay held until they fall
back below 0.4, so one resting near halfway doesn't flicker.

Sticks rarely rest exactly in their middle, nor reach their edge, so a stick counts as at rest
until it's 0.15 of the way from its middle, and as pushed all the way from 0.95. Between the two,
its directions grow evenly from 0 to 1, keeping the angle it's pushed at.

## The mouse

### `input.mouse_position`

```lua
input.mouse_position(): (number?, number?)
```

Returns the cursor's position in canvas pixels, or `nil, nil` when the cursor is outside the
window. With a fixed [`resolution`](../project-structure.md#resolution), the position accounts for
the scaling, and over the black bars it's outside the canvas: below 0, or past the canvas's size.

```lua
local x, y = input.mouse_position()
if x then
    hovered = button_at(x, y)
end
```

The position is in canvas pixels whatever camera `draw` sets. To find the world position under
the cursor for a camera set with `graphics.set_camera(camera_x, camera_y, nil, zoom)`, add the
camera's offset, which the engine rounds to whole pixels, and divide by its zoom (1 if not given):

```lua
local canvas_width, canvas_height = graphics.size()
local world_x = (x + math.round(camera_x * zoom - canvas_width / 2)) / zoom
local world_y = (y + math.round(camera_y * zoom - canvas_height / 2)) / zoom
```

With bounds, use the center the bounds held the camera to.

### `input.mouse_held`

```lua
input.mouse_held(button: MouseButton): boolean
```

Returns whether `button` is down: `"left"`, `"right"`, `"middle"`, `"back"` or `"forward"`.

### `input.mouse_pressed`

```lua
input.mouse_pressed(button: MouseButton): boolean
```

Returns whether `button` was pressed for this update.

### `input.mouse_released`

```lua
input.mouse_released(button: MouseButton): boolean
```

Returns whether `button` was released for this update.

When the window loses focus, every key and button that was held counts as released, since the
window no longer hears about them.
