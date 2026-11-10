-- The types of the engine's API for Luau games, as a definitions file for luau-lsp, which checks
-- scripts against them and shows their docs on hover. The editor hands it to luau-lsp itself; to
-- use it from another editor, see `docs/editor.md`. It follows the reference in `docs/api/`, and
-- `tests/runtime.rs` checks it declares every function the engine's tables hold, and
-- `src/runtime/gui.rs` every method of `Ui`.
--
-- A game's own `update`, `draw`, `gui`, `close_requested` and `reloading` aren't declared, since
-- the game defines them. Write `function gui(ui: Ui)`, so luau-lsp knows what `ui` is.

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

--- How a window covers the screen: `"borderless"`, a window without borders covering it, or
--- `"exclusive"`, the screen given over to the game at a video mode of its own. Nil where these
--- take one is a window of its own size.
type Fullscreen = "borderless" | "exclusive"

--- How a fixed resolution fills the window: `"whole"`, scaled up by the largest whole number that
--- fits, with black bars; `"fit"`, as large as fits keeping its shape, filling the window's height
--- or width; or `"stretch"`, filling it both ways.
type Scaling = "whole" | "fit" | "stretch"

--- How what's drawn mixes with what's under it: `"normal"`, covering it as much as it's opaque;
--- `"add"`, adding to it, only ever brightening, for fire and glows; `"multiply"`, multiplying it,
--- only ever darkening, for shadows and night; or `"screen"`, brightening it as much as its color
--- is bright, never past white, for soft light.
type Blend = "normal" | "add" | "multiply" | "screen"

--- What an emitter of particles sends out, how often, and how its particles move and look as they
--- age: for `graphics.new_particles` and particles nodes. Ranges are `{ least, most }`, each
--- particle getting a value between them at random, or one number for every particle. Lists over a
--- particle's life are spread evenly from its start to its end.
type ParticleSettings = {
    --- Whether it sends particles out at its `rate`. By default, true.
    emitting: boolean?,
    --- Particles a second, while emitting. By default, 10.
    rate: number?,
    --- The most particles alive at once. By default, 500.
    limit: number?,
    --- Seconds it runs for as it first advances, so a fire is already burning. By default, 0.
    warmup: number?,
    --- Where its randomness starts. By default, the emitter's own.
    seed: number?,
    --- Seconds a particle lives, above 0. By default, 1.
    lifetime: (number | { number })?,
    --- How fast a particle starts, in units a second. By default, 20.
    speed: (number | { number })?,
    --- The way particles fly, in radians clockwise from right, as the emitter is turned: `-math.pi
    --- / 2` is up. By default, 0.
    direction: number?,
    --- How many radians wide the ways they fly are, centered on `direction`. By default, a full
    --- turn, every way.
    spread: number?,
    --- How much faster particles fly each second, `{ x, y }`. By default, `{ 0, 0 }`.
    gravity: { number }?,
    --- How quickly particles slow down: each second, they keep `e^-drag` of their speed. By
    --- default, 0.
    drag: number?,
    --- How far a particle is turned as it starts, in radians, clockwise. By default, 0.
    angle: (number | { number })?,
    --- How fast a particle turns, in radians a second, clockwise. By default, 0.
    spin: (number | { number })?,
    --- A particle's sizes over its life: times its sprite's size, or its width without one. By
    --- default, `{ 1 }`.
    scales: { number }?,
    --- A particle's colors over its life, multiplying its sprite's. By default, white.
    colors: { Color }?,
    --- How particles mix with what's under them. By default, `"normal"`.
    blend: Blend?,
    --- Whether particles move with the emitter after they've left it. By default, false: they stay
    --- where they were sent out, so a moving emitter leaves a trail.
    attached: boolean?,
}

--- How `audio.play` plays a sound.
type PlaySettings = {
    --- Linear volume: 1 is as recorded. By default, 1.
    volume: number?,
    --- Playback speed, which changes the pitch with it: 2 is twice as fast and an octave higher.
    --- Above 0, and by default 1.
    speed: number?,
    --- From -1 (left) through 0 (center) to 1 (right). By default, 0.
    pan: number?,
    --- Whether the sound plays over and over until its voice stops it. By default, false.
    looping: boolean?,
    --- Seconds it fades in over from silence. By default, 0: at once.
    fade_in: number?,
}

--- How `graphics.load_font` draws a TrueType or OpenType font.
type FontSettings = {
    --- How many pixels its em is. A pixel font made as a TrueType font is drawn at the size it was
    --- made for, or a whole number of times it, to fit its pixels.
    size: number,
    --- Whether pixels its outlines partly cover are partly opaque, or each opaque or clear, so a
    --- pixel font stays crisp. By default, true.
    smooth: boolean?,
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
    --- Triangles of the meshes and billboards drawn in 3D views.
    triangles: number,
    --- Draw calls: one for each run of quads with the same texture, and one for each kind of mesh,
    --- or billboard, each 3D view drew: the same mesh showing the same texture.
    draw_calls: number,
}

--- A camera looking into a 3D world, for a `View3d`. 3D coordinates are right-handed with `y` up:
--- looking along `-z`, `x` points right and `z` toward the camera.
type Camera3d = {
    --- Where the camera is.
    position: vector,
    --- The point it looks at, which shows in the middle of the view.
    target: vector,
    --- Which way is up the view, as near as it can be. By default, `vector.create(0, 1, 0)`.
    up: vector?,
    --- With perspective, the angle the view takes in from its bottom edge to its top, in
    --- radians. By default, 0.8.
    fov: number?,
    --- Without perspective, how many world units the view shows from its bottom edge to its top.
    --- A camera has a `fov` or a `height`, not both.
    height: number?,
    --- How near to the camera things show. By default, 0.1.
    near: number?,
    --- How far from the camera things show. By default, 1000.
    far: number?,
}

--- How a 3D view is lit: by a sun, from one direction, and ambient light from all around. White
--- light shows surfaces as they are, and the two together light them no brighter than that.
type Lighting = {
    --- The way the sun's light goes. By default, down from above, a little to the right and in
    --- front: `vector.create(-0.3, -1, -0.5)`.
    direction: vector?,
    --- The sun's light, on surfaces facing it. By default, `"#cccccc"`.
    color: Color?,
    --- The light on every surface. By default, `"#808080"`.
    ambient: Color?,
}

