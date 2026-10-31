# `input`

The keyboard and mouse. Everything in `input` works while the game loads and in `update` and its
tasks, but not in `draw`: read input in `update`, and remember what `draw` needs.

A key or button is **pressed** in the update after it goes down, **released** in the update after
it goes up, and **held** while it's down. Each press and release is seen by exactly one update, so
none is lost when frames come faster or slower than updates, and a key tapped between two updates
counts as both pressed and released. Keys held down don't repeat their presses.

## Actions

Rather than ask about keys, games usually bind named actions, such as `"jump"`, to keys, and ask
about the actions. Then the controls live in one place, an action can have several keys, and
native libraries can read the same actions.

```lua
input.bind("jump", { "Space", "W", "Up" })
input.bind("confirm", { "Enter", "Space", "E" })

function update(dt: number)
    if input.pressed("jump") then
        player.jump()
    end
end
```

### `input.bind`

```lua
input.bind(action: string, keys: { Key })
```

Binds `action` to `keys`, replacing any keys it was bound to before. An action is held, pressed or
released when any of its keys is.

### `input.held`

```lua
input.held(action: string): boolean
```

Returns whether any key bound to `action` is down. An action with no keys bound is never held.

### `input.pressed`

```lua
input.pressed(action: string): boolean
```

Returns whether any key bound to `action` was pressed for this update.

### `input.released`

```lua
input.released(action: string): boolean
```

Returns whether any key bound to `action` was released for this update.

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
the cursor for a camera set with `graphics.set_camera(camera_x, camera_y)`, add the camera's
offset, which the engine rounds to whole pixels:

```lua
local canvas_width, canvas_height = graphics.size()
local world_x = x + math.round(camera_x - canvas_width / 2)
local world_y = y + math.round(camera_y - canvas_height / 2)
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
