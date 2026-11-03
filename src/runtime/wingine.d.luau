-- The types of the engine's API for Luau games, as a definitions file for luau-lsp, which checks
-- scripts against them and shows their docs on hover. The editor hands it to luau-lsp itself; to
-- use it from another editor, see `docs/editor.md`. It follows the reference in `docs/api/`, and
-- `tests/runtime.rs` checks it declares every function the engine's tables hold, and
-- `src/runtime/gui.rs` every method of `Ui`.
--
-- A game's own `update`, `draw` and `gui` aren't declared, since the game defines them. Write
-- `function gui(ui: Ui)`, so luau-lsp knows what `ui` is.

--- A color: a hex string such as `"#f2c14e"` or `"#00000080"`, a table of components from 0 to 1
--- (`a` is 1 if left out), or a number `0xRRGGBBAA`, which passes fastest.
type Color = string | number | ColorComponents

--- A color as components from 0 to 1. `a` is opacity, and 1 if left out.
type ColorComponents = { r: number, g: number, b: number, a: number? }

--- A rectangle: its top-left corner, `x` and `y`, and its size.
type Rect = { x: number, y: number, width: number, height: number }

--- A key's name, such as `"Space"`, `"W"` or `"Digit1"`, in any case. Keys are physical, named for
--- where they are on a US QWERTY keyboard.
type Key = string

--- `"left"`, `"right"`, `"middle"`, `"back"` or `"forward"`, in any case.
type MouseButton = string

--- A gamepad button's name, such as `"South"`, `"Start"` or `"DPadUp"`, or a stick's direction,
--- such as `"LeftStickLeft"`, in any case. Buttons are named for where they are on the gamepad.
type GamepadButton = string

--- A direction on the grid.
type Direction = "up" | "down" | "left" | "right"

--- How `audio.play` plays a sound.
type PlaySettings = {
    --- Linear volume: 1 is as recorded. By default, 1.
    volume: number?,
    --- Playback speed, which changes the pitch with it: 2 is twice as fast and an octave higher.
    --- Above 0, and by default 1.
    speed: number?,
    --- From -1 (left) through 0 (center) to 1 (right). By default, 0.
    pan: number?,
}

--- A kind of tile, for `tileset:add`.
type Tile = {
    --- The sprite drawn, stretched to fill the cell.
    sprite: Sprite,
    --- A color the sprite's colors are multiplied by. By default, white.
    tint: Color?,
    --- Whether a cell holding this tile, on any layer, blocks movement. By default, false.
    solid: boolean?,
}

--- How long the frame before took, in seconds measured on the CPU, and how much it drew.
type FrameStats = {
    --- The whole frame: updating, drawing, rendering, and waiting for vsync and the GPU.
    frame: number,
    --- The frame's updates: `update`, the tasks and native libraries' update hooks.
    update: number,
    --- `draw` and native libraries' draw hooks: recording what to draw.
    draw: number,
    --- Uploading what was drawn to the GPU and submitting draw calls.
    render: number,
    --- Quads drawn: sprites, rectangles and glyphs of text.
    quads: number,
    --- Draw calls: one for each run of quads with the same texture.
    draw_calls: number,
}

--- A window of an interface, for `ui:window`. Positions and sizes are in points: the window's
--- logical pixels, not the canvas's.
type WindowOptions = {
    --- The text in its title bar, which also tells it apart from other windows.
    title: string,
    --- Where its top-left corner starts. It can be dragged elsewhere.
    x: number?,
    y: number?,
    --- The size it starts at.
    width: number?,
    height: number?,
    --- Whether its edges can be dragged to resize it. By default, true.
    resizable: boolean?,
    --- Whether its title bar folds it away. By default, true.
    collapsible: boolean?,
}

--- An edge of the window, which a panel runs along.
type PanelSide = "left" | "right" | "top" | "bottom"