--- A 3D view, for `graphics.begin_3d`, `graphics.project` and `graphics.ray`.
type View3d = {
    camera: Camera3d,
    --- Where on the canvas the view shows. By default, the whole canvas.
    rect: Rect?,
    --- By default, the default sun and ambient light.
    lighting: Lighting?,
}

--- How `graphics.draw_mesh` places and colors a mesh: scaled, rolled, pitched and turned, in that
--- order, about its origin.
type MeshOptions = {
    --- Radians to turn about `y`, counter-clockwise seen from above.
    yaw: number?,
    --- Radians to turn about `x`, counter-clockwise seen from `+x`.
    pitch: number?,
    --- Radians to turn about `z`, counter-clockwise seen from `+z`.
    roll: number?,
    --- A scale along every axis, or a vector of one for each.
    scale: (number | vector)?,
    --- The sprite the mesh shows. By default, none: white.
    sprite: Sprite?,
    --- A color the mesh's colors are multiplied by. By default, white.
    tint: Color?,
}

--- A corner of a mesh's triangles, for `graphics.new_mesh`.
type MeshVertex = {
    position: vector,
    --- Which way the surface faces here, for lighting. By default, up.
    normal: vector?,
    --- Where on the sprite this corner shows, from 0 at its left and top to 1 at its right and
    --- bottom. By default, 0.
    u: number?,
    v: number?,
    --- Multiplies the sprite's colors here. By default, white.
    color: Color?,
}

--- The triangles of a mesh, for `graphics.new_mesh`.
type MeshData = {
    vertices: { MeshVertex },
    --- Three vertices for each triangle, counting from 1, counter-clockwise seen from the side it
    --- faces. Triangles show only from that side.
    indices: { number },
}

--- A 2D physics world, for `physics.world`.
type WorldOptions = {
    --- How many of the scene's units make a metre, which Rapier tunes itself for and densities
    --- are of a square of. By default, 16.
    metre: number?,
    --- How fast dynamic bodies falling speed up each second, down the scene, in its units. By
    --- default, 9.81 metres' worth, as Earth's gravity does; 0 for a game seen from above.
    gravity: number?,
}

