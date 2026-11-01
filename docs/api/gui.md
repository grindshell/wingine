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
  and using it is an error.
- **A `Ui` works only during the call it's passed to.** One kept for later is an error to use.
- **Write `function gui(ui: Ui)`**, with the type, so [luau-lsp](../editor.md#checking-scripts-with-luau-lsp)
  knows what `ui` is and checks its methods. Inside containers, it knows from the method.

The interface is drawn at the window's own resolution, over the canvas and any bars, so its text
stays sharp however small a fixed resolution is. Positions and sizes are in points, the window's
logical pixels, not canvas pixels. Input the interface wants, such as a click on a window or keys
typed into a text field, doesn't reach [`input`](input.md); the rest does, as usual.

In `gui`, the context works as in `update`: sounds play, scenes load, and so on. Drawing doesn't,
and key presses have already been seen by `update`, which is where to read them. An error in `gui`
stops the game, as one in `update` does. Without the engine's `egui` feature, which is on by
default, `gui` isn't called.

A game that doesn't define `gui` when it loads skips egui altogether, apart from the window of
frame stats on its [`debug_key`](../project-structure.md#debug_key).

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
```

Scrolls what `contents` adds up and down, when there's more than fits.

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
ui:text_input(text: string, hint: string?): (string, boolean)
```

A field of one line of text, showing `hint` greyed out while it's empty. Returns its text after any
typing, and whether Enter was pressed in it, as for a field of commands:

```lua
local command, entered = ui:text_input(command, "Command")
if entered then
    run(command)
    command = ""
end
```

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