--- A panel of an interface, for `ui:panel`.
type PanelOptions = {
    side: PanelSide,
    --- The width it starts at, or for the top and bottom, its height, in points.
    size: number?,
    --- Whether its inner edge can be dragged to resize it.
    resizable: boolean?,
}

--- A rectangle of an image, to draw, from a sprite sheet.
declare extern type Sprite with
    --- Returns the sprite's width and height in texels.
    function size(self): (number, number)
    --- Returns a copy mirrored left to right.
    function flipped_x(self): Sprite
    --- Returns a copy mirrored top to bottom.
    function flipped_y(self): Sprite
end

--- An image divided into a grid of sprites the same size.
declare extern type SpriteSheet with
    --- Returns the sprite at `column` and `row`, counting from 0 at the top-left. A position
    --- outside the sheet is an error.
    function sprite(self, column: number, row: number): Sprite
    --- Returns how many columns and rows of sprites the sheet has.
    function size(self): (number, number)
end

--- Sprites shown one after another at a steady pace. It doesn't keep time: games keep how long
--- it has played and ask for the frame to draw.
declare extern type Animation with
    --- Returns the sprite showing `time` seconds after the animation started.
    function frame(self, time: number): Sprite
    --- Returns the number of the frame showing at `time`, counting from 1.
    function frame_index(self, time: number): number
    --- Returns how many frames the animation has.
    function frame_count(self): number
    --- Returns the seconds it takes to show every frame once.
    function duration(self): number
    --- Returns a copy that plays once, then holds its last frame.
    function once(self): Animation
    --- Returns whether an animation made with `once` has finished by `time`. One that loops never
    --- finishes.
    function is_finished(self, time: number): boolean
    --- Returns a copy with every frame mirrored left to right.
    function flipped_x(self): Animation
    --- Returns a copy with every frame mirrored top to bottom.
    function flipped_y(self): Animation
end

--- A font to draw text with, such as the built-in pixel font, `graphics.default_font()`.
declare extern type Font with
    --- Returns the font drawn `scale` times as large. Whole numbers keep a pixel font crisp.
    function scaled(self, scale: number): Font
    --- Returns the width and height of `text`, drawn without wrapping. The width runs to the edge
    --- of the ink, so text centers exactly.
    function measure(self, text: string): (number, number)
    --- Returns the distance from one line of text to the next.
    function line_height(self): number
    --- Lays `text` out once, to draw with `graphics.draw_text_layout`. With `max_width`, lines wrap
    --- between words to fit it.
    function layout(self, text: string, max_width: number?): TextLayout
end

--- Text laid out once by `font:layout`, to draw a little at a time.
declare extern type TextLayout with
    --- Returns the width and height the text takes up.
    function size(self): (number, number)
    --- Returns how many glyphs there are to draw. Spaces and line breaks don't count.
    function glyph_count(self): number
    --- Returns how many lines the text takes.
    function line_count(self): number
end

--- A sound effect, decoded into memory, from `audio.load`.
declare extern type Sound with
    --- Returns how long the sound lasts at its normal speed, in seconds.
    function duration(self): number
end

--- Kinds of tile, for building maps with `tilemap.new`.
declare extern type Tileset with
    --- Adds a kind of tile and returns its ID, for `map:set`. IDs count from 0.
    function add(self, tile: Tile): number
end

--- A grid of cells in layers, each holding a tile of its tileset or nothing.
declare extern type Tilemap with
    --- Adds an empty layer on top of the others and returns its number.
    function add_layer(self): number
    --- Returns how many layers the map has.
    function layer_count(self): number
    --- Returns the map's columns and rows.
    function size(self): (number, number)
    --- Returns the ID of the tile in a cell on `layer`, or `nil` if it's empty or outside the map.
    function get(self, layer: number, column: number, row: number): number?
    --- Puts the tile with ID `tile` in a cell on `layer`, or with `nil`, empties it.
    function set(self, layer: number, column: number, row: number, tile: number?): ()
    --- Returns whether a cell is inside the map.
    function contains(self, column: number, row: number): boolean
    --- Returns whether a cell blocks movement: whether it holds a solid tile on any layer, or is
    --- outside the map.
    function is_solid(self, column: number, row: number): boolean
    --- Returns the world area the map covers.
    function bounds(self): Rect
    --- Returns the world position of the top-left corner of a cell, which may be a fraction of the
    --- way between cells.
    function tile_to_world(self, column: number, row: number): (number, number)
    --- Returns the cell containing a world position, which may be outside the map.
    function world_to_tile(self, x: number, y: number): (number, number)