--- How `world:add` makes a node solid in 2D, or gives it a body. A tile map or a tile layer is
--- solid where its solid tiles are, and takes none of these.
type ColliderOptions = {
    --- Whether only the node's edges are solid, keeping what's inside in and what's outside out,
    --- rather than all of it. Only for a fixed box. By default, false.
    edges: boolean?,
    --- Makes it a sensor: an area nothing bumps into, which `world:inside` tells what's in. Fixed
    --- or kinematic, and not with `edges`.
    sensor: boolean?,
    --- How it moves: not at all (`"fixed"`, the default), as it's pushed and falls
    --- (`"dynamic"`), or where its node is put (`"kinematic"`, a character's).
    body: ("fixed" | "dynamic" | "kinematic")?,
    --- What it's shaped as, from what its node covers: that (`"box"`, the default), or the circle
    --- inside it as wide as its widest side (`"circle"`).
    shape: ("box" | "circle")?,
    --- How heavy each square metre of it is. By default, 1.
    density: number?,
    --- How much it grips what slides along it, from 0 for ice. By default, 0.5.
    friction: number?,
    --- How much it bounces, from 0 for not at all to 1 for as fast as it hit. By default, 0.
    bounce: number?,
    --- Makes it a character, which `world:walk` walks: an upright box, kinematic.
    character: CharacterOptions2d?,
}

--- A 2D character's size and how it walks, for `ColliderOptions.character`.
type CharacterOptions2d = {
    --- How wide its box is. By default, as wide as what its node covers.
    width: number?,
    --- How tall it is, from its feet, its node's position. By default, as tall as what its node
    --- covers.
    height: number?,
    --- The highest it steps up, as onto a stair. By default, a third of its height.
    step: number?,
    --- The steepest slope it walks up and stands on, in radians. By default, 45 degrees
    --- (`math.pi / 4`).
    slope: number?,
    --- How heavy it is, for how hard it pushes dynamic bodies. By default, 1.
    mass: number?,
}

--- A 3D physics world, for `physics.world_3d`.
type World3dOptions = {
    --- How fast dynamic bodies falling speed up each second. By default, 9.81 down, as Earth's
    --- gravity does in metres.
    gravity: vector?,
}

--- How `world:add` gives a node in a 3D view a body.
type BodyOptions = {
    --- How it moves: not at all (`"fixed"`, the default), as it's pushed and falls
    --- (`"dynamic"`), or where its node is put (`"kinematic"`, a character's).
    body: ("fixed" | "dynamic" | "kinematic")?,
    --- What it's shaped as, from what its node shows: its triangles (`"triangles"`, by default
    --- for fixed bodies, and only for them), the convex shape around them (`"hull"`, by default
    --- for the rest), the box around them (`"box"`), or the ball inside that box as wide as its
    --- widest side (`"ball"`).
    shape: ("triangles" | "hull" | "box" | "ball")?,
    --- How heavy each unit of its volume is. By default, 1.
    density: number?,
    --- How much it grips what slides along it, from 0 for ice. By default, 0.5.
    friction: number?,
    --- How much it bounces, from 0 for not at all to 1 for as fast as it hit. By default, 0.
    bounce: number?,
    --- Makes it a character, which `world:walk` walks: an upright cylinder, kinematic.
    character: CharacterOptions?,
    --- Makes it a sensor: an area nothing bumps into, which `world:inside` tells what's in.
    --- Fixed or kinematic, and shaped as a hull (by default), a box or a ball.
    sensor: boolean?,
}

--- How `world:ray` looks, in 2D or 3D.
type RayOptions = {
    --- How far it goes, in lengths of its direction. By default, as far as there is.
    distance: number?,
    --- A node, or a list of them, whose colliders or bodies it goes through.
    except: (SceneNode | { SceneNode })?,
}

--- How a world's overlaps look: `overlap_circle`, `overlap_ball` and `overlap_box`.
type OverlapOptions = {
    --- A node, or a list of them, whose colliders or bodies don't count.
    except: (SceneNode | { SceneNode })?,
}

--- Where a 2D world's `world:ray` hit a collider, in the scene's coordinates.
type RayHit = {
    --- The node whose collider it hit.
    node: SceneNode,
    --- How far along the ray, in lengths of its direction.
    distance: number,
    --- Where.
    x: number,
    y: number,
    --- The way out of the collider there, of length 1, facing back along the ray, or zero if the
    --- ray started inside the collider.
    normal_x: number,
    normal_y: number,
}

--- Where a 3D world's `world:ray` hit a body.
type RayHit3d = {
    --- The node whose body it hit.
    node: SceneNode,
    --- How far along the ray, in lengths of its direction.
    distance: number,
    --- Where, in the view's world.
    point: vector,
    --- The way out of the body there, of length 1, facing back along the ray, or zero if the ray
    --- started inside the body.
    normal: vector,
}

--- A character's size and how it walks, for `BodyOptions.character`.
type CharacterOptions = {
    --- How far it reaches out from its middle. By default, half the widest its node's box is
    --- across the ground.
    radius: number?,
    --- How tall it is. By default, as tall as its node's box.
    height: number?,
    --- The highest it steps up, as onto a stair. By default, a third of its height.
    step: number?,
    --- The steepest slope it walks up and stands on, in radians. By default, 45 degrees
    --- (`math.pi / 4`).
    slope: number?,
    --- How heavy it is, for how hard it pushes dynamic bodies. By default, 1.
    mass: number?,
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

--- How `ui.menu` finds its items and takes input. The actions are named as `input.bind` binds
--- them, and one that isn't bound does nothing.
type MenuOptions = {
    --- Makes the items the nodes inside the menu's node with this tag, in the order they draw,
    --- rather than its children.
    tag: string?,
    --- Whether moving past the last item that way goes round to the farthest the other way. By
    --- default, true.
    wrap: boolean?,
    --- Whether the mouse moving onto an item selects it, and clicking one picks it. By default,
    --- true.
    mouse: boolean?,
    --- The actions that move the selection, by default `"up"`, `"down"`, `"left"` and `"right"`.
    up: string?,
    down: string?,
    left: string?,
    right: string?,
    --- The action that picks the selected item, by default `"confirm"`.
    confirm: string?,
    --- The action that cancels the menu, by default `"cancel"`.
    cancel: string?,
}

--- What happened to a menu in an update.
type MenuEvent = "moved" | "picked" | "cancelled"

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

--- A mesh uploaded to the GPU, to draw in 3D views.
declare extern type Mesh with
    --- Returns how many triangles it has.
    function triangle_count(self): number
    --- Returns the least and greatest corners of the box its vertices take up, or nothing if it
    --- has none.
    function bounds(self): (vector?, vector?)
end

--- Meshes placed together, each showing a sprite or none, loaded from a model file.
declare extern type Model with
    --- Returns how many triangles its parts have together.
    function triangle_count(self): number
    --- Returns the least and greatest corners of the box its parts take up, placed in it, or
    --- nothing if they have no vertices.
    function bounds(self): (vector?, vector?)
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

--- An emitter of particles, from `graphics.new_particles`, which `graphics.draw_particles` draws.
--- It doesn't keep time: games advance it.
declare extern type Particles with
    --- Sends `count` particles out at once from where it is, emitting or not, as many as its
    --- limit leaves room for.
    function emit(self, count: number): ()
    --- Moves its particles on by `dt` seconds, ending those whose lives are over, and sends new
    --- ones out at its rate while emitting, spread along the way it moved since it last advanced.
    function advance(self, dt: number): ()
    --- Returns how many particles are alive.
    function count(self): number
    --- Ends every particle's life at once.
    function clear(self): ()
    --- Returns whether it sends particles out at its rate.
    function is_emitting(self): boolean
    --- Starts or stops sending particles out at its rate. Those it sent live out their lives.
    function set_emitting(self, emitting: boolean): ()
    --- Returns where it sends particles out from.
    function position(self): (number, number)
    --- Moves where it sends particles out from.
    function set_position(self, x: number, y: number): ()
    --- Turns it, in radians, clockwise, which turns the way its particles fly.
    function set_rotation(self, angle: number): ()
    --- Returns its settings, every one of them.
    function settings(self): ParticleSettings
    --- Changes the settings `settings` has, keeping the rest and its particles.
    function set_settings(self, settings: ParticleSettings): ()
    --- Changes what its particles show, or with `nil`, white squares.
    function set_sprite(self, sprite: Sprite?): ()
end

--- A font to draw text with: the built-in pixel font, `graphics.default_font()`, or one from
--- `graphics.load_font`.
declare extern type Font with
    --- Returns the font drawn `scale` times as large. Whole numbers keep a pixel font crisp; a
    --- TrueType or OpenType font is rasterized at the new size, so it stays sharp at any.
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

--- A sound effect playing, from `audio.play`, by which it's changed, paused or stopped as it
--- plays. Once it's over, changing it does nothing. Its methods work where `audio`'s do.
declare extern type Voice with
    --- Stops the sound, fading it out over `fade` seconds (by default, at once). It can't start
    --- again.
    function stop(self, fade: number?): ()
    --- Pauses the sound, fading it out over `fade` seconds, until it's resumed.
    function pause(self, fade: number?): ()
    --- Resumes the sound where it paused, fading it in over `fade` seconds.
    function resume(self, fade: number?): ()
    --- Changes the sound's volume over `fade` seconds.
    function set_volume(self, volume: number, fade: number?): ()
    --- Changes the sound's speed, and its pitch with it, over `fade` seconds. Above 0.
    function set_speed(self, speed: number, fade: number?): ()
    --- Moves the sound between the speakers, from -1 (left) to 1 (right), over `fade` seconds.
    function set_pan(self, pan: number, fade: number?): ()
    --- Returns whether the sound hasn't finished or been stopped. A paused one is still playing.
    function is_playing(self): boolean
    --- Returns whether the sound is paused.
    function is_paused(self): boolean
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
    --- Returns `"node"`, `"sprite"`, `"rect"`, `"text"`, `"area"`, `"stack"`, `"panel"`,
    --- `"particles"`, `"tilemap"`, `"tile_layer"`, `"view3d"`, `"mesh"`, `"model"` or
    --- `"billboard"`.
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
    --- Returns how far it's turned about its origin, in radians, clockwise. Not for nodes in 3D
    --- views, which turn by `set_orientation`.
    function rotation(self): number
    --- Turns it about its origin, in radians, clockwise.
    function set_rotation(self, angle: number): ()
    --- Returns where the origin of a node in a 3D view is within its parent: `position`'s `x` and
    --- `y`, and its `z`.
    function position_3d(self): vector
    --- Moves the origin of a node in a 3D view within its parent.
    function set_position_3d(self, position: vector): ()
    --- Returns how a node in a 3D view is turned about its origin: its yaw, pitch and roll.
    function orientation(self): (number, number, number)
    --- Turns a node in a 3D view about its origin: by `roll` about `z`, `pitch` about `x` and
    --- `yaw` about `y`, in that order, in radians, counter-clockwise seen from the axis's end.
    function set_orientation(self, yaw: number, pitch: number?, roll: number?): ()
    --- Returns how much a node in a 3D view is stretched along its own axes.
    function scale_3d(self): vector
    --- Stretches a node in a 3D view by a vector, or a number for every axis.
    function set_scale_3d(self, scale: number | vector): ()
    --- Returns where a node in a 3D view is in its view's world.
    function world_position_3d(self): vector
    --- Returns the least and greatest corners of the box in its view's world around what a mesh,
    --- model or billboard shows, or nothing.
    function bounds_3d(self): (vector?, vector?)
    --- Returns a 3D view's camera.
    function camera(self): Camera3d
    --- Changes a 3D view's camera.
    function set_camera(self, camera: Camera3d): ()
    --- Returns a 3D view's lighting, its colors as numbers.
    function lighting(self): Lighting
    --- Changes a 3D view's lighting.
    function set_lighting(self, lighting: Lighting): ()
    --- Returns the nodes of a 3D view under a point in the scene's coordinates, nearest first.
    function pick(self, x: number, y: number): { SceneNode }
    --- Returns where a point in a 3D view's world shows in the scene, or nothing if it's behind
    --- the camera.
    function project(self, point: vector): (number?, number?)
    --- Returns the ray into a 3D view's world through a point in the scene's coordinates: where it
    --- starts and the way it goes, of length 1, or nothing outside the view.
    function ray(self, x: number, y: number): (vector?, vector?)
    --- Returns how much it's stretched along its own axes. Negative scales mirror it.
    function scale(self): (number, number)
    --- Stretches it by `x` and `y`, or by `x` both ways.
    function set_scale(self, x: number, y: number?): ()
    --- Returns where its origin is in the scene, after every node it's inside has placed it. An
    --- error for a node in a 3D view: see `world_position_3d`.
    function world_position(self): (number, number)
    --- Returns whether it's shown.
    function is_visible(self): boolean
    --- Shows or hides it and everything inside it.
    function set_visible(self, visible: boolean): ()
    --- Returns the width and height it covers, before any scaling, or a billboard's.
    function size(self): (number, number)
    --- Changes the size of a sprite, a rect, an area, a 3D view, a billboard, or the rectangle
    --- particles start in.
    function set_size(self, width: number, height: number): ()
    --- Returns the smallest rectangle in the scene's coordinates around what it covers, or `nil`
    --- for a plain node.
    function bounds(self): Rect?
    --- Returns whether a point in the scene's coordinates is inside what it covers.
    function contains(self, x: number, y: number): boolean
    --- Changes the sprite a sprite node shows, which keeps its size, or a particles node's
    --- particles show.
    function set_sprite(self, sprite: Sprite): ()
    --- Shows another cell of the sheet of a sprite, a billboard, a mesh or particles, counting from
    --- 0 at the top-left.
    function set_cell(self, column: number, row: number): ()
    --- Changes the color the colors of a sprite, mesh, model or billboard are multiplied by.
    function set_tint(self, tint: Color): ()
    --- Changes the color of a rect or text node.
    function set_color(self, color: Color): ()
    --- Returns how a sprite, rect, text, tile layer or particles node mixes with what's under it.
    function blend(self): Blend
    --- Changes how a sprite, rect, text, tile layer or particles node mixes with what's under it.
    function set_blend(self, blend: Blend): ()
    --- Sends `count` of a particles node's particles out at once from where it is, emitting or
    --- not.
    function emit(self, count: number): ()
    --- Returns whether a particles node sends particles out at its rate.
    function is_emitting(self): boolean
    --- Starts or stops a particles node sending particles out at its rate.
    function set_emitting(self, emitting: boolean): ()
    --- Returns a particles node's settings, every one of them.
    function particle_settings(self): ParticleSettings
    --- Changes the settings of a particles node that `settings` has, keeping the rest.
    function set_particle_settings(self, settings: ParticleSettings): ()
    --- Moves the particles of this node and every particles node inside it on by `dt` seconds,
    --- sending new ones out from where each node is now. Scenes keep no time, so games call it.
    function advance_particles(self, dt: number): ()
    --- Returns how many particles this node and every particles node inside it have alive.
    function particle_count(self): number
    --- Ends the lives of the particles of this node and every particles node inside it.
    function clear_particles(self): ()
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

--- Colliders covering nodes of one scene as they're drawn, or tile maps' solid tiles, and bodies,
--- which follow the nodes as they move and go when they do, nodes moved and characters walked
--- among them, from `physics.world()`. Dynamic bodies move their nodes as the world steps.
declare extern type PhysicsWorld with
    --- Makes `node` solid where it's drawn, turned and stretched as it is, or with `edges`, only
    --- along its edges, or gives it a body. A tile map is solid where its solid tiles are. Adding
    --- a node again changes how solid it is.
    function add(self, node: SceneNode, options: ColliderOptions?): ()
    --- Makes `node` solid nowhere, if it was.
    function remove(self, node: SceneNode): ()
    --- Moves the world on by `dt` seconds: dynamic bodies fall, roll and come to rest, moving and
    --- turning their nodes, and kinematic bodies go where their nodes are.
    function step(self, dt: number): ()
    --- Walks the character `node` by up to `dx`, `dy`, along the ground, up steps and slopes, and
    --- along with what it stands on, and returns how far it moved and whether it's on the ground.
    function walk(self, node: SceneNode, dx: number, dy: number): (number, number, boolean)
    --- How fast `node`'s body is moving, in units a second.
    function velocity(self, node: SceneNode): (number, number)
    --- Sets how fast `node`'s dynamic body is moving.
    function set_velocity(self, node: SceneNode, vx: number, vy: number): ()
    --- Pushes `node`'s dynamic body through its middle, changing its velocity by the impulse over
    --- its mass.
    function push(self, node: SceneNode, x: number, y: number): ()
    --- Whether `node`'s body is at rest.
    function is_resting(self, node: SceneNode): boolean
    --- Moves `node` by up to `dx`, `dy` in the scene, as `box` around its position does, stopping
    --- at what's solid and sliding along it, and returns how far it moved.
    function move(self, node: SceneNode, dx: number, dy: number, box: Rect): (number, number)
    --- Returns where a ray from `x`, `y` going `dx`, `dy` first hits a collider, or nil. Sensors
    --- are never hit.
    function ray(self, x: number, y: number, dx: number, dy: number, options: RayOptions?): RayHit?
    --- Returns the nodes whose colliders touch a circle, in the order they were added.
    function overlap_circle(self, x: number, y: number, radius: number, options: OverlapOptions?): { SceneNode }
    --- Returns the nodes whose colliders touch a box in the scene, in the order they were added.
    function overlap_box(self, box: Rect, options: OverlapOptions?): { SceneNode }
    --- Returns the nodes whose colliders are in the sensor `sensor`'s, even partly, in the order
    --- they were added.
    function inside(self, sensor: SceneNode): { SceneNode }
end

--- Bodies of nodes of one 3D view, shaped like what they show, from `physics.world_3d()`: fixed
--- and kinematic bodies follow their nodes, dynamic bodies move their nodes as the world steps,
--- and characters walk their nodes among them. Bodies go when their nodes do.
declare extern type PhysicsWorld3d with
    --- Gives `node` a body, or a new one if it has one. Nodes around a body that moves may move
    --- it, but not turn or stretch it.
    function add(self, node: SceneNode, options: BodyOptions?): ()
    --- Takes away `node`'s body, if it has one.
    function remove(self, node: SceneNode): ()
    --- Moves the world on by `dt` seconds: dynamic bodies fall, tumble and come to rest, moving
    --- their nodes, and kinematic bodies move to their nodes.
    function step(self, dt: number): ()
    --- Walks the character `node` by up to `motion`: across the ground, up stairs and slopes it
    --- can walk up, along walls, pushing dynamic bodies; or off the ground, or rising, as `motion`
    --- says. Returns how far it moved, and whether it's on the ground.
    function walk(self, node: SceneNode, motion: vector): (vector, boolean)
    --- Returns how fast `node`'s body is moving, in units a second.
    function velocity(self, node: SceneNode): vector
    --- Sets how fast `node`'s dynamic body is moving.
    function set_velocity(self, node: SceneNode, velocity: vector): ()
    --- Pushes `node`'s dynamic body through its middle, changing its velocity by `impulse` over
    --- its mass.
    function push(self, node: SceneNode, impulse: vector): ()
    --- Returns whether `node`'s body is at rest: always for a fixed body, and for a dynamic one
    --- once it has stopped.
    function is_resting(self, node: SceneNode): boolean
    --- Returns where a ray from `origin` going `direction` first hits a body, by its shape, or
    --- nil. Sensors are never hit.
    function ray(self, origin: vector, direction: vector, options: RayOptions?): RayHit3d?
    --- Returns the nodes whose bodies touch a ball, in the order they were added.
    function overlap_ball(self, center: vector, radius: number, options: OverlapOptions?): { SceneNode }
    --- Returns the nodes whose bodies touch a box along the world's axes, between two corners, in
    --- the order they were added.
    function overlap_box(self, min: vector, max: vector, options: OverlapOptions?): { SceneNode }
    --- Returns the nodes whose bodies are in the sensor `sensor`'s, even partly, in the order they
    --- were added.
    function inside(self, sensor: SceneNode): { SceneNode }
end

--- A selection among a node's items, its children or the nodes inside it with a tag, those shown,
--- from `ui.menu`. Items are numbered from 1 in order, and found again each call, so they may come
--- and go; the selection stays on its item while it's there, and on one that can be picked.
declare extern type Menu with
    --- Takes the player's input, in `update`, and returns what they did, if anything, and to
    --- which item: `"moved"` the selection to it, by an action or the mouse moving onto it,
    --- `"picked"` it, by the confirm action or a click, or `"cancelled"` the menu.
    function update(self): (MenuEvent?, number?)
    --- Returns the selected item's number, or nil without an item that can be picked.
    function selected(self): number?
    --- Selects item `item`, or if it can't be picked, the next that can.
    function select(self, item: number): ()
    --- Returns the items, in order.
    function items(self): { SceneNode }
    --- Sets whether item `item` can be selected and picked. Every item can, until this says not.
    function set_enabled(self, item: number, enabled: boolean): ()
    --- Returns whether item `item` can be selected and picked.
    function is_enabled(self, item: number): boolean
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
    --- A button showing `sprite` as `ui:sprite` does, in a frame, named `text`, which shows when
    --- it's hovered: returns whether it was clicked. It's framed as chosen when `selected`.
    function sprite_button(self, sprite: Sprite, text: string, selected: boolean?, tint: Color?, width: number?, height: number?): boolean
    --- A line between what comes before and after.
    function separator(self): ()
    --- Empty space, `amount` points of it, or the usual space between widgets.
    function space(self, amount: number?): ()
end

--- Drawing, the camera, blend modes, sprites, animations, particles, text and 3D views. Drawing
--- works only in `draw`.
declare graphics: {
    --- Fills the canvas with `color`, discarding anything drawn so far this frame.
    clear: (color: Color) -> (),
    --- Fills a rectangle with its top-left corner at `x`, `y`.
    fill_rect: (x: number, y: number, width: number, height: number, color: Color) -> (),
    --- Draws a rectangle's outline, `thickness` wide (1 by default) inside its edges.
    outline_rect: (x: number, y: number, width: number, height: number, color: Color, thickness: number?) -> (),
    --- Draws a line from `x1`, `y1` to `x2`, `y2`, `thickness` wide (1 by default).
    draw_line: (x1: number, y1: number, x2: number, y2: number, color: Color, thickness: number?) -> (),
    --- Fills a circle of `radius` about `x`, `y`.
    fill_circle: (x: number, y: number, radius: number, color: Color) -> (),
    --- Draws a circle's outline, `thickness` wide (1 by default) inside its edge.
    outline_circle: (x: number, y: number, radius: number, color: Color, thickness: number?) -> (),
    --- Draws `sprite` with its top-left corner at `x`, `y`, stretched to `width` by `height`, with
    --- its colors multiplied by `tint`.
    draw_sprite: (sprite: Sprite, x: number, y: number, tint: Color?, width: number?, height: number?) -> (),
    --- Draws `sprite` as `draw_sprite` does, turned by `angle` radians clockwise about the center
    --- of where it's drawn.
    draw_sprite_rotated: (sprite: Sprite, x: number, y: number, angle: number, tint: Color?, width: number?, height: number?) -> (),
    --- Draws `sprite` sliced to fill `width` by `height`, as a frame is drawn at any size: the
    --- `slice` texels along its edges (one number for all four, or `{ left, top, right, bottom }`)
    --- keep their size, so its corners stay as they are and the rest stretches.
    draw_sprite_sliced: (sprite: Sprite, x: number, y: number, width: number, height: number, slice: number | { number }, tint: Color?) -> (),
    --- Draws `text` with its top-left corner at `x`, `y`. `"\n"` starts a new line.
    draw_text: (font: Font, text: string, x: number, y: number, color: Color) -> (),
    --- Draws text laid out with `font:layout`, or only its first `glyphs` glyphs.
    draw_text_layout: (layout: TextLayout, x: number, y: number, color: Color, glyphs: number?) -> (),
    --- Draws every layer of `map` in order, or only `layer`, at the world's origin.
    draw_tilemap: (map: Tilemap, layer: number?) -> (),
    --- Draws `node` and every node inside it, laid out on the canvas.
    draw_scene: (node: SceneNode) -> (),
    --- Centers the view on world position `x`, `y` for what's drawn after, kept inside `bounds`,
    --- at `zoom` canvas pixels a world unit (1 by default), or with `nil`, goes back to canvas
    --- pixels.
    set_camera: ((x: number, y: number, bounds: Rect?, zoom: number?) -> ()) & ((nothing: nil) -> ()),
    --- Blends what's drawn after with what's under it as `blend` says, until the frame ends.
    set_blend: (blend: Blend) -> (),
    --- Returns an emitter of particles with `settings`, each showing `sprite` or a white square.
    --- It works anywhere.
    new_particles: (settings: ParticleSettings?, sprite: Sprite?) -> Particles,
    --- Draws the particles of `particles`, in its blend mode.
    draw_particles: (particles: Particles) -> (),
    --- Returns the canvas's width and height in canvas pixels.
    size: () -> (number, number),
    --- Returns `color` as a number `0xRRGGBBAA`, the fastest way to pass it, with its alpha
    --- multiplied by `opacity` if given, as for fading it.
    color: (color: Color, opacity: number?) -> number,
    --- Returns how long the frame before took, and how much it drew.
    frame_stats: () -> FrameStats,
    --- Loads the PNG image at `path`, divided into sprites `tile_width` by `tile_height` texels.
    load_sprite_sheet: (path: string, tile_width: number, tile_height: number) -> SpriteSheet,
    --- Returns an animation that shows each of `frames` for `frame_time` seconds, then starts over.
    new_animation: (frames: { Sprite }, frame_time: number) -> Animation,
    --- Returns the built-in pixel font.
    default_font: () -> Font,
    --- Loads a font. Without `settings`, `path` is a font file (`*.font.luau`): a bitmap font,
    --- its glyphs drawn in a PNG, or a TrueType or OpenType font at a size. With them, it's a
    --- `.ttf` or `.otf` file, drawn as they say.
    load_font: (path: string, settings: FontSettings?) -> Font,
    --- Begins a 3D view, over what was drawn before. Meshes and billboards drawn after go in it,
    --- until anything else is drawn, which goes over it.
    begin_3d: (view: View3d) -> (),
    --- Draws `mesh` in the 3D view begun last, its origin at `position`.
    draw_mesh: (mesh: Mesh, position: vector, options: MeshOptions?) -> (),
    --- Draws `model`'s parts in the 3D view begun last, its origin at `position`, placed by
    --- `options` as a mesh is. Its parts show their own sprites, so `options.sprite` is an error.
    draw_model: (model: Model, position: vector, options: MeshOptions?) -> (),
    --- Draws `sprite` standing upright in the 3D view begun last, facing the camera, with the
    --- middle of its bottom edge at `position`: `height` world units tall (1 by default), and
    --- `width` wide (by default, as its shape makes it). A negative `width` mirrors it.
    draw_billboard: (sprite: Sprite, position: vector, tint: Color?, width: number?, height: number?) -> (),
    --- Returns where `point` shows on the canvas through `view`, or nothing if it's nearer than
    --- the camera's near plane, such as behind it, or beyond its far plane.
    project: (view: View3d, point: vector) -> (number?, number?),
    --- Returns the ray into `view` through `x`, `y` on the canvas: where it starts, on the near
    --- plane, and the way it goes, of length 1.
    ray: (view: View3d, x: number, y: number) -> (vector, vector),
    --- Returns how far along the ray from `origin` going `direction` it enters the box between
    --- corners `min` and `max`, in lengths of `direction`: 0 if it starts inside, or nothing if
    --- it misses.
    hit_box: (origin: vector, direction: vector, min: vector, max: vector) -> number?,
    --- Loads the model file at `path`, glTF 2.0 as `.glb` or `.gltf`. Each file is read once.
    load_model: (path: string) -> Model,
    --- Returns a mesh of `data`'s triangles.
    new_mesh: (data: MeshData) -> Mesh,
    --- Returns a cube `size` along each side, centered on its origin, showing the whole sprite on
    --- each side.
    new_cube: (size: number) -> Mesh,
    --- Returns a box `width`, `height` and `depth` along `x`, `y` and `z`, as `new_cube` does.
    new_cuboid: (width: number, height: number, depth: number) -> Mesh,
    --- Returns a flat rectangle facing up, `width` along `x` and `depth` along `z`, centered on
    --- its origin, showing the sprite with its top toward `-z`.
    new_plane: (width: number, depth: number) -> Mesh,
    --- Returns a sphere of `radius`, `segments` slices around (16 by default) and `rings` from top
    --- to bottom (8 by default), each from 2 or 3 to 512. Sizes and radii must be above 0.
    new_sphere: (radius: number, segments: number?, rings: number?) -> Mesh,
}

--- The keyboard, mouse and gamepads, and actions bound to keys and buttons. Works while the game
--- loads and in `update`.
declare input: {
    --- Binds `action` to keys and gamepad buttons, replacing what it was bound to before. With
    --- `gamepad`, only that gamepad's buttons count; without, any gamepad's do.
    bind: (action: string, controls: { Key | GamepadButton }, gamepad: number?) -> (),
    --- Returns what `action` is bound to, in the order it was bound, empty if it isn't, and the
    --- gamepad whose buttons count, if one was given.
    bindings: (action: string) -> ({ Key | GamepadButton }, number?),
    --- Returns the keys and gamepad buttons pressed for this update, in the order they were
    --- pressed, named as `bind` takes them: what a screen choosing controls binds. Gamepad buttons
    --- are any gamepad's.
    pressed_controls: () -> { Key | GamepadButton },
    --- Returns whether anything bound to `action` is down.
    held: (action: string) -> boolean,
    --- Returns whether anything bound to `action` was pressed for this update.
    pressed: (action: string) -> boolean,
    --- Returns whether anything bound to `action` was released for this update.
    released: (action: string) -> boolean,
    --- Returns whether a key bound to `action` repeated for this update, as `key_repeated` says.
    repeated: (action: string) -> boolean,
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
    --- Returns whether the system repeated `key`, held down, for this update, as it does after a
    --- delay and then at a steady rate. The press that started it isn't a repeat.
    key_repeated: (key: Key) -> boolean,
    --- Returns what was typed for this update, as the player's keyboard layout made it, without
    --- control characters or shortcuts' letters.
    text: () -> string,
    --- Returns what the input method is composing, or `""`.
    composition: () -> string,
    --- Takes the system's input method while `area`, around where text goes, is given, or with
    --- `nil`, lets it go. While it composes, keys reach it rather than the game.
    set_input_method: (area: Rect?) -> (),
    --- Returns the cursor's position in canvas pixels, or `nil, nil` outside the window.
    mouse_position: () -> (number?, number?),
    --- Returns whether `button` is down.
    mouse_held: (button: MouseButton) -> boolean,
    --- Returns whether `button` was pressed for this update.
    mouse_pressed: (button: MouseButton) -> boolean,
    --- Returns whether `button` was released for this update.
    mouse_released: (button: MouseButton) -> boolean,
    --- Returns how far the mouse wheel turned for this update, in notches: `y` positive turned
    --- away from the player, as to scroll up, and `x` positive tilted right. A touchpad scrolls by
    --- fractions of a notch.
    wheel: () -> (number, number),
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
    --- Plays `sound`, once or looping, and returns its voice, by which it's changed as it plays.
    play: (sound: Sound, settings: PlaySettings?) -> Voice,
    --- Stops every sound effect playing, fading them out over `fade` seconds.
    stop_sounds: (fade: number?) -> (),
    --- Pauses every sound effect playing, fading them out over `fade` seconds.
    pause_sounds: (fade: number?) -> (),
    --- Resumes every sound effect paused, fading them in over `fade` seconds.
    resume_sounds: (fade: number?) -> (),
    --- Returns how many sound effects are playing, paused ones too.
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

--- Bodies and characters among a scene's nodes, in 2D and in its 3D views.
declare physics: {
    --- Returns an empty physics world, which the first node given to it makes a world of that
    --- node's scene.
    world: (options: WorldOptions?) -> PhysicsWorld,
    --- Returns an empty 3D physics world, which the first node given to it makes a world of the
    --- 3D view that node is in.
    world_3d: (options: World3dOptions?) -> PhysicsWorld3d,
}

declare scene: {
    --- Loads the scene file at `path` and returns its root, a scene of its own.
    load: (path: string) -> SceneNode,
    --- Returns the clips of the scene file at `path`, by name.
    clips: (path: string) -> { [string]: Clip },
}

--- Menus over scenes' nodes, which games show as they like.
declare ui: {
    --- Returns a menu of `node`'s items: its children, such as a stack's, or with `options.tag`,
    --- the nodes inside it with that tag. Those shown are the items, where they're drawn, which
    --- must be in canvas pixels, without a camera, for the mouse to point at them.
    menu: (node: SceneNode, options: MenuOptions?) -> Menu,
}

--- Data kept between runs of the game, in saves named with letters, digits, `_` and `-`, but no
--- name Windows keeps for a device, such as "con". Saves hold plain data: booleans, numbers, strings, and tables of them that are lists or have strings
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

--- The game's window.
declare window: {
    --- Closes the window once the frame has drawn, which ends the game. Works while the game
    --- loads, in `update`, `gui` and `close_requested`.
    quit: () -> (),
    --- Returns a copy of what the game before this one returned from its `reloading`, if the
    --- game was reloaded, or nil. Works at any time.
    reloaded: () -> any,
    --- Returns how the window covers the screen, or nil if it's a window of its own size: as it
    --- opened, as the game last set it, or as the player changed it through the system. A window
    --- that couldn't be exclusive is borderless. Works while the game loads, in `update`, `gui`
    --- and `close_requested`.
    fullscreen: () -> Fullscreen?,
    --- Makes the window cover the screen as `mode` says, or a window of its own size again for
    --- nil, once the frame has drawn. Works while the game loads, in `update`, `gui` and
    --- `close_requested`.
    set_fullscreen: (mode: Fullscreen?) -> (),
    --- Returns how a fixed resolution fills the window. Works while the game loads, in `update`,
    --- `gui` and `close_requested`.
    scaling: () -> Scaling,
    --- Scales a fixed resolution to the window by `scaling`, from the frame that draws next. It
    --- changes nothing without one. Works while the game loads, in `update`, `gui` and
    --- `close_requested`.
    set_scaling: (scaling: Scaling) -> (),
}

--- Math functions that give the same bits on every platform, unlike `math`'s, which come from the
--- C library Luau is built with. Each takes what `math`'s function of the same name takes, and
--- costs about five times as much to call.
declare exact: {
    sin: (x: number) -> number,
    cos: (x: number) -> number,
    tan: (x: number) -> number,
    asin: (x: number) -> number,
    acos: (x: number) -> number,
    atan: (x: number) -> number,
    atan2: (y: number, x: number) -> number,
    sinh: (x: number) -> number,
    cosh: (x: number) -> number,
    tanh: (x: number) -> number,
    exp: (x: number) -> number,
    --- The natural logarithm of `x`, or with `base`, its logarithm in that base.
    log: (x: number, base: number?) -> number,
    log10: (x: number) -> number,
    pow: (x: number, y: number) -> number,
}

--- Rectangles, such as `node:bounds()` returns. Written in Luau, so they cost what a script's own
--- functions do.
declare rect: {
    --- Whether `a` and `b` overlap. Rectangles that only touch don't.
    overlaps: (a: Rect, b: Rect) -> boolean,
    --- Whether the point `x`, `y` is in `r`, counting its top and left edges but not its bottom and
    --- right, so a point on the edge between two rectangles side by side is in one of them.
    contains: (r: Rect, x: number, y: number) -> boolean,
}

--- Returns `value` moved toward `target` by `step`, 0 or more, but no further than `target`.
declare function approach(value: number, target: number, step: number): number

--- Starts `f(...)` as a task, running it until it first waits or finishes. The engine resumes it
--- every update from then on.
declare function spawn<A...>(f: (A...) -> ...any, ...: A...): ()

--- Pauses the task for `seconds` of game time, or until the next update. Works only in tasks.
declare function wait(seconds: number?): ()

--- Pauses the task until `condition()` returns true, checking once each update. Works only in
--- tasks.
declare function wait_until(condition: () -> boolean): ()

-- What only tests have: the files named `*.test.luau` that `wingine test` runs.

--- How a test runs.
type TestOptions = {
    --- Seconds of game time it may run before it fails, 60 by default.
    time_limit: number?,
}

--- How a screenshot matches its reference.
type ScreenshotOptions = {
    --- How much a channel of a pixel may differ, from 0 to 255: 2 by default.
    tolerance: number?,
    --- How many pixels may differ by more: 0 by default.
    pixels: number?,
}

--- Declares a test, which runs against a fresh copy of the game once the file has loaded. Only
--- in tests, and only as their file loads.
declare function test(name: string, run: () -> (), options: TestOptions?): ()

--- Fails the test unless `actual` and `expected` are equal, tables by their fields, saying what
--- each was. Only in tests.
declare function assert_equal(actual: any, expected: any, message: string?): ()

--- Fails the test unless `actual` is within `tolerance` (1e-6 by default) of `expected`. Only in
--- tests.
declare function assert_near(actual: number, expected: number, tolerance: number?, message: string?): ()

--- Drives the game in a test, as a player would, and runs its frames. Only in tests.
declare sim: {
    --- Runs `frames` frames, 1 by default.
    step: (frames: number?) -> (),
    --- Runs `seconds` of game time, in whole frames of a sixtieth of a second.
    run_for: (seconds: number) -> (),
    --- Returns how many frames have run.
    frame_count: () -> number,
    --- Holds `key` down until `sim.release(key)`.
    press: (key: Key) -> (),
    release: (key: Key) -> (),
    --- Presses `key`, runs a frame, and lets it go.
    tap: (key: Key) -> (),
    --- Holds `key` down for `seconds` of game time.
    hold: (key: Key, seconds: number) -> (),
    --- Types `text`, without pressing keys.
    type_text: (text: string) -> (),
    --- Holds down what the game bound `action` to: its first key, or else its first button of a
    --- connected gamepad.
    press_action: (action: string) -> (),
    release_action: (action: string) -> (),
    tap_action: (action: string) -> (),
    hold_action: (action: string, seconds: number) -> (),
    --- Moves the cursor to `x`, `y` in canvas pixels.
    move_mouse: (x: number, y: number) -> (),
    --- Holds a mouse button down, the left by default.
    press_mouse: (button: MouseButton?) -> (),
    release_mouse: (button: MouseButton?) -> (),
    --- Moves the cursor to `x`, `y`, presses a mouse button (the left by default), runs a frame,
    --- and lets it go.
    click: (x: number, y: number, button: MouseButton?) -> (),
    --- Turns the mouse wheel `x`, `y` notches where the cursor is: positive `y` away from the
    --- player, as to scroll up, and positive `x` tilted right.
    scroll: (x: number, y: number) -> (),
    --- Connects a gamepad, and returns its number.
    connect_gamepad: (name: string?) -> number,
    disconnect_gamepad: (gamepad: number) -> (),
    --- Pushes a gamepad's button `strength` of the way, from 0 to 1 (all the way by default).
    press_button: (gamepad: number, button: GamepadButton, strength: number?) -> (),
    release_button: (gamepad: number, button: GamepadButton) -> (),
    tap_button: (gamepad: number, button: GamepadButton) -> (),
    hold_button: (gamepad: number, button: GamepadButton, seconds: number) -> (),
    --- Moves a gamepad's stick to `x`, `y`, each from -1 to 1, with `y` down.
    move_stick: (gamepad: number, stick: "left" | "right", x: number, y: number) -> (),
    --- Clicks the interface's widget labelled `label`, laid out in the frame before, and runs the
    --- frames that complete the click.
    click_gui: (label: string) -> (),
    --- Returns where the interface's widget labelled `label` is, in canvas pixels, or nil.
    gui_rect: (label: string) -> Rect?,
    --- Resizes the window, in pixels.
    resize: (width: number, height: number) -> (),
    --- Presses the window's close button, and returns whether the window would close.
    request_close: () -> boolean,
    --- Returns whether the game called `window.quit`.
    has_quit: () -> boolean,
    --- Saves the latest frame as a PNG at `path`, relative to the test file.
    screenshot: (path: string) -> (),
    --- Returns whether the latest frame matches the PNG at `path`, relative to the test file, and
    --- if not, what differs: `assert(sim.matches_screenshot(path))`.
    matches_screenshot: (path: string, options: ScreenshotOptions?) -> (boolean, string?),
}
