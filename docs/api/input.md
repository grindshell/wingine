# `input`

The keyboard, mouse and gamepads. Everything in `input` works while the game loads and in
`update` and its tasks, but not in `draw`: read input in `update`, and remember what `draw` needs.

A key or button is **pressed** in the update after it goes down, **released** in the update after
it goes up, and **held** while it's down. Each press and release is seen by exactly one update, so
none is lost when frames come faster or slower than updates, and a key tapped between two updates
counts as both pressed and released.

A key held down isn't pressed again, but it **repeats**, as the system repeats keys: after a delay,
then at a steady rate, both as the player's settings say. Only the last key pressed repeats, as in
a text field. A game that takes a step for each press and each repeat walks while a key is held:

```lua
for direction, key in { left = "H", down = "J", up = "K", right = "L" } do
    if input.key_pressed(key) or input.key_repeated(key) then
        player.step(direction)
    end
end
```

Gamepad buttons don't repeat, since the system repeats only keys.

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
input.bind(action: string, controls: { Control }, gamepad: number?)
```

Binds `action` to `controls`, [keys](#key-names), [gamepad buttons](#button-names) and
[mouse buttons](#mouse-buttons-as-controls), replacing what it was bound to before. An action is
held, pressed or released when any of its controls is.

A mouse button is named with `Mouse` before it, since `"Left"` and `"Right"` are the arrow keys:

```lua
input.bind("fire", { "LeftControl", "MouseLeft", "RightTrigger" })
```

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

### `input.repeated`

```lua
input.repeated(action: string): boolean
```

Returns whether a key bound to `action` [repeated](#inputkey_repeated) for this update. Its
gamepad buttons don't repeat.

### `input.strength`

```lua
input.strength(action: string): number
```

Returns how far `action` is pushed, from 0 to 1: 1 while one of its keys or mouse buttons is down,
and otherwise as far as the furthest of its gamepad buttons. A trigger or a stick's direction can
be anywhere between; other buttons are 0 or 1. An action with nothing bound is 0.

### `input.axis`

```lua
input.axis(negative: string, positive: string): number
```

Returns how far `positive` is pushed less how far `negative` is, from -1 to 1. Bound to keys, it's
-1, 0 or 1; bound to a stick's two directions along one axis, it's where the stick is along it.

### `input.bindings`

```lua
input.bindings(action: string): ({ Control }, number?)
```

Returns what `action` is bound to, named as `input.bind` takes them, in the order they were
bound, and the gamepad whose buttons count, if it was bound to one. An action with nothing bound
has an empty list. A game names an action by its first control, so its prompts follow the
player's choice. Like the rest of `input`, it doesn't work in `draw`, so find the name in
`update`:

```lua
local prompt = ""

function update(dt: number)
    prompt = `{input.bindings("interact")[1]}: Talk`
end

function draw()
    graphics.draw_text(font, prompt, 4, 4, "#ffffff")
end
```

## Choosing controls

A screen where players choose their own controls waits for them to press something, then binds
it. [`input.pressed_controls`](#inputpressed_controls) says what they pressed, and
[`input.bindings`](#inputbindings) what an action has, to show it and to bind it again with the
change:

```lua
input.bind("jump", { "Space", "South" })
-- What the player chose before, by action.
local chosen = save.read("controls") or {}
for action, controls in chosen do
    input.bind(action, controls)
end

-- The action waiting for its new key, if one is: the menu picking one sets it.
local waiting: string? = nil

function update(dt: number)
    local pressed = input.pressed_controls()[1]
    if waiting and pressed == "Escape" then
        waiting = nil
    elseif waiting and pressed then
        -- The new key goes first, in place of the one it had first.
        local controls = input.bindings(waiting)
        controls[1] = pressed
        input.bind(waiting, controls)
        chosen[waiting] = controls
        save.write("controls", chosen)
        waiting = nil
    end