end

--- A character that walks from tile to tile. Call `update` every update, then `step`.
declare extern type GridMover with
    --- Moves the step under way on by `dt` seconds. Call it every update.
    function update(self, dt: number): ()
    --- Turns toward `direction` and starts a step onto the next tile that way, unless `blocked`
    --- returns true for it. Returns whether a step started; while one is under way, does nothing.
    function step(self, direction: Direction, blocked: ((column: number, row: number) -> boolean)?): boolean
    --- Returns the tile the mover stands on, or while stepping, the tile it's stepping onto.
    function tile(self): (number, number)
    --- Returns where the mover is, in tiles: between two during a step.
    function position(self): (number, number)
    --- Returns the direction the mover faces: the way it last stepped, or tried to.
    function facing(self): Direction
    --- Turns the mover to face `direction` without moving.
    function face(self, direction: Direction): ()
    --- Returns whether a step is under way.
    function is_moving(self): boolean
    --- Returns the seconds the mover takes to walk one tile.
    function step_time(self): number
    --- Changes how long walking one tile takes. A step under way keeps the share already walked.
    function set_step_time(self, seconds: number): ()
    --- Moves the mover straight to a tile, ending any step under way.
    function place(self, column: number, row: number): ()
end

--- Tracks of keyframes on nodes' properties, from `scene.clips`, which pose nodes with
--- `node:pose`. It doesn't keep time: games keep how long it has played.
declare extern type Clip with
    --- Returns how many seconds it lasts.
    function duration(self): number
    --- Returns whether it starts over at its end, rather than holding its last pose.
    function is_looping(self): boolean
    --- Returns whether a clip that doesn't loop has reached its end by `time`.
    function is_finished(self, time: number): boolean
end

