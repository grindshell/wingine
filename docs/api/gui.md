# `gui`

Interfaces drawn over the game with [egui](https://www.egui.rs/): windows and panels of buttons,
sliders, text fields and the like, for debug tools, cheats and editors of a game's own. The RPG's
F1 window, in `examples/rpg/debug_panel.luau`, is one.

A game lays its interface out in a function of its own, `gui(ui)`, which the engine calls once a
frame, after the frame's updates and before `draw`, with a `Ui` covering the window. egui is
immediate mode: there's nothing to create or keep. Each frame, the game says what the interface
holds, and the widgets say what happened to them since:

```lua
local speed = 100
local god_mode = false

function gui(ui: Ui)
    ui:window("Tuning", function(ui)
        speed = ui:slider("Speed", speed, 0, 400)
        god_mode = ui:checkbox("God mode", god_mode)
        if ui:button("Restart") then
            restart()
        end
    end)
end
```

- **Widgets that change a value take it and return it**, changed or not, since Luau can't change
  a variable it's passed: `speed = ui:slider("Speed", speed, 0, 400)`. Buttons return whether
  they were clicked.
- **Containers take a function**, which they call with the `Ui` inside them, such as a window's.
  Inside it, use that `Ui`, not the one outside: the outer one is busy until the function returns,
  and using it is an error. Containers hold any others, their own kind too, such as a group
  greyed out with `ui:enabled` inside a panel greyed out with another.
- **A `Ui` works only during the call it's passed to.** One kept for later is an error to use.
- **Write `function gui(ui: Ui)`**, with the type, so [luau-lsp](../editor.md#checking-scripts-with-luau-lsp)
  knows what `ui` is and checks its methods. Inside containers, it knows from the method.

The interface is drawn at the window's own resolution, over the canvas and any bars, so its text
stays sharp however small a fixed resolution is. Positions and sizes are in points, the window's
logical pixels, not canvas pixels. Input the interface wants, such as a click on a window or keys
typed into a text field, doesn't reach [`input`](input.md); the rest does, as usual. Tab reaches
the interface only while one of its widgets has keyboard focus, moving it to the next, so a game
can use Tab for itself while its interface shows.

In `gui`, the context works as in `update`: sounds play, scenes load, and so on. Drawing doesn't,
and key presses have already been seen by `update`, which is where to read them. An error in `gui`
stops the game, as one in `update` does. Without the engine's `egui` feature, which is on by
default, `gui` isn't called.

While a game has no `gui`, its frames skip egui altogether, apart from the window of frame stats
on its [`debug_key`](../project-structure.md#debug_key); one it defines as it plays is called from
the next frame.

## Scale and appearance

`ui:scale()` returns the current game-wide zoom relative to the display's own scale.
`ui:scale(1.5)` requests 150% starting on the next pass and returns the factor for the current
pass. The default is 1 and supported factors range from 0.25 to 4. Canvas drawing and pointer
coordinates remain in points. Use `paint:bounds()` when placing game graphics under a canvas.

Players change it too, with egui's own shortcuts: Ctrl and plus (or equals) zooms in by a tenth,
Ctrl and minus zooms out, and Ctrl and 0 goes back to 1 (Command for Ctrl on macOS).
`ui:scale()` returns what they chose. Nothing keeps it between runs, so a game that should
remember it saves `ui:scale()` and sets it again as it starts.

Call `ui:set_theme(palette)` once when choosing a palette. Panels, text fields, buttons,
selection highlights and popup windows share it. Colors take the usual engine color values.
`dark` defaults to true and `radius` defaults to three points.

```lua
ui:set_theme({
    panel = "#172526", window = "#213031", field = "#101919",
    control = "#30423f", hover = "#405751", active = "#51694f",
    text = "#f2e9d4", muted = "#b8c5b9", border = "#546865", accent = "#e6b55e",
    radius = 2,
})
```

## Containers

### `ui:window`

```lua
ui:window(window: string | WindowOptions, contents: (Ui) -> ())
```

A window over the game, which `contents` fills, with a title bar to drag it by and fold it away
with. Its title tells it apart from other windows. `window` is its title, or a table of options:

| Option | What it does | Default |
|---|---|---|
| `title` | The text in its title bar. Required. | |
| `x`, `y` | Where its top-left corner starts, in points. | Where egui puts it |
| `width`, `height` | The size it starts at, in points. | Its contents' size |
| `resizable` | Whether its edges can be dragged to resize it. | `true` |
| `collapsible` | Whether its title bar folds it away. | `true` |

```lua
ui:window({ title = "Debug", x = 12, y = 12, resizable = false }, function(ui)
    ui:label("Hello")
end)
```

Only where it starts is set: once dragged or resized, a window stays where it's put. While it's
folded away, `contents` isn't called.

### `ui:panel`

```lua
ui:panel(panel: PanelSide | PanelOptions, contents: (Ui) -> ())
```

A panel along an edge of the window, `"left"`, `"right"`, `"top"` or `"bottom"`, which `contents`
fills. `panel` is its side, or a table with its `side` and either of `size`, its width (or height,
along the top or bottom) in points, and `resizable`, whether its inner edge can be dragged. Panels
cover the game rather than shrinking the canvas. There can be one on each side.

### `ui:horizontal`, `ui:vertical`

```lua
ui:horizontal(contents: (Ui) -> ())
ui:vertical(contents: (Ui) -> ())
```

Lay out what `contents` adds from left to right, or from top to bottom, as is usual outside
`horizontal`.

### `ui:group`

```lua
ui:group(contents: (Ui) -> ())
```

Frames what `contents` adds.

### `ui:collapsing`

```lua
ui:collapsing(title: string, contents: (Ui) -> ())
```

A header that opens and closes what `contents` adds, which starts closed. `contents` runs only
while it's open.

### `ui:scroll`

```lua
ui:scroll(contents: (Ui) -> ())
ui:scroll(options: ScrollOptions, contents: (Ui) -> ())
```

Scrolls what `contents` adds up and down, when there's more than fits. `options` is a table with:

| Option | What it does | Default |
|---|---|---|
| `height` | Maximum height in points, leaving room below the scroll area for other controls. | Available height |
| `stick_to_bottom` | Whether it keeps to its end as more is added, as a log or a chat does. It does while it's scrolled to its end: scrolled back to read, it stays where it's put. | `false` |

```lua
ui:scroll({ stick_to_bottom = true }, function(ui)
    for _, line in log do
        ui:label(line)
    end
end)
```

### `ui:modal`

```lua
ui:modal(name: string, contents: (Ui) -> ()): boolean
```

A box in the middle of the window, over everything else, which `contents` fills, and which must be
answered: while it's laid out, the rest of the interface is dimmed and can't be clicked or typed
into. `name` tells it apart from other modals. A [window](#uiwindow) only floats, and what's
behind it still works.

Returns whether the player asked to be rid of it, by pressing Escape or clicking outside it. As
with everything in an interface, it shows only in the frames the game lays it out, so the game
keeps whether it's open:

```lua
if removing then
    local dismissed = ui:modal("remove", function(ui)
        ui:heading("Remove this file?")
        if ui:button("Remove it") then
            remove(removing)
            removing = nil
        end
        if ui:button("Keep it") then
            removing = nil
        end
    end)
    if dismissed then
        removing = nil
    end
end
```

### `ui:grid`

```lua
ui:grid(name: string, contents: (Ui) -> ())
ui:end_row()
```

Lines what `contents` adds up in columns, a row at a time, with `ui:end_row()` after each row.
`name` tells it apart from other grids.

```lua
ui:grid("stats", function(ui)
    for _, stat in { "attack", "defense" } do
        ui:label(stat)
        ui:monospace(tostring(stats[stat]))
        ui:end_row()
    end
end)
```

### `ui:enabled`

```lua
ui:enabled(enabled: boolean, contents: (Ui) -> ())
```

Unless `enabled`, greys out what `contents` adds, which can't then be used: buttons can't be
clicked, and return false.

## Text

### `ui:label`

```lua
ui:label(text: string, color: Color?)
```

Text, in the interface's color, or `color` if given.

### `ui:heading`, `ui:monospace`, `ui:small`

```lua
ui:heading(text: string)
ui:monospace(text: string)
ui:small(text: string)
```

Large text, text in a monospace font, as for numbers in columns, and small text.

## Widgets

### `ui:button`

```lua
ui:button(text: string): boolean
```

Returns whether the button was clicked.

### `ui:checkbox`

```lua
ui:checkbox(text: string, checked: boolean): boolean
```

A box that's checked or not: returns `checked`, or the opposite if it was clicked.

### `ui:radio`

```lua
ui:radio(current: T, value: T, text: string): T
```

One of a set of buttons that choose between values, each a button for one value. It shows as
chosen when `current` is `value`, and returns `value` if it was clicked, or `current`:

```lua
difficulty = ui:radio(difficulty, "easy", "Easy")
difficulty = ui:radio(difficulty, "hard", "Hard")
```

### `ui:combo`

```lua
ui:combo(text: string, current: T, choices: { T }): T
```

A drop-down list of `choices`, each shown as `tostring` shows it, labelled `text`: returns the
choice picked, or `current`.

```lua
rpg.quest = ui:combo("Quest", rpg.quest, story.STAGES)
```

### `ui:slider`

```lua
ui:slider(text: string, value: number, min: number, max: number, step: number?): number
```

A slider from `min` to `max`, with the number beside it, which can be typed into: returns `value`
after any change. With `step`, it moves by that much, as `1` keeps it to whole numbers.

### `ui:drag`

```lua
ui:drag(text: string, value: number, speed: number?, min: number?, max: number?): number
```

A number field, which changes as it's dragged sideways, by `speed` a point (by default, 1), or as
it's typed into: returns `value` after any change, kept between `min` and `max` if they're given.

### `ui:text_input`

```lua
ui:text_input(text: string, field: (string | TextInputOptions)?): (string, boolean)
```

A field of one line of text. Returns its text after any typing, and whether Enter was pressed in
it, as for a field of commands. `field` is its hint, shown greyed out while it's empty:

```lua
local command, entered = ui:text_input(command, "Command")
if entered then
    run(command)
    command = ""
end
```

Or `field` is a table with any of:

| Option | What it does | Default |
|---|---|---|
| `id` | Which text it holds, as a [text edit](#uitext_edit)'s says. | Where the field is |
| `label` | What [tests](testing.md#the-interface) and screen readers find it by, as well as by its hint. Nothing shows it. | None |
| `hint` | Text shown greyed out while it's empty. | None |

```lua
file.title = ui:text_input(file.title, { id = file.name, hint = "Title" })
```

### `ui:text_edit`

```lua
ui:text_edit(text: string, options: TextEditOptions?): (string, boolean, TextCursor?)
```

A field of many lines of text, in which Enter starts a new line, as for notes or a document.
Returns its text after any typing, whether that changed it, as for saving it only then, and
[where its cursor is](#the-cursor):

```lua
local changed
notes, changed = ui:text_edit(notes, { label = "Notes", hint = "Write here", rows = 12 })
if changed then
    wg.save.write("notes", { text = notes })
end
```

It's as wide as there's room for, and grows taller with its text, so one that may outgrow the
window is given `fill`, or goes in a [`ui:scroll`](#uiscroll) with what else scrolls. The field
has what egui's fields have: a cursor moved by the arrow keys and the mouse, selections, the
clipboard, and undo. `options` is a table with any of:

| Option | What it does | Default |
|---|---|---|
| `id` | Which text it holds: see below. | Where the field is |
| `label` | What [tests](testing.md#the-interface) and screen readers find it by. Nothing shows it. | None |
| `hint` | Text shown greyed out while it's empty. | None |
| `rows` | How many lines tall it is at least. | 4 |
| `monospace` | Whether its text is in the monospace font, as for code. | `false` |
| `fill` | Whether it takes all the room its container has left: see below. | `false` |
| `tab` | What Tab does in it: see [Tab](#tab). | Moves on |

With `fill`, the field is as tall as what its container has left below what was added before it,
however tall the window is, and text taller than that scrolls within it, following the cursor. So
a document that is all of a panel, or all the panels leave, needs no size worked out for it:

```lua
function gui(ui: Ui)
    ui:panel("top", toolbar)
    file.text = ui:text_edit(file.text, { id = file.name, fill = true })
end
```

Whatever is added after it in the same container has no room, so add a field that fills last.

A field keeps its cursor, its selection and its undo history from frame to frame by where it is in
the interface. So a field that holds one text and then another, as an editor's does when another
file is opened, would undo the second text back into the first. Give such a field an `id` that
changes with the text it holds, and each text keeps its own:

```lua
file.text = ui:text_edit(file.text, { id = file.name })
```

The whole text goes to the field and comes back every frame, so a long one costs more than a
short one whether or not it changes.

#### Tab

In a field, as in any widget, Tab moves the keyboard's focus on to the next. A field of code or
of indented text wants Tab for itself, and says so with `tab`:

- **A number**, from 1 to 16, indents by that many spaces: to the next stop that many apart, in
  place of anything selected within a line, or with lines selected, each a level more. Shift+Tab
  takes a level off each line selected, or the cursor's: that many spaces, as many as it has, or
  a tab. The text holds no tabs that Tab typed.
- **`true`** types a tab character, and Shift+Tab takes a tab, or four spaces, off the start of
  the cursor's line.

```lua
-- Luau, indented as Luau usually is.
script.text = ui:text_edit(script.text, { id = script.name, monospace = true, tab = 4 })
```

Tab stays in the field from the frame after it's focused, so one pressed in the very frame the
field is clicked still moves on.

#### The cursor

The third result is where the field's cursor is in its text, or `nil` until it has had one, which
it has from the first time it's clicked or typed in:

| Field | What it is |
|---|---|
| `line` | The line the cursor is on, from 1. Lines are what `"\n"` ends, not the rows a long line wraps into. |
| `column` | How far along the line it is, from 1, counting characters. |
| `offset` | How many bytes of the text are before it, so `string.sub(text, 1, offset)` is the text before the cursor. |
| `anchor` | The same of the other end of what's selected, before or after the cursor. It's `offset` when nothing is selected. |

```lua
local text, _, cursor = ui:text_edit(file.text, { id = file.name, fill = true })
if cursor then
    status = `Line {cursor.line}, column {cursor.column}`
    selected = string.sub(text, math.min(cursor.offset, cursor.anchor) + 1, math.max(cursor.offset, cursor.anchor))
end
```

A script reads the cursor and can't move it.

### `ui:progress`

```lua
ui:progress(fraction: number, text: string?)
```

A bar filled `fraction` of the way, from 0 to 1, with `text` on it if given.

### `ui:sprite`

```lua
ui:sprite(sprite: Sprite, tint: Color?, width: number?, height: number?)
```

Shows `sprite`, its colors multiplied by `tint` (by default, white: unchanged), at `width` by
`height` points, or its own size in texels. It's sampled as the game draws it, so pixel art stays
crisp at any size.

### `ui:sprite_button`

```lua
ui:sprite_button(
    sprite: Sprite,
    text: string,
    selected: boolean?,
    tint: Color?,
    width: number?,
    height: number?
): boolean
```

A button showing `sprite` as [`ui:sprite`](#uisprite) does, in a frame: returns whether it was
clicked. `text` names it, and shows beside the pointer while it's hovered. It's framed as chosen
when `selected`, as for a palette of tiles or an inventory, where one is picked:

```lua
for _, tile in tiles do
    if ui:sprite_button(tile.sprite, tile.name, tile == brush, nil, 32, 32) then
        brush = tile
    end
end
```

Tests find it by its `text`, as [`sim.click_gui`](testing.md#simclick_gui) finds other widgets by
their labels.

## Canvases

### `ui:canvas`

```lua
ui:canvas(canvas: string | CanvasOptions, paint: (Paint, CanvasInput) -> ())
```

A space in the interface to paint lines, shapes and text on, as an application paints a graph, a
chart or a timeline, which widgets can't show. `paint` is called with a [`Paint`](#paint) to
paint it with and what the pointer did to it this frame, and paints it afresh each frame, as
everything in an interface is laid out afresh:

```lua
local points = {}

function gui(ui: Ui)
    ui:panel("top", function(ui)
        ui:label("Click to add a point")
    end)
    ui:canvas("Sketch", function(paint, input)
        if input.clicked then
            table.insert(points, input.x)
            table.insert(points, input.y)
        end
        if #points >= 4 then
            paint:path(points, "#8fb8de", 2)
        end
        for i = 1, #points, 2 do
            paint:fill_circle(points[i], points[i + 1], 4, "#f2c14e")
        end
    end)
end
```

`canvas` is its label, or a table of options:

| Option | What it does | Default |
|---|---|---|
| `label` | What [tests](testing.md#the-interface) and screen readers find it by. Required. Nothing shows it. | |
| `width`, `height` | Its size, in points. | All the room its container has left that way |

So a canvas with no size takes all the room left, as a [text edit](#uitext_edit) that fills does:
add it last. A `Paint` works only until `paint` returns.

The input is a table, in points from the canvas's top-left corner:

| Field | What it is |
|---|---|
| `hovered` | Whether the pointer is over the canvas, with nothing else in the way. |
| `x`, `y` | Where the pointer is, while it's over the canvas or dragging it; nil otherwise. |
| `clicked` | Whether the canvas was clicked: the primary button pressed on it and let go without dragging. |
| `dragged_x`, `dragged_y` | How far it was dragged this frame with the primary button held, as to pan, or 0. A press becomes a drag once the pointer has moved a few points, which isn't a click. |
| `scrolled_x`, `scrolled_y` | How far the wheel scrolled this frame while the pointer was over it, in points, smoothed over a few frames: positive `y` as the wheel turns away, to scroll up. |
| `zoom` | How much a scroll with Ctrl held, or a pinch, zoomed this frame while the pointer was over it: above 1 in, below 1 out, 1 for neither. |

Panning and zooming are the script's: it keeps where its view is and how large, and moves them
by what the input says.

### `Paint`

What a canvas is painted with. Positions are in points from the canvas's top-left corner, and
nothing painted shows outside it. What's painted later is over what was painted before. Colors are
[`Color`](README.md#color)s, as everywhere.

| Method | What it paints |
|---|---|
| `paint:size(): (number, number)` | Nothing: returns the canvas's width and height. |
| `paint:bounds(): Rect` | Nothing: returns where the canvas is on the game's canvas, in canvas pixels, as below. |
| `paint:scene(width, height, draw)` | Fills the paint area with a separate game canvas of `width` by `height` pixels. Use `wingine.graphics` in `draw()` to draw into it. The image scales with nearest-neighbor sampling, and later paint calls overlay it in UI points. |
| `paint:line(x1, y1, x2, y2, color, thickness?)` | A line from one point to the other, `thickness` wide, by default 1. |
| `paint:path(points, color, thickness?)` | Lines through `points`, `{ x1, y1, x2, y2, ... }`, two or more. |
| `paint:fill_polygon(points, color)` | The convex polygon whose corners are `points`, three or more, such as an arrow's head. |
| `paint:fill_rect(x, y, width, height, color, rounding?)` | A rectangle with its top-left corner at `x`, `y`, its corners rounded `rounding` points. |
| `paint:outline_rect(x, y, width, height, color, thickness?, rounding?)` | The outline of that rectangle, `thickness` wide inside its edges. |
| `paint:fill_circle(x, y, radius, color)` | A circle of `radius` about `x`, `y`. |
| `paint:outline_circle(x, y, radius, color, thickness?)` | The outline of that circle, `thickness` wide inside its edge. |
| `paint:text(text, x, y, color, options?): (number, number)` | Text on one line, its top at `y`; returns its width and height. |
| `paint:measure(text, options?): (number, number)` | Nothing: returns the width and height `text` would take. |

Text is in the interface's font, and `options` is a table with any of:

| Option | What it does | Default |
|---|---|---|
| `size` | Its size, in points. | The interface's text's |
| `monospace` | Whether it's in the monospace font. | `false` |
| `align` | Which of its edges is at `x`: `"left"`, `"right"`, or its middle, `"center"`. | `"left"` |

```lua
-- A box with a name in its middle, as wide as the name.
local width, height = paint:measure(name)
paint:fill_rect(x - width / 2 - 6, y - 4, width + 12, height + 8, "#2a2e36", 4)
paint:text(name, x, y, "#e8e8e8", { align = "center" })
```

#### Drawing behind a canvas

What a canvas leaves unpainted shows what's under the interface: what the game drew in `draw`.
`paint:bounds()` says where the canvas is there, in [canvas pixels](graphics.md#graphicssize), as
the mouse's position is, so `draw` can fill it, such as with a [3D view](graphics.md#3d-views)
whose `rect` it is, wherever panels round it have left it, while the canvas takes the pointer:

```lua
local wg = require("@wingine")
local cube = wg.graphics.new_cube(1)
local bounds = nil
local turn = 0

function gui(ui: Ui)
    ui:panel("left", function(ui)
        ui:label("Drag the room to turn it")
    end)
    ui:canvas("Room", function(paint, input)
        bounds = paint:bounds()
        turn += input.dragged_x / 100
    end)
end

function draw()
    wg.graphics.clear("#1b1d22")
    if bounds then
        local camera = { position = vector.create(math.sin(turn) * 6, 3, math.cos(turn) * 6), target = vector.zero }
        wg.graphics.begin_3d({ camera = camera, rect = bounds })
        wg.graphics.draw_mesh(cube, vector.zero)
    end
end
```

`gui` runs before `draw` each frame, so `draw` has the canvas's bounds of the same frame.

## Spacing

### `ui:separator`

```lua
ui:separator()
```

A line between what comes before and after.

### `ui:space`

```lua
ui:space(amount: number?)
```

Empty space, `amount` points of it, or the usual space between widgets.
