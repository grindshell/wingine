# The editor

The editor edits a game's [scenes](scenes.md): it shows them as the game draws them, and changes
their nodes with the mouse and the Inspector instead of by hand. It writes scene files back in
the canonical layout, so what it saves stays easy to read and diffs stay small. It's laid out like
[Godot](https://godotengine.org)'s editor, and is itself a game made with Wingine, drawing its
interface with [egui](https://github.com/emilk/egui).

![The editor with the garden open, its pond selected](screenshots/editor.png)

Run it from the repository with the game to edit:

```bash
cargo run -p wingine-editor -- examples/garden
```

Without a folder, it starts with a choice of the repository's examples, and **Project > Open
Folder…** opens any game.

The editor is new, and grows with each part of the current milestone. It edits scenes' nodes and
the scene's sheets and props now. Painting tiles, anchor presets, editing clips and editing scripts
come next. Until then, edit those in the scene file, and the editor shows the change once it's
saved.

## The layout

- **The top bar**: the menus on the left; the main screens in the middle, the **2D** view of the
  open scene, and later **Script**; and **Play** and **Stop** on the right.
- **The Scene dock**, at the top left: the open scene's tree of nodes.
- **The FileSystem dock**, below it: the files in the game's folder. Double-click a scene to open
  it.
- **The Inspector**, on the right: every field of the selected node.
- **The bottom panel**: **Output**, with what the editor says and what a game played from it
  prints. Click its tab to hide it, and again to show it.
- **The middle**: a tab for each open scene, the viewport's toolbar, and the viewport.

Drag the edges between them to resize them.

## The viewport

The viewport shows the open scene as the game draws it, laid out on the game's canvas: the size of
the [`resolution`](project-structure.md#resolution) in `conf.luau`, or of its window. A purple
rectangle outlines the canvas, and red and green lines are the scene's x and y axes through its
origin. A screen anchored to the canvas's edges is laid out as it is in the game.

- **Pan** by dragging with the middle button, or with the left while holding Space.
- **Zoom** with the wheel, about the pointer. **Frame** shows the whole canvas again, and **1:1**
  shows a pixel of the scene for each of the screen's.
- **Select** a node by clicking it. What can be seen comes first, then areas, then plain nodes near
  their origins, then tilemaps. Click again in the same place for the node under that one, and so
  on, round to the first.

The selected node is outlined in blue, with a cross at its origin. Text and tilemaps move with
their node but don't turn or stretch, and their outline shows that.

## Changing nodes

The toolbar's tools, or their keys, decide what dragging in the viewport does:

| Tool | Key | Dragging |
|---|---|---|
| Select | Q | Selects the node under the pointer, and moves it |
| Move | W | Moves the selected node, wherever the drag starts |
| Rotate | E | Turns the selected node about its origin |
| Scale | S | Stretches the selected node along its own axes; with Shift, evenly |

A node moves within its parent, so moving a node inside a turned parent changes its `x` and `y`
along the parent's axes. With **Snap** on, moving snaps `x` and `y` to the grid's spacing, turning
snaps to 15°, and stretching to tenths. Holding Shift snaps turning without it.

The **Inspector** shows the selected node's fields, as scene files name them:

- **Node**: whether it's `visible`, and its `tags`.
- **Transform**: `x` and `y`, `rotation` in degrees (scene files hold radians), `scale_x` and
  `scale_y`, and its `anchor`, if it has one. Drag a number to change it, or click it to type one.
- **Its kind's fields**: a sprite's sheet and cell (**Pick…** shows the sheet to click a cell
  in), its size, stretch, pivot and tint; a rect's size, stretch, pivot and color; text's text,
  scale, wrap, alignment, pivot and color; an area's size, stretch and pivot; a tilemap's tileset
  and size, which resizes its layers; an instance's scene, and the overrides it has.
- **Props**: the node's data for the game. Add one by typing its name and choosing its kind of
  value; lists and tables hold more values.

Select the scene itself, at the top of the Scene dock, to change its sheets and props.

## The Scene dock

- **➕** adds a node of a kind inside the selected one, and **⛓** places another scene file in
  it, an instance. Right-click a node for the same, and more.
- **Rename** a node by double-clicking it or pressing F2, then typing a name and pressing Enter.
  Escape keeps the old name. Clips' tracks follow a node that's renamed or moved.
- **Duplicate** a node with Ctrl+D, and **delete** it with Delete.
- **Drag** a node onto another to move it inside it, onto the top edge of one to move it before it,
  or onto the bottom edge to move it after it. **Move Up** and **Move Down**, in its menu, change
  its place among its siblings: later nodes draw over earlier ones.

## Saving, undo and playing

- **Ctrl+Z** undoes a change and **Ctrl+Y** (or Ctrl+Shift+Z) redoes it. A drag, or typing in a
  field, is one change.
- **Ctrl+S** saves the open scene. A tab shows `*` while its scene has unsaved changes, and closing
  it or the editor asks first.
- **F5**, or **Play**, saves every scene and plays the game in a window of its own, as
  `cargo run -- path/to/game` would. What it prints shows in Output. **F8**, or **Stop**, ends it.

The editor writes scene files in the canonical layout, as
[`SceneFile::to_luau`](scenes.md) does. A file already in it changes only where the scene did, but
the layout has no comments: saving a scene written by hand drops its comments.

The editor reads the game's folder again every few seconds, so files changed elsewhere, such as
images, show up without opening them again.

## Shortcuts

| Keys | |
|---|---|
| Ctrl+S | Save the open scene |
| Ctrl+Z, Ctrl+Y | Undo, redo |
| Ctrl+D | Duplicate the selected node |
| Delete | Delete the selected node |
| F2 | Rename the selected node |
| Q, W, E, S | The Select, Move, Rotate and Scale tools |
| F5, F8 | Play, stop |
| Middle drag, Space+drag | Pan the viewport |
| Wheel | Zoom the viewport |