--- A node of a loaded scene. Every node of a scene shares it, and two of the same node are `==`.
declare extern type SceneNode with
    --- Returns the node's name, unique among its siblings. The root's is `""`.
    function name(self): string
    --- Returns `"node"`, `"sprite"`, `"rect"`, `"text"`, `"area"`, `"tilemap"` or `"tile_layer"`.
    function kind(self): string
    --- Returns the names from the root to the node, separated by `/`.
    function path(self): string
    --- Returns the node `path` leads to from this one, such as `"mill/sails"`, or `nil`.
    function find(self, path: string): SceneNode?
    --- Returns this node and every node inside it with the tag `tag`, in the order they draw.
    function tagged(self, tag: string): { SceneNode }
    --- Returns the node it's inside, or `nil` for the root.
    function parent(self): SceneNode?
    --- Returns the nodes directly inside it, in the order they draw.
    function children(self): { SceneNode }
    --- Returns the node's tags.
    function tags(self): { string }
    --- Returns whether the node has the tag `tag`.
    function has_tag(self, tag: string): boolean
    --- Returns a copy of the node's props, the game's data from the scene file.
    function props(self): { [string]: any }
    --- Returns where its origin is within its parent: from the parent's origin, or its anchor.
    function position(self): (number, number)
    --- Moves its origin within its parent.
    function set_position(self, x: number, y: number): ()
    --- Returns the point of its parent's rectangle its position is from, as fractions of the
    --- rectangle's size, or `nil` if it's placed from its parent's origin.
    function anchor(self): (number?, number?)
    --- Places it from the point `x`, `y` of its parent's rectangle, keeping its position, or with
    --- `nil`, from its parent's origin.
    function set_anchor(self, x: number?, y: number?): ()
    --- Returns how far it's turned about its origin, in radians, clockwise.
    function rotation(self): number
    --- Turns it about its origin, in radians, clockwise.
    function set_rotation(self, angle: number): ()
    --- Returns how much it's stretched along its own axes. Negative scales mirror it.
    function scale(self): (number, number)
    --- Stretches it by `x` and `y`, or by `x` both ways.
    function set_scale(self, x: number, y: number?): ()
    --- Returns where its origin is in the scene, after every node it's inside has placed it.
    function world_position(self): (number, number)
    --- Returns whether it's shown.
    function is_visible(self): boolean
    --- Shows or hides it and everything inside it.
    function set_visible(self, visible: boolean): ()
    --- Returns the width and height it covers, before any scaling.
    function size(self): (number, number)
    --- Changes the size of a sprite, a rect or an area.
    function set_size(self, width: number, height: number): ()
    --- Returns the smallest rectangle in the scene's coordinates around what it covers, or `nil`
    --- for a plain node.
    function bounds(self): Rect?
    --- Returns whether a point in the scene's coordinates is inside what it covers.
    function contains(self, x: number, y: number): boolean
    --- Changes the sprite a sprite node shows. It keeps its size.
    function set_sprite(self, sprite: Sprite): ()
    --- Shows another cell of the sprite node's sheet, counting from 0 at the top-left.
    function set_cell(self, column: number, row: number): ()
    --- Changes the color a sprite node's colors are multiplied by.
    function set_tint(self, tint: Color): ()
    --- Changes the color of a rect or text node.
    function set_color(self, color: Color): ()
    --- Returns a text node's text.
    function text(self): string
    --- Changes a text node's text.
    function set_text(self, text: string): ()
    --- Poses it and the nodes inside it as `clip` has them `time` seconds after it started.
    --- `weight`, from 0 to 1, is how much the clip counts.
    function pose(self, clip: Clip, time: number, weight: number?): ()
    --- Adds a copy of `source` and everything inside it as the last child, named `name` or as
    --- `source` is, and returns the copy.
    function add(self, source: SceneNode, name: string?): SceneNode
    --- Takes the node and everything inside it out of its scene.
    function remove(self): ()
    --- Returns whether the node has been removed.
    function is_removed(self): boolean
    --- Returns whether a cell of the tilemap blocks movement.
    function is_solid(self, column: number, row: number): boolean
    --- Returns the position in the scene of the top-left corner of a cell of the tilemap.
    function tile_to_world(self, column: number, row: number): (number, number)
    --- Returns the cell of the tilemap containing a position in the scene.
    function world_to_tile(self, x: number, y: number): (number, number)
    --- Returns how many columns and rows the tilemap has.
    function grid_size(self): (number, number)
    --- Returns the width and height of the tilemap's tiles.
    function tile_size(self): (number, number)
    --- Returns the props of the tile named `name` in the tilemap's tileset.
    function tile_props(self, name: string): { [string]: any }
    --- Returns the name of the tile in a cell of this tile layer, or `nil` if it's empty.
    function tile(self, column: number, row: number): string?
    --- Puts the tile named `name` in a cell of this tile layer, or with `nil`, empties it.
    function set_tile(self, column: number, row: number, name: string?): ()
end