end
```

Bindings last only while the game runs, so a game keeps the player's in a [save](save.md) and
binds them again as it starts. The press that picked the action from a menu is seen by the update
that picked it, so a screen starts waiting from the next. The RPG's `controls.luau` is such a
screen, which also hands a key another action has over to it in exchange for the one it replaces,
so no two actions share one.

### `input.pressed_controls`

```lua
input.pressed_controls(): { Control }
```

Returns the keys, mouse buttons and gamepad buttons pressed for this update, in the order they
were pressed, named as [`input.bind`](#inputbind) takes them, so what it returns binds as it is:
a click of the left button is `"MouseLeft"`. Gamepad buttons are any gamepad's, and include
sticks' directions, such as `"LeftStickUp"`, pushed past halfway. A key the system
[repeats](#inputkey_repeated) isn't pressed again. What the interface takes isn't among them: the
keys a focused text field does, or a click on one of its windows.

### Mouse buttons as controls

| Button | As [`input.mouse_held`](#inputmouse_held) names it | As a control |
|---|---|---|
| Left | `"left"` | `"MouseLeft"` |
| Right | `"right"` | `"MouseRight"` |
| Middle, the wheel pressed in | `"middle"` | `"MouseMiddle"` |
| Back, on the side | `"back"` | `"MouseBack"` |
| Forward, on the side | `"forward"` | `"MouseForward"` |

Names can be written in any case, and none is also a key's name or a gamepad button's. An action
bound to a mouse button is pressed by a click anywhere on the canvas, wherever the cursor is, but
not by one the [interface](gui.md) takes.

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

### `input.key_repeated`

```lua
input.key_repeated(key: Key): boolean
```

Returns whether the system repeated `key` for this update, as it does while the key is held. The
press that started it isn't a repeat: `key_pressed` tells of that.

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

## Text

Keys are physical, but what they type is the player's layout's: Shift and the key a US keyboard
has `.` on types `>` there, but `:` on a German one, where `>` is Shift and the key beside the left
Shift. So a game whose commands are characters reads what's typed rather than which keys are
pressed:

```lua
function update(dt: number)
    for _, character in utf8.codes(input.text()) do
        local command = COMMANDS[utf8.char(character)]
        if command then
            command()
        end
    end
end
```

A key held down types again each time it [repeats](#inputkey_repeated).

### `input.text`

```lua
input.text(): string
```

Returns what was typed for this update, in order, as the player's layout made it, with what dead
keys compose (`´` then `e` types `é`) and an [input method](#inputset_input_method) chooses. It
isn't the keys: Enter, Tab, Backspace and the like type no text, so read them as keys, and neither
does a key pressed with Control (but not Alt, since Control and Alt together are AltGr on Windows)
or with Command or the Windows key, which make shortcuts. Text that a focused field of the game's
[interface](gui.md) takes isn't the game's.

On macOS, dead keys compose only while the game [takes an input method](#inputset_input_method);
otherwise each types on its own.

### `input.set_input_method`

```lua
input.set_input_method(area: Rect?)
```

Takes the system's input method while `area` is given, and with `nil`, lets it go. Players of
Chinese, Japanese and Korean, among others, type with one, composing each character from several
keys and choosing it from what the method offers, which it shows beside `area`: a rect in canvas
pixels, `{ x = ..., y = ..., width = ..., height = ... }`, around where the text goes. Take one
only while the game asks for text, such as a name:

```lua
input.set_input_method({ x = 40, y = 80, width = 160, height = 12 })
```

While the method composes, the keys the player types reach it rather than the game, so they're not
pressed, and what it composes is [`input.composition`](#inputcomposition) until the player chooses
what it becomes, which `input.text` then returns. Games take none by default, so every key reaches
them.

### `input.composition`

```lua
input.composition(): string
```

Returns what the input method is composing, for the game to show where the text goes, or `""`
while it isn't composing or the game takes none.

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
window, or the mouse is [captured](#capturing-the-mouse). With a fixed [`resolution`](../project-structure.md#resolution), the position accounts for
the scaling, and over the black bars it's outside the canvas: below 0, or past the canvas's size.

```lua
local x, y = input.mouse_position()
if x then
    hovered = button_at(x, y)