--- Where an interface is laid out, with egui, in the game's `gui(ui)`: the whole window, or the
--- inside of a container. Widgets that change a value take it and return the new one. It works
--- only until the function it was passed to returns, and inside a container, use the `Ui` the
--- container passes to its function.
declare extern type Ui with
    --- A window over the game, named by its title or with options, which `contents` fills.
    function window(self, window: string | WindowOptions, contents: (Ui) -> ()): ()
    --- A panel along an edge of the window, by its side or with options, which `contents` fills.
    function panel(self, panel: PanelSide | PanelOptions, contents: (Ui) -> ()): ()
    --- Lays out what `contents` adds from left to right.
    function horizontal(self, contents: (Ui) -> ()): ()
    --- Lays out what `contents` adds from top to bottom, as inside `horizontal`.
    function vertical(self, contents: (Ui) -> ()): ()
    --- Frames what `contents` adds.
    function group(self, contents: (Ui) -> ()): ()
    --- A header that opens and closes what `contents` adds, which runs only while it's open.
    function collapsing(self, title: string, contents: (Ui) -> ()): ()
    --- Scrolls what `contents` adds up and down when it doesn't fit.
    function scroll(self, contents: (Ui) -> ()): ()
    --- Lines what `contents` adds up in columns, a row at a time: `ui:end_row()` ends each.
    function grid(self, name: string, contents: (Ui) -> ()): ()
    --- Ends a row of a grid.
    function end_row(self): ()
    --- Greys out what `contents` adds, which can't be used, unless `enabled`.
    function enabled(self, enabled: boolean, contents: (Ui) -> ()): ()
    --- Text, in `color` if given.
    function label(self, text: string, color: Color?): ()
    --- Large text.
    function heading(self, text: string): ()
    --- Text in a monospace font.
    function monospace(self, text: string): ()
    --- Small text.
    function small(self, text: string): ()
    --- Returns whether the button was clicked.
    function button(self, text: string): boolean
    --- Returns whether the box is checked, after any click.
    function checkbox(self, text: string, checked: boolean): boolean
    --- One of a set of buttons choosing between values: returns `value` if it was clicked, and
    --- `current` otherwise. It shows as chosen when `current` is `value`.
    radio: <T>(self: Ui, current: T, value: T, text: string) -> T
    --- A drop-down list of `choices`, shown with `tostring`: returns the one chosen, or `current`.
    combo: <T>(self: Ui, text: string, current: T, choices: { T }) -> T
    --- Returns `value` after any dragging between `min` and `max`, by `step` if given.
    function slider(self, text: string, value: number, min: number, max: number, step: number?): number
    --- A number field, changed by dragging or typing: returns `value` after any change, by
    --- `speed` for each point dragged (1 by default), kept between `min` and `max` if given.
    function drag(self, text: string, value: number, speed: number?, min: number?, max: number?): number
    --- A text field, showing `hint` while empty: returns its text after any typing, and whether
    --- Enter was pressed in it.
    function text_input(self, text: string, hint: string?): (string, boolean)
    --- A bar filled `fraction` of the way, from 0 to 1, with `text` on it.
    function progress(self, fraction: number, text: string?): ()
    --- Shows `sprite`, its colors multiplied by `tint`, at `width` by `height` points, or its own
    --- size.
    function sprite(self, sprite: Sprite, tint: Color?, width: number?, height: number?): ()
    --- A line between what comes before and after.
    function separator(self): ()
    --- Empty space, `amount` points of it, or the usual space between widgets.
    function space(self, amount: number?): ()
end

--- Drawing, the camera, sprites, animations and text. Drawing works only in `draw`.
declare graphics: {
    --- Fills the canvas with `color`, discarding anything drawn so far this frame.
    clear: (color: Color) -> (),
    --- Fills a rectangle with its top-left corner at `x`, `y`.
    fill_rect: (x: number, y: number, width: number, height: number, color: Color) -> (),
    --- Draws `sprite` with its top-left corner at `x`, `y`, stretched to `width` by `height`, with
    --- its colors multiplied by `tint`.
    draw_sprite: (sprite: Sprite, x: number, y: number, tint: Color?, width: number?, height: number?) -> (),
    --- Draws `sprite` as `draw_sprite` does, turned by `angle` radians clockwise about the center
    --- of where it's drawn.
    draw_sprite_rotated: (sprite: Sprite, x: number, y: number, angle: number, tint: Color?, width: number?, height: number?) -> (),
    --- Draws `text` with its top-left corner at `x`, `y`. `"\n"` starts a new line.
    draw_text: (font: Font, text: string, x: number, y: number, color: Color) -> (),
    --- Draws text laid out with `font:layout`, or only its first `glyphs` glyphs.
    draw_text_layout: (layout: TextLayout, x: number, y: number, color: Color, glyphs: number?) -> (),
    --- Draws every layer of `map` in order, or only `layer`, at the world's origin.
    draw_tilemap: (map: Tilemap, layer: number?) -> (),
    --- Draws `node` and every node inside it, laid out on the canvas.
    draw_scene: (node: SceneNode) -> (),
    --- Centers the view on world position `x`, `y` for what's drawn after, kept inside `bounds`,
    --- or with `nil`, goes back to canvas pixels.
    set_camera: ((x: number, y: number, bounds: Rect?) -> ()) & ((nothing: nil) -> ()),
    --- Returns the canvas's width and height in canvas pixels.
    size: () -> (number, number),
    --- Returns `color` as a number `0xRRGGBBAA`, the fastest way to pass it.
    color: (color: Color) -> number,
    --- Returns how long the frame before took, and how much it drew.
    frame_stats: () -> FrameStats,
    --- Loads the PNG image at `path`, divided into sprites `tile_width` by `tile_height` texels.
    load_sprite_sheet: (path: string, tile_width: number, tile_height: number) -> SpriteSheet,
    --- Returns an animation that shows each of `frames` for `frame_time` seconds, then starts over.
    new_animation: (frames: { Sprite }, frame_time: number) -> Animation,
    --- Returns the built-in pixel font.
    default_font: () -> Font,
}

--- The keyboard, mouse and gamepads, and actions bound to keys and buttons. Works while the game
--- loads and in `update`.
declare input: {
    --- Binds `action` to keys and gamepad buttons, replacing what it was bound to before. With
    --- `gamepad`, only that gamepad's buttons count; without, any gamepad's do.
    bind: (action: string, controls: { Key | GamepadButton }, gamepad: number?) -> (),
    --- Returns whether anything bound to `action` is down.
    held: (action: string) -> boolean,
    --- Returns whether anything bound to `action` was pressed for this update.
    pressed: (action: string) -> boolean,
    --- Returns whether anything bound to `action` was released for this update.
    released: (action: string) -> boolean,
    --- Returns how far `action` is pushed, from 0 to 1: 1 while a key bound to it is down, and
    --- otherwise as far as the furthest of its buttons, such as a trigger or a stick's direction.
    strength: (action: string) -> number,
    --- Returns how far `positive` is pushed less how far `negative` is, from -1 to 1.
    axis: (negative: string, positive: string) -> number,
    --- Returns whether `key` is down.
    key_held: (key: Key) -> boolean,
    --- Returns whether `key` was pressed for this update.
    key_pressed: (key: Key) -> boolean,
    --- Returns whether `key` was released for this update.
    key_released: (key: Key) -> boolean,
    --- Returns the cursor's position in canvas pixels, or `nil, nil` outside the window.
    mouse_position: () -> (number?, number?),
    --- Returns whether `button` is down.
    mouse_held: (button: MouseButton) -> boolean,
    --- Returns whether `button` was pressed for this update.
    mouse_pressed: (button: MouseButton) -> boolean,
    --- Returns whether `button` was released for this update.
    mouse_released: (button: MouseButton) -> boolean,
    --- Returns the numbers of the gamepads connected, in order. They count from 1, in the order
    --- gamepads first connect.
    gamepads: () -> { number },
    --- Returns the name of gamepad `gamepad`, or `nil` if it's never been connected.
    gamepad_name: (gamepad: number) -> string?,
    --- Returns whether `button` is down on `gamepad`, or without it, on any gamepad.
    button_held: (button: GamepadButton, gamepad: number?) -> boolean,
    --- Returns whether `button` was pressed for this update on `gamepad`, or on any gamepad.
    button_pressed: (button: GamepadButton, gamepad: number?) -> boolean,
    --- Returns whether `button` was released for this update on `gamepad`, or on any gamepad.
    button_released: (button: GamepadButton, gamepad: number?) -> boolean,
    --- Returns how far `button` is pushed on `gamepad`, or the furthest on any, from 0 to 1.
    button_strength: (button: GamepadButton, gamepad: number?) -> number,
}