end
```

The position is in canvas pixels whatever camera `draw` sets. To find the world position under
the cursor for a camera set with `graphics.set_camera(camera_x, camera_y, nil, zoom)`, add the
camera's offset and divide by its zoom (1 if not given). The engine rounds the offset to whole
pixels at whole zooms and their reciprocals, such as 0.5, and leaves it as it is at others:

```lua
local canvas_width, canvas_height = graphics.size()
local on_grid = zoom % 1 == 0 or (1 / zoom) % 1 == 0
local offset_x = camera_x * zoom - canvas_width / 2
local offset_y = camera_y * zoom - canvas_height / 2
if on_grid then
    offset_x, offset_y = math.round(offset_x), math.round(offset_y)
end
local world_x = (x + offset_x) / zoom
local world_y = (y + offset_y) / zoom
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

### `input.wheel`

```lua
input.wheel(): (number, number)
```

Returns how far the mouse wheel turned for this update, in notches, as `x, y`: `y` is positive
turned away from the player, as to scroll up, and negative turned toward them, and `x`, for a wheel
that tilts, is positive tilted right. Turns between two updates add up, and each is seen by one
update, as presses are. A touchpad scrolls by fractions of a notch, one for every 40 points it
scrolls. Turns over the [interface](gui.md) are its own, as its clicks are.

```lua
local _, turned = input.wheel()
-- Nearer for each notch turned away, and farther for each turned toward.
zoom = math.clamp(zoom * 0.85 ^ turned, 0.4, 2)
```

## Capturing the mouse

A game that looks around with the mouse, as a first-person one does, has no use for a cursor,
which stops at the screen's edge. It **captures** the mouse instead: the cursor is hidden and
held in the window, and the game reads how far the mouse moved.

```lua
-- Radians the view turns for each unit the mouse moves.
local SENSITIVITY = 0.0025

input.bind("release", { "Escape" })

function update(dt: number)
    if input.pressed("release") then
        input.set_mouse_captured(false)
    elseif input.mouse_pressed("left") and not input.mouse_captured() then
        input.set_mouse_captured(true)
    end
    local x, y = input.mouse_motion()
    yaw -= x * SENSITIVITY
    pitch = math.clamp(pitch - y * SENSITIVITY, -1.5, 1.5)
end
```

### `input.set_mouse_captured`

```lua
input.set_mouse_captured(captured: boolean)
```

Captures the mouse, or with `false`, releases it. While it's captured:

- The cursor is hidden and can't leave the window: locked in place where the system can, and
  otherwise confined to the window.
- [`input.mouse_motion`](#inputmouse_motion) returns how far the mouse moved.
- There's no cursor, so [`input.mouse_position`](#inputmouse_position) returns `nil, nil`.
- The mouse's buttons and wheel are the game's wherever they're pressed, and none reach the
  [interface](gui.md).

The window captures the mouse once the frame has drawn. A window that can't, as one without focus
can't, logs a warning, and the mouse is released again.

When the window loses focus, as when the player switches to another, the mouse is released, and
stays released when they come back: the game captures it again when it wants, as on a click.
Wingine binds no key to release it, so a game binds its own, as Escape above, or the player has
only the system's ways out, such as Alt+Tab.

A game [reloaded](../running-games.md#reloading-as-you-work) in its window starts with the mouse
released, as it started the first time.

On the web, a page may capture the mouse only just after the player clicks or presses a key, so
call it in the `update` that sees the press, and the browser's own Escape releases the mouse
without telling the game.

In [tests](testing.md) and scripted runs, capturing the mouse is only noted.

### `input.mouse_captured`

```lua
input.mouse_captured(): boolean
```

Returns whether the mouse is captured: as the game last set it, unless the window couldn't capture
it or has lost focus since. A game shows "Click to play" while it isn't.

### `input.mouse_motion`

```lua
input.mouse_motion(): (number, number)
```

Returns how far the mouse moved for this update while it's captured, as `x, y`: `x` is positive
moved right, and `y` positive moved toward the player, as canvas pixels grow. Moves between two
updates add up, and each is seen by one update, as presses are. It's `0, 0` while the mouse isn't
captured.

The numbers are in the mouse's own units, not canvas pixels, and aren't scaled with the canvas:
what the mouse reports, before the system speeds up the cursor, on Windows and Linux, and how far
the cursor would have moved on macOS and the web. Mice differ in how much they report for the
same move, so multiply by a sensitivity, and let the player set it.