--- Sound effects and music, from WAV or OGG Vorbis files. Volumes are linear: 1 is as recorded.
declare audio: {
    --- Loads the sound effect at `path`, decoding all of it into memory.
    load: (path: string) -> Sound,
    --- Plays `sound` once.
    play: (sound: Sound, settings: PlaySettings?) -> (),
    --- Stops every sound effect playing, fading them out over `fade` seconds.
    stop_sounds: (fade: number?) -> (),
    --- Returns how many sound effects are playing.
    sounds_playing: () -> number,
    --- Starts looping the music file at `path`, crossfading from what's playing over `fade` seconds.
    play_music: (path: string, fade: number?) -> (),
    --- Stops the music, fading it out over `fade` seconds.
    stop_music: (fade: number?) -> (),
    --- Returns whether music is playing.
    music_playing: () -> boolean,
    --- Sets the volume of everything: sound effects and music.
    set_volume: (volume: number) -> (),
    --- Sets the volume of sound effects.
    set_sound_volume: (volume: number) -> (),
    --- Sets the volume of music.
    set_music_volume: (volume: number) -> (),
}

--- Tilesets and maps of tiles in layers.
declare tilemap: {
    --- Returns an empty tileset of tiles `tile_width` by `tile_height` world units.
    new_tileset: (tile_width: number, tile_height: number) -> Tileset,
    --- Returns a map `columns` by `rows` cells, with no layers yet, of a copy of `tileset`.
    new: (tileset: Tileset, columns: number, rows: number) -> Tilemap,
}

--- Characters that walk from tile to tile.
declare grid: {
    --- Returns a mover on the tile at `column`, `row`, facing down, that takes `step_time` seconds
    --- a step.
    new_mover: (column: number, row: number, step_time: number) -> GridMover,
    --- Returns the next tile from `column`, `row` in `direction`.
    neighbor: (direction: Direction, column: number, row: number) -> (number, number),
    --- Returns the opposite direction.
    opposite: (direction: Direction) -> Direction,
}

--- Scenes loaded from scene files, and their nodes.
declare scene: {
    --- Loads the scene file at `path` and returns its root, a scene of its own.
    load: (path: string) -> SceneNode,
    --- Returns the clips of the scene file at `path`, by name.
    clips: (path: string) -> { [string]: Clip },
}

--- Data kept between runs of the game, in saves named with letters, digits, `_` and `-`. Saves
--- hold plain data: booleans, numbers, strings, and tables of them that are lists or have strings
--- for keys. Works at any time.
declare save: {
    --- Writes `data` to the save `name`, replacing any.
    write: (name: string, data: any) -> (),
    --- Returns the data in the save `name`, or nil if there's no such save.
    read: (name: string) -> any,
    --- Removes the save `name`, if there is one.
    remove: (name: string) -> (),
    --- Returns the names of the game's saves, in order.
    list: () -> { string },
}

--- Starts `f(...)` as a task, running it until it first waits or finishes. The engine resumes it
--- every update from then on.
declare function spawn<A...>(f: (A...) -> ...any, ...: A...): ()

--- Pauses the task for `seconds` of game time, or until the next update. Works only in tasks.
declare function wait(seconds: number?): ()

--- Pauses the task until `condition()` returns true, checking once each update. Works only in
--- tasks.
declare function wait_until(condition: () -> boolean): ()
