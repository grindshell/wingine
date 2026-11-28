//! Games written entirely in Luau, run from their folder.
//!
//! A game is a folder with a `main.luau` script, run with the `wingine` executable
//! (`cargo run -- path/to/game`) or [`run`]. The script's top level runs once, when the window is
//! ready, to set the game up. Then each frame the engine calls the script's global function
//! `update(dt)`, with the seconds since the last frame, resumes the tasks started with `spawn`
//! (those `update` starts, from the next update), and calls `draw()`. With an `update_rate`
//! setting, the game updates that many times a second instead, each time by the same `dt`, and
//! each frame runs as many updates as its time holds before it draws. Both functions are
//! optional. When the player presses the window's close button, the engine calls the script's
//! global `close_requested()`, if it has one, lending it what `update` is lent: the window closes
//! unless it returns false, as a game does to ask first, and `window.quit` closes it later. The
//! built-ins `wait`, `wait_until`, `spawn` and `print` work as described in the
//! [`lua`](crate::lua) module, which also lists how engine types such as colors and keys are
//! written in Luau.
//!
//! The engine's API is a module, which a script requires: `local wg = require("@wingine")`, then
//! `wg.graphics.color`, `wg.input.held` and so on. It isn't a file: the engine answers
//! `@wingine` before looking at any `.luaurc`, so a game can alias the name to a file of types for
//! its language server, which the engine never follows. Its only globals are Luau's `print` and
//! `require`, the functions it defines for the engine to call, its native libraries' tables, and
//! whatever it makes global itself.
//!
//! The game's settings come from an optional `conf.luau` that returns a table, which the engine
//! reads before the game starts. Every setting is optional:
//!
//! ```lua
//! return {
//!     title = "Pong", -- by default, the folder's name
//!     -- A square PNG the window shows in its title bar and taskbar button, and on Windows, the
//!     -- exported game's executable. Pixel art stays crisp. By default, the system's.
//!     icon = "icon.png",
//!     width = 1280, -- the window's initial size in logical pixels
//!     height = 720,
//!     resizable = true,
//!     -- Whether the window opens covering the screen: "borderless", or "exclusive", the screen
//!     -- given over to the game at a video mode of its own. By default, it doesn't.
//!     fullscreen = "borderless",
//!     -- A fixed canvas size. By default, the canvas is the window.
//!     resolution = { width = 320, height = 180 },
//!     -- How a fixed resolution fills the window: "whole", scaled up by the largest whole number
//!     -- that fits, with black bars, the default; "fit", as large as fits keeping its shape; or
//!     -- "stretch", filling it both ways.
//!     scaling = "fit",
//!     -- Whether frames wait for the display's refresh (true by default). Turning it off shows
//!     -- how long frames really take.
//!     vsync = false,
//!     -- Updates a second, each by the same `dt`, so the game plays the same at any frame rate.
//!     -- By default, the game updates once a frame by the frame's time.
//!     update_rate = 60,
//!     -- A key that shows and hides a window of how long frames take. The game never sees it.
//!     debug_key = "F3",
//!     -- Names the folder the game's saves are kept in: letters, digits, `_` and `-`, but no
//!     -- name Windows keeps for a device, such as "con". By default, the folder's name, with `_`
//!     -- for anything else.
//!     identity = "pong",
//!     -- Native libraries to load, in order, from the game's `native` folder. One marked
//!     -- optional loads only if it's there: `if danmaku then ... end` tells whether it did.
//!     libraries = { "particles", { name = "danmaku", optional = true } },
//! }
//! ```
//!
//! Native libraries are compiled code written in Rust with the `wingine-native` crate. Each is
//! loaded from the game's `native` folder, named as the platform names libraries:
//! `particles.dll` on Windows, `libparticles.so` on Linux and `libparticles.dylib` on macOS. Once
//! loaded, a library stays loaded until the program ends. It adds a global table of its own,
//! named by the library, before `main.luau` runs. Unlike scripts, a library isn't sandboxed: a
//! game that lists one runs it with the player's full permissions.
//!
//! Scripts `require` other scripts by their path relative to the script, such as
//! `require("./opponent")`. Files the engine loads, such as sounds and images, are relative to
//! the game's folder.
//!
//! A run can hand the game words, after `--` in its command: `wingine path/to/game -- urban 7`,
//! `wingine --watch path/to/game -- urban 7`, `wingine test path/to/game -- timing=2`, and an
//! exported game's `tiny -- urban 7`. `arguments()` returns them, in order, as a new table each
//! call, the same in every test of a run and in a game reloaded in place, and empty when there are
//! none. The engine makes nothing of them; `name=value` is a game's to match. They're in
//! [`Config::arguments`].
//!
//! The module holds the built-ins `wait`, `wait_until`, `spawn`, `exact`, `rect`, `approach` and
//! `arguments`, and eleven tables:
//!
//! `graphics`, for drawing in `draw` or in a `paint:scene` callback. `graphics.size`, `graphics.color`, `graphics.default_font`,
//! `graphics.frame_stats`, `graphics.new_animation`, `graphics.new_particles` and
//! `graphics.hit_box` work anywhere,
//! `graphics.load_sprite_sheet`, `graphics.load_model` and the functions that make meshes while
//! the game loads and in `update`, and `graphics.project` and `graphics.ray` then and in `draw`.
//!
//! - `graphics.clear(color)` fills the canvas with `color`, discarding anything drawn so far.
//! - `graphics.fill_rect(x, y, width, height, color)`
//! - `graphics.outline_rect(x, y, width, height, color, thickness)` draws the rectangle's
//!   outline, `thickness` wide (1 by default) inside its edges, so it covers the edge of what
//!   `fill_rect` would fill. Its sides don't overlap, so a translucent outline is even.
//! - `graphics.draw_line(x1, y1, x2, y2, color, thickness)` draws a line `thickness` wide (1 by
//!   default) from `x1`, `y1` to `x2`, `y2`, centered on them and ending square at them. A
//!   line covers the pixels whose middles it covers, so a line 1 wide lights a row of pixels
//!   when it runs along their middles, such as from 0, 5.5 to 11, 5.5.
//! - `graphics.fill_circle(x, y, radius, color)` fills a circle of `radius` about `x`, `y`.
//! - `graphics.outline_circle(x, y, radius, color, thickness)` draws the circle's outline,
//!   `thickness` wide (1 by default) inside its edge.
//! - `graphics.draw_sprite(sprite, x, y, tint, width, height)` draws `sprite` with its top-left
//!   corner at `x`, `y`, its colors multiplied by `tint` (white, unchanged, by default), and
//!   stretched to `width` by `height` (its own size by default).
//! - `graphics.draw_sprite_rotated(sprite, x, y, angle, tint, width, height)` draws `sprite` as
//!   `draw_sprite` does, then rotated by `angle` radians about its center. Positive angles turn
//!   clockwise, since `y` grows downward.
//! - `graphics.draw_sprite_sliced(sprite, x, y, width, height, slice, tint)` draws `sprite` sliced
//!   to fill `width` by `height` with its top-left corner at `x`, `y`, as a frame or a panel is
//!   drawn at any size: `slice` is how many texels along its edges keep their size, a number for
//!   them all or `{ left, top, right, bottom }`, so its corners are drawn as they are, its edges
//!   stretch along their length and its middle both ways. Edges too big for the space shrink
//!   alike to meet. `tint` is white by default.
//! - `graphics.draw_text(font, text, x, y, color)` draws `text` with its top-left corner at `x`,
//!   `y`, rounded to whole pixels.
//! - `graphics.draw_text_layout(layout, x, y, color, glyphs)` draws text laid out with
//!   `font:layout`, as `draw_text` does. With `glyphs`, it draws only that many of the layout's
//!   glyphs, rounded down, so drawing a growing number reveals the text a little at a time.
//! - `graphics.draw_tilemap(map, layer)` draws one layer of `map`, or all of them in order
//!   without a layer. Only the cells the canvas shows are drawn.
//! - `graphics.draw_scene(node)` draws a node of a scene and every node inside it, each before
//!   its children, where they are in the scene, first laying the scene out on the canvas, so its
//!   anchors follow the canvas's edges. Hidden nodes don't draw, nor what's inside them, and
//!   nothing is drawn where it's off the canvas.
//! - `graphics.set_camera(x, y, bounds, zoom)` centers the view on world position `x`, `y` for
//!   whatever is drawn next, keeping it inside `bounds`, a rect such as `map:bounds()`, if given,
//!   and scaling it by `zoom`, canvas pixels a world unit: above 1 zooms in and below 1 out, about
//!   the view's center. It's 1 by default, and must be above 0. `graphics.set_camera(nil)`, the
//!   default at the start of each frame, draws in canvas pixels, as for a HUD.
//! - `graphics.set_blend(blend)` mixes whatever is drawn next with what's under it as `blend`
//!   says: `"normal"`, the default at the start of each frame, `"add"`, `"multiply"` or
//!   `"screen"`. A scene's nodes draw in their own modes, and leave it as it was.
//! - `graphics.new_particles(settings, sprite)` returns an emitter of particles, each showing
//!   `sprite` or a white square, with `settings` as a scene file's particles node has them (see
//!   `wingine::scene`), and `graphics.draw_particles(particles)` draws its particles, in its
//!   blend mode. Emitters keep no time: `particles:advance(dt)` moves them on. Each one a game
//!   makes is seeded by how many it made before, unless its settings give a seed, so a game plays
//!   the same every run.
//! - `graphics.size()` returns the canvas's width and height.
//! - `graphics.default_font()` returns the built-in pixel font.
//! - `graphics.load_font(path, settings)` loads a font. Without `settings`, `path` is a font file,
//!   `*.font.luau` by convention: a bitmap font, its glyphs drawn in a PNG, or a TrueType or
//!   OpenType font at a size (see `wingine::scene` for the format). With them, it's a `.ttf` or
//!   `.otf` file itself, and `settings` is a table of `size`, how many pixels its em is, and
//!   `smooth`, true by default, or false for each pixel to be opaque or clear, so a pixel font
//!   stays crisp. Each file is read once: loading one again, even at another size, reuses it.
//! - `graphics.color(color, opacity)` returns `color`, such as a hex string or a table, as a
//!   number `0xRRGGBBAA`, which passes faster: a string or table is read again each time it's
//!   passed, which costs more than drawing a sprite, so code that draws a lot makes its colors
//!   numbers once. With `opacity`, its alpha is multiplied by it (and kept from 0 to 1), so a
//!   color made a number once can fade a little each frame.
//! - `graphics.frame_stats()` returns how long the frame before took, and how much it drew, as a
//!   table: `frame`, `update`, `draw` and `render` are seconds, and `quads`, `triangles`,
//!   `draw_calls`, `out_of_sight`, `lights`, `lights_left_out`, `shadow_maps` and
//!   `shadows_left_out` counts.
//!   Before the first frame has finished, all are 0. See `FrameStats` for what each measures.
//!   `libraries` is what each native library cost the frame, by its table's name: its `update`
//!   and its `draw`, each a table of `hooks`, the seconds its hooks took, `calls`, how many calls
//!   scripts made into it, and `call_time`, the seconds those took, which is there only while
//!   calls are timed.
//! - `graphics.time_library_calls(on)` times the calls scripts make into native libraries, for
//!   `frame_stats`, from the next frame on, or stops. Timing a call costs about a tenth of the
//!   call, so calls aren't timed until a game asks.
//! - `graphics.load_sprite_sheet(path, tile_width, tile_height)` loads a PNG divided into a grid
//!   of sprites `tile_width` by `tile_height` texels, starting at its top-left corner. Each image
//!   is read once; loading it again, even divided differently, reuses it.
//! - `graphics.new_animation(frames, frame_time)` returns an animation that shows each sprite in
//!   the list `frames` for `frame_time` seconds, then starts over. Draw it with
//!   `graphics.draw_sprite(animation:frame(time), x, y)`, where `time` is how long it has been
//!   playing.
//! - `graphics.begin_3d(view)` begins a 3D view over what's been drawn. The meshes and billboards
//!   drawn after it go in it, nearer ones hiding farther ones, until anything else is drawn, which
//!   goes over it. `view` is a table of `camera`, a table of `position` and `target` (vectors),
//!   and optionally `up`, `fov` (with perspective, 0.8 radians by default) or `height` (without),
//!   `near` and `far`; `rect`, where on the canvas it shows (the whole canvas by default, moved by
//!   the camera `set_camera` sets); and `lighting`, a table of any of `direction` (a vector),
//!   `color` and `ambient`, a sun's light and light from all around, `shadows`, a table of a
//!   `reach` and optionally a `darkness` (1), with which the view's meshes and models in the
//!   sunlight's way shade what's behind them, out to `reach` world units from the point the
//!   camera looks at, a shadow taking `darkness` of the sun's light, from 0 to 1, in a shadow
//!   map of the sun's own, 2048 texels across the reach each way; and `lights`, a list of
//!   lights at places: each a table of a `position` (a vector), and optionally a `color`
//!   (white), a `range` in world units (5), across which its light falls off in a straight line
//!   to nothing, and an `energy` (1), times its color; and for a spot light, a `cone`, half its
//!   angle in radians, the `direction` it shines (a vector), and the `edge` of the cone it fades
//!   across, a part of it from 0 to 1 (0). A view is lit by 32 lights at once: of those reaching
//!   what its camera sees, the nearest to the camera. All a view's light together lights a
//!   surface no brighter than white. With `shadows` (`false`), the view's meshes and models in
//!   a light's way shade what's behind them, with hard edges; billboards shade nothing. A view
//!   has 24 shadow maps, which the lights nearest its camera take first: a spot light whose
//!   `cone` is no more than a sixth of a turn takes one, and any other light six. A light there
//!   are none left for lights without shadows. With `bounds`, six numbers `{ x0, y0, z0, x1, y1,
//!   z1 }`, a box's least corner and then its most, a light lights only within that box, faces
//!   and all, allowing for floating-point roundoff at a face. So one without shadows can be
//!   kept from lighting through walls and floors. A light whose box is wholly out of what the
//!   camera sees doesn't count toward the 32.
//! - `graphics.draw_mesh(mesh, position, options)` draws a mesh in the view begun last, its origin
//!   at `position`, a vector, scaled by `options.scale` (a number, or a vector for each axis),
//!   turned by `roll` about `z`, `pitch` about `x` and `yaw` about `y`, in that order, showing
//!   `sprite` multiplied by `tint`. It casts shadows from lights with `shadows` unless
//!   `options.casts_shadow` is `false`. With `options.blend`, a blend mode's name, it blends
//!   with what's behind it, as see-through as its colors say: drawn after the view's opaque
//!   meshes, the farthest of those blended normally first, by where its origin is, showing both
//!   its sides and casting no shadow. Drawing one with no view open is an error. One wholly out
//!   of sight is left out, as `Canvas3d` says, which shows the same.
//! - `graphics.draw_model(model, position, options)` draws each part of a model as `draw_mesh`
//!   would, placed in the model, and the model placed by `position` and `options` as a mesh is,
//!   all multiplied by `options.tint`, casting shadows or not by `options.casts_shadow`, and
//!   blended by `options.blend`, each part ordered by where it is. Its parts show their own
//!   sprites, so `options.sprite` is an error. A part out of sight is left out.
//! - `graphics.draw_billboard(sprite, position, tint, width, height)` draws a sprite standing
//!   upright in the view begun last, the middle of its bottom edge at `position`, turned to face
//!   the camera across the ground: `height` tall (1 by default) and `width` wide (by default, as
//!   its shape makes it).
//! - `graphics.draw_text_3d(font, text, position, { height, align, color, glow })` draws text in
//!   the view begun last as a sign, standing upright and facing the camera as a billboard does,
//!   its lines `height` apart (1 by default), with the middle of its bottom edge at `position`,
//!   or its left or right end as `align` says. Each glyph is a billboard of the font's texture.
//! - `graphics.project(view, point)` returns where `point` shows on the canvas through `view`, or
//!   nothing if it's nearer than the near plane or beyond the far one. `graphics.ray(view, x, y)`
//!   returns the ray through `x`, `y` into the view: its origin, on the near plane, and its
//!   direction, of length 1.
//! - `graphics.hit_box(origin, direction, min, max)` returns how far along the ray from `origin`
//!   going `direction` it enters the box between the corners `min` and `max`, in lengths of
//!   `direction` (so world units, for a ray from `graphics.ray`), 0 if it starts inside, or
//!   nothing if it misses. With `mesh:bounds()` or `model:bounds()`, moved to where it's drawn,
//!   it tells what's under the mouse.
//! - `graphics.new_cube(size)`, `graphics.new_cuboid(width, height, depth)`,
//!   `graphics.new_plane(width, depth)` and `graphics.new_sphere(radius, segments, rings)` make
//!   meshes, their sizes above 0 and spheres' slices and rings at most 512, and
//!   `graphics.new_mesh(data)` one of `data.vertices`, each a table of `position`
//!   and optionally `normal`, `u`, `v` and `color`, and `data.indices`, three vertices for each
//!   triangle, counting from 1.
//! - `graphics.load_model(path)` loads a model file, glTF 2.0 as `.glb` or `.gltf` (with the
//!   files it names beside it), and returns a `Model` of its default scene's meshes, with their
//!   vertex colors, materials' base colors and PNG textures. Each file is read once; loading it
//!   again returns the same model. See `Context::load_model` for what's read. An engine built
//!   without its `models` feature fails to load any.
//!
//! `tilemap`, for building maps of tiles, anywhere:
//!
//! - `tilemap.new_tileset(tile_width, tile_height)` returns an empty tileset of tiles
//!   `tile_width` by `tile_height` world units, which are canvas pixels without a camera.
//! - `tilemap.new(tileset, columns, rows)` returns a map `columns` by `rows` tiles in size,
//!   with no layers yet, built from a copy of `tileset`. Maps hold at most 16,777,216 cells
//!   across all their layers.
//!
//! `grid`, for walking from tile to tile, anywhere:
//!
//! - `grid.new_mover(column, row, step_time)` returns a mover standing on a tile and facing
//!   down, which takes `step_time` seconds to walk to the next. Each frame, call
//!   `mover:update(dt)`, then `mover:step(direction, blocked)` to start the next step once it
//!   isn't moving. A step that starts as another ends picks up the time left over, so walking
//!   keeps an even pace.
//! - `grid.neighbor(direction, column, row)` returns the column and row of the next tile in
//!   `direction`, failing past the edge of the tiles an `i32` numbers.
//! - `grid.opposite(direction)`
//!
//! `physics`, for bodies and characters among a scene's nodes, in 2D and in 3D views, anywhere:
//!
//! - `physics.world(options)` returns an empty `PhysicsWorld`, with `options.metre` of the scene's
//!   units to a metre (16 by default), whose dynamic bodies fall down the scene with
//!   `options.gravity` (9.81 metres a second each second by default), and which the first node
//!   given to it makes a world of that node's scene: its colliders cover nodes as they're drawn,
//!   or tile maps' solid tiles, following them as they move and going when they do; its bodies
//!   fall, roll and are carried, moving their nodes as `world:step` moves on through time;
//!   `world:move` slides nodes among them and `world:walk` walks characters; and rays, overlaps
//!   and sensors ask it where things are.
//! - `physics.world_3d(options)` returns an empty `PhysicsWorld3d`, whose dynamic bodies fall with
//!   `options.gravity`, a vector (9.81 down by default), and which the first node given to it
//!   makes a world of that node's 3D view: bodies shaped like what nodes show, which `world:step`
//!   moves on through time, characters `world:walk` walks among them, and sensors; rays and
//!   overlaps ask it where things are. It follows its nodes by which of them were placed anew,
//!   so what moves where no body is costs a large world nothing.
//!
//! See [`physics`](crate::physics) for how. An engine built without its `physics` feature fails to
//! make either.
//!
//! `scene`, for loading [scenes](crate::scene):
//!
//! - `scene.load(path)`, while the game loads and in `update`, loads the scene file at `path`,
//!   the scene files its instances place, the tileset files its tilemaps use and the images they
//!   all name, and returns the scene's root node, laid out on the canvas. Each file is read once,
//!   and each load returns a copy of its own.
//! - `scene.clips(path)`, at any time, returns the clips of the scene file at `path`, a table of
//!   `Clip`s by name, for posing nodes with `node:pose(clip, time, weight)`. A clip poses any node
//!   whose nodes its tracks' paths lead to, such as each instance of the file.
//!
//! `ui`, for menus over scenes' nodes, at any time:
//!
//! - `ui.menu(node, options)` returns a `Menu` of `node`'s items: its children, such as a
//!   stack's, or the nodes inside it tagged `options.tag`, those shown, where they're drawn (see
//!   [`Scene::extent`](crate::scene::Scene::extent)), numbered from 1 in order. `options` may
//!   also turn off `wrap` and `mouse`, and name the actions it answers to, `up`, `down`, `left`,
//!   `right`, `confirm` and `cancel` by default. Items are found again each time it's used, and
//!   the selection stays on its item while it's there. See [`menu`](crate::menu) for how it
//!   moves.
//! - `menu:update()`, while the game loads and in `update`, takes the player's input and returns
//!   what they did, if anything, and to which item: `"moved"` the selection to it, `"picked"` it,
//!   or `"cancelled"` the menu, with no item.
//! - `menu:selected()` returns the selected item's number, `menu:select(item)` selects one,
//!   `menu:items()` returns the items, and `menu:set_enabled(item, enabled)` and
//!   `menu:is_enabled(item)` set and tell whether one can be picked.
//! - `ui.new_menu(options)` returns a menu without a scene, for an interface a script draws
//!   itself: `menu:update(items, clip)` takes the items as they are, each a table of `x`, `y`,
//!   `width`, `height` and optionally `enabled`, and returns what `menu:update()` does. With
//!   `clip`, a rect, the mouse counts only inside it, as a list that scrolls wants, whose items
//!   scrolled out are past its edges. `menu:selected()` and `menu:select(item)` are its
//!   selection, which `select(nil)` clears.
//!
//! `input`, while the game loads and in `update`:
//!
//! - `input.bind(action, controls, gamepad)` binds an action, such as `"jump"`, to a list of
//!   keys, mouse buttons and gamepad buttons, such as `{ "Space", "W", "South" }`, replacing any
//!   it had. A gamepad's buttons include its sticks' directions, such as `"LeftStickLeft"`, and
//!   a mouse button is named with `Mouse` before it, as `"MouseLeft"`, since `"Left"` is the
//!   arrow key. With `gamepad`, a number from 1, only that gamepad's buttons count; without, any
//!   gamepad's do.
//! - `input.bindings(action)` returns what an action is bound to, a list of names in the order
//!   they were bound, empty if it isn't, and the gamepad it was bound to, if one, so a game shows
//!   the player its first: "Space: Examine".
//! - `input.pressed_controls()` returns the keys, mouse buttons and gamepad buttons pressed this
//!   frame, in the order they were pressed, named as `input.bind` takes them, as a screen that
//!   lets players choose their controls asks which they pressed. Gamepad buttons are any
//!   gamepad's.
//! - `input.held(action)`, `input.pressed(action)` and `input.released(action)` tell whether any
//!   of the action's controls is held, or was pressed or released this frame.
//! - `input.strength(action)` tells how far it's pushed, from 0 to 1: 1 for a key or a mouse
//!   button, and for a trigger or a stick's direction, how far that is. `input.axis(negative, positive)` is how far
//!   `positive` is pushed less how far `negative` is, from -1 to 1.
//! - `input.key_held(key)`, `input.key_pressed(key)` and `input.key_released(key)` do the same for
//!   a key.
//! - `input.key_repeated(key)` tells whether the system repeated a key held down this frame, as
//!   it does after a delay and then at a steady rate, by the player's settings. The press that
//!   started it isn't a repeat, so `input.key_pressed(key) or input.key_repeated(key)` acts once
//!   as the key goes down, then again each repeat. `input.repeated(action)` does the same for an
//!   action's keys; gamepad buttons don't repeat.
//! - `input.text()` returns what was typed for this update, as the player's keyboard layout, dead
//!   keys and any input method made it, such as `">"`, without control characters or what's
//!   typed with Control or Command for shortcuts. Text a focused interface field takes isn't the
//!   game's.
//! - `input.set_input_method(area)` takes the system's input method, by which players of Chinese,
//!   Japanese and Korean compose characters, while `area`, a rect in canvas pixels around where
//!   the text goes, is given, and `input.set_input_method(nil)` lets it go. While it composes,
//!   keys reach it, not the game, and `input.composition()` returns what it's composing, for the
//!   game to show; it's `""` otherwise.
//! - `input.mouse_position()` returns the cursor's position in canvas pixels, or `nil` when the
//!   cursor is outside the window, or the mouse is captured.
//! - `input.set_mouse_captured(captured)` captures the mouse, as a game that looks around with it
//!   does, or releases it. Captured, its cursor is hidden and held in the window, and its buttons
//!   and wheel are the game's rather than the interface's. The window captures it once the frame
//!   has drawn, and releases it when it loses focus, after which the game captures it again when
//!   it wants, as on a click. `input.mouse_captured()` tells whether it's captured.
//! - `input.mouse_motion()` returns how far the mouse moved this frame while it's captured, as
//!   `x, y`: `x` is positive moved right, and `y` positive moved toward the player. It's in the
//!   mouse's own units rather than canvas pixels, for a game to multiply by a sensitivity, and
//!   `0, 0` while the mouse isn't captured.
//! - `input.mouse_held(button)`, `input.mouse_pressed(button)` and
//!   `input.mouse_released(button)` do the same for a mouse button.
//! - `input.wheel()` returns how far the mouse wheel turned this frame, in notches, as `x, y`:
//!   `y` is positive turned away from the player, as to scroll up, and `x` positive tilted
//!   right. A touchpad scrolls by fractions of a notch. Turns over the interface are its own.
//! - `input.gamepads()` returns the connected gamepads' numbers, which count from 1 in the order
//!   gamepads first connect, and `input.gamepad_name(gamepad)` a gamepad's name.
//! - `input.button_held(button, gamepad)`, `input.button_pressed(button, gamepad)`,
//!   `input.button_released(button, gamepad)` and `input.button_strength(button, gamepad)` do
//!   the same for a gamepad's button, or without `gamepad`, any gamepad's.
//!
//! `audio`, while the game loads and in `update`. `audio.load` and `audio.make` work anywhere.
//!
//! - `audio.load(path)` loads a sound effect from a WAV or OGG Vorbis file.
//! - `audio.make(samples, rate, channels)` makes a sound effect of the samples in a `buffer`,
//!   each a 32-bit float from -1 to 1 as `buffer.writef32` writes one, `rate` of them a second
//!   for each of its `channels`: 1, by default, or 2, a sample for the left then one for the
//!   right. No samples, one that isn't a number, a buffer that isn't whole samples for each
//!   channel, a rate that isn't a whole number from 1 to 192,000, or other channels are errors.
//! - `audio.play(sound, settings)` plays a sound, with optional settings, and returns its `Voice`,
//!   by which the game changes it as it plays. A sound plays once, or with `looping`, until its
//!   voice stops it.
//! - `audio.stop_sounds(fade)` fades out every sound effect over `fade` seconds (0 by default),
//!   and `audio.pause_sounds(fade)` and `audio.resume_sounds(fade)` pause and resume them, as
//!   the game pauses.
//! - `audio.sounds_playing()` returns how many sound effects are playing, paused ones too.
//! - A voice's methods work where `audio`'s do. `voice:stop(fade)`, `voice:pause(fade)` and
//!   `voice:resume(fade)` act over `fade` seconds (0 by default); a stopped voice can't start
//!   again. `voice:set_volume(volume, fade)`, `voice:set_speed(speed, fade)` and
//!   `voice:set_pan(pan, fade)` change the settings of those names, checked as `audio.play`'s
//!   are. `voice:is_playing()` is whether it hasn't finished or been stopped (a paused voice is
//!   still playing), and `voice:is_paused()` whether it's paused. Once its sound is over, changing
//!   a voice does nothing.
//! - `audio.play_music(path, fade)` streams music from a file and loops it, crossfading from any
//!   music already playing over `fade` seconds.
//! - `audio.stop_music(fade)` and `audio.music_playing()`
//! - `audio.load_song(path)` loads a [song](crate::music) file, which works anywhere. A song's
//!   methods tell what's in it: `song:sections()`, `song:tracks()`, `song:start()`,
//!   `song:tempo()`, `song:meter()` (beats a bar, and the note that's a beat),
//!   `song:bars(section)` and `song:markers(section)`, the last two nil for no such section.
//! - `audio.play_song(song, settings)` plays a song live on the music track and returns its
//!   `Performance`. Its settings are a table of `volume` (1), `fade_in` (0 seconds), `section`
//!   to start with (the song's start) and `beat` into it to start at (0). A section it doesn't
//!   have, or a setting that isn't a number, is an error.
//! - `audio.songs_playing()` and `audio.stop_songs(fade)`, as for sound effects.
//! - A performance's methods work where `audio`'s do. Times a change happens `at` are `"now"`,
//!   `"beat"` and `"bar"`, the start of the next one, and `"section"`, when the section playing
//!   ends, in place of what would follow it. Names the song doesn't have are errors.
//!   - `performance:go_to(section, at)` moves to a section (at once by default);
//!     `performance:set_tempo(tempo, { at, over })` changes the tempo, in beats a minute kept
//!     from 1 to 999, sliding over `over` beats (0); `performance:set_track_volume(track,
//!     volume, fade)` fades a track in or out; `performance:set_volume(volume, fade)`,
//!     `performance:pause(fade)`, `performance:resume(fade)` and `performance:stop(fade)` are as
//!     a voice's. A stopped song is over at once, and an ended one the update after it ends.
//!   - `performance:is_playing()`, `performance:is_paused()`, and `performance:has_ended()`,
//!     whether it ended in the last update, which is the last it plays.
//!   - `performance:position()` returns the section playing, the bar of it from 1, and how far
//!     into the bar from 1, as 2.5 is halfway through its second beat; `performance:tempo()` the
//!     tempo. Both are nil once it's over.
//!   - `performance:events()` returns what happened as the last update played, in order: tables
//!     with a `kind`, `"section"` (`section`), `"beat"` (`bar`, `beat`), `"marker"` (`section`,
//!     `name`), `"note"` (`track`, `pitch`, `velocity`), or `"end"`. Notes are among them only for
//!     tracks `performance:listen(track, listen)` has it listen to.
//!   - `performance:on(event, listener)` calls `listener` with each event named `event` from the
//!     next update on, each call a task of its own, and returns a listener for
//!     `performance:off(listener)` to stop. Events are named `"beat"`, `"bar"` (a beat that
//!     starts a bar), `"section"`, `"marker"`, `"end"`, `"note:<track>"`, which listens to the
//!     track, or a section or marker by name, as `"section:chorus"` or `"marker:drop"`.
//!     `performance:wait_for(event)` waits in a task for the next such event and returns it, or
//!     nil if the song is over first. Both are Luau, over `events`.
//!   - `performance:song()` returns the song.
//! - `audio.set_volume(volume)`, `audio.set_sound_volume(volume)` and
//!   `audio.set_music_volume(volume)` set the volume of everything, of sound effects and of
//!   music, where 1 is as recorded. A volume that isn't a number is an error.
//!
//! `save`, for data kept between runs of the game, at any time. Each save has a name of letters,
//! digits, `_` and `-`, but no name Windows keeps for a device, such as `con`, and holds plain data: a boolean, number, string, or table of them, where a
//! table is a list or has strings for keys, as scenes' props are. See [`save`](crate::save) for
//! where saves are kept.
//!
//! - `save.write(name, data)` writes `data` to the save `name`, replacing any. Anything else in
//!   it, such as a function, is an error naming where it is.
//! - `save.read(name)` returns the data in the save `name`, or `nil` if there's no such save.
//! - `save.remove(name)` removes the save `name`, if there is one.
//! - `save.list()` returns the names of the game's saves, in order.
//!
//! `files`, for folders the player picks and the files in them, as an application's documents.
//! A script reaches the player's files only through a `Folder` the player picked, by paths
//! relative to it, which can't leave it: see [`folders`](crate::folders) for what a path may be.
//!
//! - `files.pick_folder()`, only in a task, since it waits, opens the system's dialog for picking
//!   a folder once the frame is done, waits until the player has picked one, and returns it, or
//!   nil if they picked none. A dialog asked for while one is open is an error. The engine
//!   remembers the folder.
//! - `files.folders()`, at any time, returns the folders picked and not forgotten, most recently
//!   first, which the engine remembers between runs, and `files.forget(folder)` leaves one out.
//! - A folder's methods work at any time. `folder:name()` returns its name, never its path.
//!   `folder:list(path)` returns the files and folders in the folder at `path`, or in this one
//!   without it, in order of their paths, each `{ path, kind, size, modified }`, `kind` being
//!   `"file"` or `"folder"` and `modified` seconds since 1970 began, or nil if there's no such
//!   folder. `folder:read(path)` returns what a file holds, or nil if there's no such file.
//!   `folder:write(path, contents)` writes a file in full before it replaces any, making the
//!   folders it's in. `folder:rename(from, to)` moves a file or folder, never over anything, and
//!   `folder:remove(path)` removes a file, if there is one, but never a folder. Two folders are
//!   equal when they're the same folder.
//!
//! A [`Simulation`](crate::headless::Simulation) opens no dialog: it answers the game's with
//! [`Simulation::pick`](crate::headless::Simulation::pick), handing it a copy of a folder, and
//! remembers the folders picked only while it runs. On the web, there are no folders to pick yet.
//!
//! `window`, for the game's window:
//!
//! - `window.quit()`, while the game loads, in `update`, `gui` and `close_requested`, closes the
//!   window once the frame has drawn, which ends the game. A game that saves on the way out writes
//!   its save first.
//! - `window.reloaded()`, at any time, returns a copy of what the game before this one returned
//!   from `reloading`, if it was reloaded (see below), or nil.
//! - `window.fullscreen()` returns whether the window covers the screen: `"borderless"`, a
//!   window without borders covering it, `"exclusive"`, the screen given over to the game at a
//!   video mode of its own, or nil, a window of its own size. It's as the window opened, as the
//!   game last set it, or as the player changed it through the system, such as with a browser's
//!   Escape. A window that couldn't be exclusive, as on Wayland and the web, is borderless.
//! - `window.set_fullscreen(mode)` makes the window cover the screen as `mode` says, or a window
//!   of its own size again for nil, once the frame has drawn. A game binds a key to it itself.
//! - `window.scaling()` returns how a fixed resolution fills the window: `"whole"`, by the
//!   largest whole number that fits, with black bars, `"fit"`, as large as fits keeping its shape,
//!   or `"stretch"`, filling it both ways.
//! - `window.set_scaling(scaling)` scales a fixed resolution by `scaling` from the frame that
//!   draws next. It changes nothing without one.
//!
//! These four work while the game loads, in `update`, `gui` and `close_requested`. A
//! [`Simulation`](crate::headless::Simulation) notes the window's fullscreen without changing its
//! frames' size, and scales them.
//!
//! A game may also define a global `gui(ui)`, which the engine calls once a frame, after the
//! updates and before `draw`, with a `Ui` covering the window, to lay out an interface over the
//! game with egui: debug tools, say. The context is lent as in `update`. Widgets that change a
//! value take it and return the new one, and containers call a function with the `Ui` inside them:
//!
//! - Containers: `ui:window(title or { title, x, y, width, height, resizable, collapsible }, fn)`,
//!   `ui:panel(side or { side, size, resizable }, fn)`, `ui:horizontal(fn)`, `ui:vertical(fn)`,
//!   `ui:group(fn)`, `ui:collapsing(title, fn)`, `ui:scroll(fn)` or
//!   `ui:scroll({ stick_to_bottom }, fn)`, `ui:grid(name or { name, columns }, fn)` with
//!   `ui:end_row()`, whose last column has the rest of each row once it knows its `columns`,
//!   `ui:enabled(enabled, fn)`, and `ui:modal(name, fn)`, a box over everything that must be
//!   answered, which returns whether the player asked to be rid of it.
//! - Text: `ui:label(text, color)`, `ui:heading(text)`, `ui:monospace(text)` and `ui:small(text)`.
//! - Widgets: `ui:button(text, { width, height })`, at least that size with its text wrapped to
//!   its width, and `ui:sprite_button(sprite, text, selected, tint, width, height)`, a sprite in a
//!   frame, named `text` and framed as chosen when `selected`, return whether they were clicked; `ui:checkbox(text, checked)`,
//!   `ui:radio(current, value, text)`, `ui:combo(text, current, choices)`,
//!   `ui:slider(text, value, min, max, step)`, `ui:drag(text, value, speed, min, max)` and
//!   `ui:text_input(text, hint or { id, label, hint })` (which also returns whether Enter was
//!   pressed) return their values, and
//!   `ui:text_edit(text, { id, label, hint, rows, monospace, fill, tab })`, a field of many lines
//!   as wide as there's room for, its text, whether typing changed it, and where its cursor is
//!   (`{ line, column, offset, anchor }`); with `tab`, Tab stays in it, typing a tab (`true`) or
//!   indenting by that many spaces, every line selected a level, and Shift+Tab unindents;
//!   `ui:progress(fraction, text or { text, hover, label, width })`, whose `hover` shows under
//!   the bar while the pointer is over it, and `ui:sprite(sprite, tint, width, height)` show them.
//! - `ui:canvas(label or { label, width, height }, fn)`, a canvas as big as given or as all the
//!   room its container has left, calls `fn(paint, input)`: `paint` paints it, in points from its
//!   top-left corner, clipped to it, with `paint:size()`, `paint:line(x1, y1, x2, y2, color,
//!   thickness)`, `paint:path(points, color, thickness)`, `paint:fill_polygon(points, color)`,
//!   `paint:fill_rect(x, y, width, height, color, rounding)`, `paint:outline_rect(x, y, width,
//!   height, color, thickness, rounding)`, `paint:fill_circle(x, y, radius, color)`,
//!   `paint:outline_circle(x, y, radius, color, thickness)`, and `paint:text(text, x, y, color,
//!   { size, monospace, align })` and `paint:measure(text, options)`, which return its size; and
//!   `input` is what the pointer did to it this frame (`{ hovered, x, y, clicked, dragged_x,
//!   dragged_y, scrolled_x, scrolled_y, zoom }`). `paint:bounds()` returns where the canvas is
//!   on the game's canvas, in canvas pixels, so `draw` can draw behind it. For a view with its
//!   own resolution, `paint:scene(width, height, draw)` paints what `draw` draws with `graphics`.
//! - `ui:separator()` and `ui:space(amount)`, and `ui:available_size()`, the room left across and
//!   down, in points.
//! - `ui:scale(factor)` returns the interface's zoom, having set it to `factor` from the next
//!   pass if it's given. Players change it too, with egui's shortcuts: Ctrl and plus, minus or 0.
//!   `ui:set_theme(palette)` colors the interface, and `ui:set_fonts({ body, heading,
//!   monospace })` sets its text in fonts of the game's, each the path of a TrueType or OpenType
//!   file in its folder or `{ file, size }`, a size in points, the rest in egui's own fonts and
//!   sizes. Headings, and windows' titles, are in the heading's font, else the body's. Fonts set
//!   once text has been laid out in a frame show from the next; a reloaded game starts in egui's.
//!
//! A `Ui` works only until the function it was passed to returns, and inside a container's
//! function, only the `Ui` it's passed works, as a canvas's `Paint` works only in its function.
//! Containers hold any others, their own kind too.
//! Tab reaches the interface only while one of its widgets has keyboard focus, so a game can use
//! it while its interface shows. Without the `egui` feature, `gui` isn't called.
//!
//! An error while the game loads ends [`run`] with that error. An error in `update`, `gui`,
//! `draw` or `close_requested` stops the game and shows the error in the window, and a stopped
//! game's window closes when asked. A task that fails stops and logs its
//! error, and the other tasks carry on.
//!
//! Run with [`run_watching`] (`wingine --watch path/to/game`), a game reloads whenever a file in
//! its folder changes, as the editor's Play runs games. Its window stays open, and a new VM runs
//! `main.luau` from the top, reading every script, scene, image, sound and model again. First, the
//! engine calls the game's global `reloading()`, if it has one, lending it what `update` is lent:
//! what it returns, plain data as `save.write` takes, the reloaded game gets from
//! `window.reloaded()`, so it can carry on where it was, even after stopping with an error. An
//! error in `reloading`, or data that isn't plain, is logged, and the game reloads keeping
//! nothing. Sound effects and music stop, volumes go back to 1, and actions are unbound, but
//! saves stay. `conf.luau` is read again for its `libraries`, but the window's settings change
//! only when the game starts again. Native libraries aren't loaded again, since they never
//! unload, but each reload gives them new state. A game that fails to load, as it starts or as it
//! reloads, shows the error in the window and loads again at the next change, keeping for it what
//! `reloading` returned before.
//!
//! A game ships as the folder [`export`] makes: the executable, named for the game, beside the
//! game's folder, called `game`. Started without a folder, as a double-click starts it, the
//! executable runs that one ([`shipped_game`], [`run_shipped`]), and shows any error loading it,
//! or reading its `conf.luau`, in its window, since players have no console to read it in.

mod audio;
#[cfg(not(target_arch = "wasm32"))]
pub mod export;
mod files;
#[cfg(not(target_arch = "wasm32"))]
mod fs_require;
mod graphics;
mod grid;
#[cfg(feature = "egui")]
mod gui;
mod input;
mod native;
mod physics;
#[cfg(all(feature = "egui", not(target_arch = "wasm32")))]
mod plugin;
#[cfg(target_arch = "wasm32")]
mod require;
mod save;
mod scene;
#[cfg(not(target_arch = "wasm32"))]
pub mod testing;
mod tilemap;
mod ui;
mod window;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::{Path, PathBuf},
    ptr,
    rc::Rc,
};

use wingine_abi::Phase;

use native::Natives;
pub use native::{LibraryDefinitions, link_library};
#[cfg(all(feature = "egui", not(target_arch = "wasm32")))]
pub use plugin::PluginVm;
#[cfg(target_arch = "wasm32")]
use require::FilesRequirer as FileRequirer;

#[cfg(not(target_arch = "wasm32"))]
use fs_require::{FsRequirer as FileRequirer, Lookups};

#[cfg(not(target_arch = "wasm32"))]
use crate::watch::Watcher;
use crate::{
    Context, Error, Game, Result, WindowConfig, WindowIcon,
    graphics::{Blend, Canvas, Color, Font, FontSettings, Model, Resolution, Texture},
    lua::{
        ApiModule, Bytecode, Scripts, mlua,
        mlua::{
            FromLuaMulti,
            luau::{NavigateError, Require},
        },
    },
    scene::{Value, plain},
};

/// How often a game [watching](LuauGame::load_watching) its folder looks at the files in it.
#[cfg(not(target_arch = "wasm32"))]
const WATCH_INTERVAL: Duration = Duration::from_millis(250);

/// The types of the engine's API for Luau games, as a definitions file for
/// [luau-lsp](https://github.com/JohnnyMorganz/luau-lsp): the API's tables and built-ins, the
/// userdata they hand out and the values they take, with their docs, so a language server checks
/// scripts against the engine and shows its docs on hover. It's `wingine.d.luau`, next to this
/// file.
pub const DEFINITIONS: &str = include_str!("wingine.d.luau");

/// What `require("@wingine")` returns, for luau-lsp: a module whose type is the definitions'
/// `Wingine`, which a game's `.luaurc` aliases as `wingine`, as the editor's base one does. The
/// engine never runs it. It's `wingine.luau`, next to this file.
pub const API_MODULE: &str = include_str!("wingine.luau");

/// Runs the Luau game in `folder` until its window is closed, its scripts' `arguments()` returning
/// `arguments`. See the [module docs](self).
///
/// On the web, it starts the game in a canvas added to the page and returns at once, and an
/// error loading the game is logged: see [`crate::run`].
pub fn run(folder: impl AsRef<Path>, arguments: Vec<String>) -> Result<()> {
    let folder = folder.as_ref().to_owned();
    let config = Config {
        arguments,
        ..config(&folder)?
    };
    crate::run(config.window.clone(), move |ctx| {
        LuauGame::load_with(ctx, &folder, &config)
    })
}

/// Runs the Luau game in `folder` until its window is closed, as [`run`] does, but reloading it
/// whenever a file in the folder changes, as [`LuauGame::load_watching`] does. Only an error
/// reading its `conf.luau` as it starts ends it with that error: one loading the game shows in the
/// window until the next change.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_watching(folder: impl AsRef<Path>, arguments: Vec<String>) -> Result<()> {
    let folder = folder.as_ref().to_owned();
    let config = Config {
        arguments,
        ..config(&folder)?
    };
    crate::run(config.window.clone(), move |ctx| {
        LuauGame::load_watching(ctx, &folder, &config)
    })
}

/// The folder beside the executable that it runs when it's started without one: see
/// [`shipped_game`].
pub const SHIPPED_FOLDER: &str = "game";

/// The game shipped with the running executable, as [`export`] lays it out: the folder called
/// `game` beside it, whatever the executable is called, if there's one.
#[cfg(not(target_arch = "wasm32"))]
pub fn shipped_game() -> Option<PathBuf> {
    game_beside(&std::env::current_exe().ok()?)
}

/// The folder called `game` beside `executable`, if there's one.
#[cfg(not(target_arch = "wasm32"))]
fn game_beside(executable: &Path) -> Option<PathBuf> {
    let folder = executable.parent()?.join(SHIPPED_FOLDER);
    folder.is_dir().then_some(folder)
}

/// Runs the Luau game in `folder` until its window is closed, as a game shipped to players runs,
/// started without a console to print to: an error loading it shows in its window, as
/// [`LuauGame::load_showing_errors`] shows it, and so does one reading its `conf.luau`, in a
/// window with the default settings. Its scripts' `arguments()` returns `arguments`.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_shipped(folder: impl AsRef<Path>, arguments: Vec<String>) -> Result<()> {
    let folder = folder.as_ref().to_owned();
    let (config, problem) = match config(&folder) {
        Ok(config) => (
            Config {
                arguments,
                ..config
            },
            None,
        ),
        Err(err) => {
            let mut window = WindowConfig::default();
            if let Some(name) = folder_name(&folder) {
                window.identity = identity(&name);
                window.title = name;
            }
            let config = Config {
                window,
                libraries: Vec::new(),
                arguments,
            };
            (config, Some(err))
        }
    };
    crate::run(config.window.clone(), move |ctx| match problem {
        Some(err) => LuauGame::stopped(ctx, &folder, &config, err),
        None => LuauGame::load_showing_errors(ctx, &folder, &config),
    })
}

/// A Luau game's settings, from its `conf.luau`, and the words its run hands it.
#[derive(Clone, Debug)]
pub struct Config {
    pub window: WindowConfig,
    /// The native libraries to load, in order.
    pub libraries: Vec<LibraryConfig>,
    /// The words the command that ran the game gave it, after `--`, which its scripts'
    /// `arguments()` returns. `conf.luau` never sets them, so [`config`] leaves them empty.
    pub arguments: Vec<String>,
}

/// A native library a game loads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryConfig {
    pub name: String,
    /// Whether the game runs without it if there's no such library. One that's there but fails to
    /// load is an error either way.
    pub optional: bool,
}

/// The definitions file of each native library the game in `folder` lists in its `conf.luau`, in
/// order: what a language server checks the game's scripts against, as [`DEFINITIONS`] is for
/// the engine's own API. `wingine definitions` writes them, each as `<library>.d.luau`. A library
/// is asked what it registers, as when a game loads it, but no game is loaded, so this needs no
/// window. Optional libraries that aren't there are left out.
pub fn definitions(folder: impl AsRef<Path>) -> Result<Vec<LibraryDefinitions>> {
    let folder = folder.as_ref();
    native::definitions(folder, &config(folder)?.libraries)
}

/// The settings of the game in `folder`, from its `conf.luau` if it has one.
pub fn config(folder: impl AsRef<Path>) -> Result<Config> {
    let folder = folder.as_ref();
    let name = folder_name(folder);
    let mut config = Config {
        window: WindowConfig {
            title: name
                .clone()
                .unwrap_or_else(|| WindowConfig::default().title),
            identity: name
                .as_deref()
                .map(identity)
                .unwrap_or_else(|| WindowConfig::default().identity),
            ..Default::default()
        },
        libraries: Vec::new(),
        arguments: Vec::new(),
    };
    let path = folder.join("conf.luau");
    if !crate::files::is_file(&path) {
        return Ok(config);
    }
    let source = crate::files::read_to_string(&path).map_err(|source| Error::ReadFile {
        path: path.clone(),
        source,
    })?;
    let lua = mlua::Lua::new();
    lua.sandbox(true)?;
    let settings: mlua::Table = lua
        .load(source)
        .set_name(format!("@{}", path.display()))
        .eval()?;
    for pair in settings.pairs::<String, mlua::Value>() {
        let (name, value) = pair?;
        apply_setting(&mut config, folder, &lua, &name, value).map_err(|err| Error::Script {
            path: path.clone(),
            message: format!("`{name}`: {err}"),
        })?;
    }
    if settings.contains_key("scaling")? && config.window.resolution == Resolution::Window {
        return Err(Error::Script {
            path,
            message: "`scaling`: it scales a fixed `resolution`, and there's none".into(),
        });
    }
    Ok(config)
}

/// Applies the setting `name` of the game in `folder` to `config`.
fn apply_setting(
    config: &mut Config,
    folder: &Path,
    lua: &mlua::Lua,
    name: &str,
    value: mlua::Value,
) -> mlua::Result<()> {
    let window = &mut config.window;
    match name {
        "title" => window.title = lua.unpack(value)?,
        "icon" => {
            let path: String = lua.unpack(value)?;
            let icon = WindowIcon::load(folder.join(path)).map_err(mlua::Error::runtime)?;
            window.icon = Some(icon);
        }
        "width" => window.width = positive(lua.unpack(value)?)?,
        "height" => window.height = positive(lua.unpack(value)?)?,
        "resizable" => window.resizable = lua.unpack(value)?,
        "fullscreen" => window.fullscreen = lua.unpack(value)?,
        "scaling" => window.scaling = lua.unpack(value)?,
        "vsync" => window.vsync = lua.unpack(value)?,
        "debug_key" => window.debug_key = Some(lua.unpack(value)?),
        "identity" => {
            let identity: String = lua.unpack(value)?;
            if let Some(problem) = crate::save::name_problem(&identity) {
                let message = format!("{problem}, as it names a folder");
                return Err(mlua::Error::runtime(message));
            }
            window.identity = identity;
        }
        "update_rate" => {
            let rate: f32 = lua.unpack(value)?;
            if !(rate > 0.0 && rate.is_finite()) {
                return Err(mlua::Error::runtime("must be above 0"));
            }
            window.update_rate = Some(rate);
        }
        "resolution" => {
            let size: mlua::Table = lua.unpack(value)?;
            window.resolution = Resolution::Fixed {
                width: positive(size.get("width")?)?,
                height: positive(size.get("height")?)?,
            };
        }
        "libraries" => {
            let entries: Vec<mlua::Value> = lua.unpack(value)?;
            config.libraries = entries
                .into_iter()
                .map(|entry| library(lua, entry))
                .collect::<mlua::Result<_>>()?;
        }
        _ => return Err(mlua::Error::runtime("unknown setting")),
    }
    Ok(())
}

/// An entry in the `libraries` setting: a library's name, or a table with its `name` and whether
/// it's `optional`.
fn library(lua: &mlua::Lua, entry: mlua::Value) -> mlua::Result<LibraryConfig> {
    let library = match entry {
        mlua::Value::Table(table) => {
            for pair in table.pairs::<mlua::Value, mlua::Value>() {
                let key = pair?.0.to_string()?;
                if key != "name" && key != "optional" {
                    let message = format!("a library has a `name` and `optional`, not `{key}`");
                    return Err(mlua::Error::runtime(message));
                }
            }
            LibraryConfig {
                name: table.get("name")?,
                optional: table.get::<Option<bool>>("optional")?.unwrap_or(false),
            }
        }
        name => LibraryConfig {
            name: lua.unpack(name)?,
            optional: false,
        },
    };
    if let Some(problem) = crate::save::name_problem(&library.name) {
        return Err(mlua::Error::runtime(format!(
            "`{}` is not a library name: {problem}",
            library.name
        )));
    }
    Ok(library)
}

fn positive(size: u32) -> mlua::Result<u32> {
    if size == 0 {
        return Err(mlua::Error::runtime("must be at least 1"));
    }
    Ok(size)
}

fn folder_name(folder: &Path) -> Option<String> {
    let folder = crate::files::canonicalize(folder).ok()?;
    Some(folder.file_name()?.to_string_lossy().into_owned())
}

/// The identity of a game in a folder called `name`, by default: the name, with `_` for whatever
/// can't be in an identity, and after a name Windows keeps for a device.
fn identity(name: &str) -> String {
    let mut identity: String = name
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' => c,
            _ => '_',
        })
        .collect();
    if !crate::save::is_name(&identity) {
        identity.push('_');
    }
    identity
}

/// What loading a game takes from loads before it, which its scripts can't tell from loading it
/// afresh: where `require` finds modules, and the bytecode scripts compile to. A game loaded for a
/// window makes its own, so a reload finds files changed since; a run of Luau tests shares one
/// between all its games, whose files don't change under it.
#[derive(Clone, Default)]
pub(crate) struct Reuse {
    #[cfg(not(target_arch = "wasm32"))]
    lookups: Lookups,
    /// Kept only for tests, as a window's load compiles each script once anyway.
    bytecode: Option<Bytecode>,
}

impl Reuse {
    /// What a run of Luau tests shares between its games: bytecode too.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn for_tests() -> Self {
        Self {
            lookups: Lookups::default(),
            bytecode: Some(Bytecode::default()),
        }
    }

    /// The bytecode kept, if it's kept.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn bytecode(&self) -> Option<&Bytecode> {
        self.bytecode.as_ref()
    }
}

/// A game written in Luau, loaded from its folder.
pub struct LuauGame {
    // The VM goes before the libraries' state, which its values may hold.
    scripts: Scripts,
    natives: Natives,
    lent: Lent,
    /// For showing an error.
    font: Font,
    /// The error that stopped the game, if one has.
    error: Option<Error>,
    /// Where the game is, and its settings as the window has them, for loading it again.
    folder: PathBuf,
    config: Config,
    /// What the game before this one returned from `reloading`, kept while this one failed to
    /// load, for the next.
    kept: Option<Value>,
    /// Watching the game's folder, to reload it as it changes.
    #[cfg(not(target_arch = "wasm32"))]
    watcher: Option<Watcher>,
}

impl LuauGame {
    /// Loads the game in `folder`: the native libraries its `conf.luau` lists, then its
    /// `main.luau`.
    pub fn load(ctx: &mut Context, folder: impl AsRef<Path>) -> Result<Self> {
        let folder = folder.as_ref();
        Self::load_with(ctx, folder, &config(folder)?)
    }

    /// Loads the game in `folder` with its settings, `config`, read already: the native
    /// libraries they list, then its `main.luau`.
    pub fn load_with(ctx: &mut Context, folder: impl AsRef<Path>, config: &Config) -> Result<Self> {
        Self::load_keeping(ctx, folder.as_ref(), config, None, Reuse::default())
    }

    /// Loads the game in `folder` as [`LuauGame::load_with`] does, taking what it can from
    /// `reuse`, and adding what it learns to it.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn load_reusing(
        ctx: &mut Context,
        folder: &Path,
        config: &Config,
        reuse: &Reuse,
    ) -> Result<Self> {
        Self::load_keeping(ctx, folder, config, None, reuse.clone())
    }

    /// Loads the game in `folder` as [`LuauGame::load_with`] does, but if that fails, returns a
    /// game stopped by the error, which shows it in the window, as one stopped as it plays does.
    /// Only failing to make a VM at all is an error.
    pub fn load_showing_errors(
        ctx: &mut Context,
        folder: impl AsRef<Path>,
        config: &Config,
    ) -> Result<Self> {
        let folder = folder.as_ref();
        Self::load_with(ctx, folder, config).or_else(|err| Self::stopped(ctx, folder, config, err))
    }

    /// Loads the game in `folder` as [`LuauGame::load_with`] does, then [reloads](Self::reload)
    /// it whenever a file in the folder changes, looking every quarter second. An error loading
    /// it, now or as it reloads, shows in the window, as one that stops it as it plays does, until
    /// the next change loads it again.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_watching(
        ctx: &mut Context,
        folder: impl AsRef<Path>,
        config: &Config,
    ) -> Result<Self> {
        let folder = folder.as_ref();
        let mut game = Self::load_showing_errors(ctx, folder, config)?;
        let watcher = Watcher::new(folder.to_owned(), WATCH_INTERVAL, ctx.waker());
        game.watcher = Some(watcher);
        Ok(game)
    }

    /// Loads the game again from its folder, in its place, as a game [watching its
    /// folder](Self::load_watching) does when a file changes: first calling its global
    /// `reloading`, if it has one, for the data the new game gets from `window.reloaded()`, then
    /// stopping its sounds and music, unbinding its actions, and running its `main.luau` from the
    /// top in a new VM, which reads every script and asset again. `conf.luau` is read again for its
    /// libraries; its window settings change only when the game starts again, and its
    /// [arguments](Config::arguments) never do. An error loading it stops it, showing the error,
    /// with an empty VM.
    pub fn reload(&mut self, ctx: &mut Context) {
        let kept = self.keep(ctx).or_else(|| self.kept.take());
        let config = config(&self.folder).map(|mut config| {
            if config.window != self.config.window {
                log::warn!("conf.luau's window settings change when the game starts again");
                config.window = self.config.window.clone();
            }
            config.arguments = self.config.arguments.clone();
            config
        });
        ctx.audio.reset();
        ctx.input.clear_bindings();
        #[cfg(feature = "egui")]
        {
            // Frames ask the new game for an interface.
            ctx.gui.game_has_gui = true;
        }
        // Nothing is reused: a reload is for files that changed.
        let loaded = config.and_then(|config| {
            Self::load_keeping(ctx, &self.folder, &config, kept.clone(), Reuse::default())
        });
        let game = match loaded {
            Ok(game) => game,
            Err(err) => match Self::stopped(ctx, &self.folder, &self.config, err) {
                Ok(game) => Self { kept, ..game },
                Err(err) => {
                    // Without a VM to stand in for it, the old game stays, stopped.
                    self.stop(err);
                    return;
                }
            },
        };
        #[cfg(not(target_arch = "wasm32"))]
        let game = Self {
            watcher: self.watcher.take(),
            ..game
        };
        // The old game goes as any does, VM first.
        *self = game;
    }

    /// What the game's `reloading` returns, as plain data, for the game loaded in its place:
    /// nothing if it has no `reloading`, or that returns nil or fails.
    fn keep(&self, ctx: &mut Context) -> Option<Value> {
        let reloading = self.scripts.lua().globals().get::<mlua::Value>("reloading");
        if reloading.is_ok_and(|reloading| reloading.is_nil()) {
            return None;
        }
        let lent = self.lent.clone();
        let state = lent.lend(ctx, || self.scripts.call::<mlua::Value>("reloading", ()));
        let kept = match state {
            Ok(mlua::Value::Nil) => return None,
            Ok(state) => plain(state, "what `reloading` returned"),
            Err(err) => Err(err.to_string()),
        };
        kept.inspect_err(|err| log::warn!("the game reloads keeping nothing: {err}"))
            .ok()
    }

    /// A game in `folder` stopped by `err` before its `main.luau` could run, which shows the
    /// error.
    fn stopped(ctx: &Context, folder: &Path, config: &Config, err: Error) -> Result<Self> {
        let mut game = Self {
            scripts: Scripts::new()?,
            natives: Natives::default(),
            lent: Lent::default(),
            font: ctx.default_font().clone(),
            error: None,
            folder: folder.to_owned(),
            config: config.clone(),
            kept: None,
            #[cfg(not(target_arch = "wasm32"))]
            watcher: None,
        };
        game.stop(err);
        Ok(game)
    }

    /// Loads the game in `folder` with its settings, `config`: the native libraries they list,
    /// then its `main.luau`, whose `window.reloaded()` returns `kept`, taking what it can from
    /// `reuse`.
    fn load_keeping(
        ctx: &mut Context,
        folder: &Path,
        config: &Config,
        kept: Option<Value>,
        reuse: Reuse,
    ) -> Result<Self> {
        let scripts = Scripts::new()?;
        let lua = scripts.lua();
        let lent = Lent::default();
        let font = ctx.default_font().clone();
        let textures = Textures::default();
        let api = scripts.api();
        let graphics = graphics::module(lua, &lent, folder, textures.clone(), font.clone())?;
        api.set("graphics", graphics)?;
        api.set("input", input::module(lua, &lent)?)?;
        api.set(
            "audio",
            audio::module(lua, &lent, folder, api.get("spawn")?)?,
        )?;
        api.set("tilemap", tilemap::module(lua)?)?;
        api.set("grid", grid::module(lua)?)?;
        api.set("physics", physics::module(lua)?)?;
        let scene = scene::module(lua, &lent, folder.to_owned(), textures)?;
        api.set("scene", scene)?;
        api.set("ui", ui::module(lua, &lent)?)?;
        api.set("save", save::module(lua, ctx.saves())?)?;
        api.set("files", files::module(lua, ctx.folders())?)?;
        api.set("window", window::module(lua, &lent, kept)?)?;
        scripts.set_arguments(config.arguments.clone())?;
        #[cfg(feature = "egui")]
        {
            gui::register(lua, &lent, folder)?;
            // A game reloaded in place starts in egui's fonts, as it would in a new window.
            gui::reset_fonts(&ctx.gui.egui);
        }
        let main = folder.join("main.luau");
        #[cfg(not(target_arch = "wasm32"))]
        let files = FileRequirer::new(reuse.lookups.clone(), reuse.bytecode.clone());
        #[cfg(target_arch = "wasm32")]
        let files = FileRequirer::new();
        let requirer = Requirer {
            files,
            api: ApiModule::new(api.clone()),
            main: format!("@{}", main.display()),
        };
        lua.globals()
            .set("require", lua.create_require_function(requirer)?)?;
        let natives = Natives::load(lua, &lent, ctx, folder, &config.libraries)?;
        let bytecode = reuse.bytecode.as_ref();
        lent.lend(ctx, || scripts.load_with_bytecode(&main, bytecode))?;
        natives.loaded();
        Ok(Self {
            scripts,
            natives,
            lent,
            font,
            error: None,
            folder: folder.to_owned(),
            config: config.clone(),
            kept: None,
            #[cfg(not(target_arch = "wasm32"))]
            watcher: None,
        })
    }

    /// The game's scripts, for looking at the game's state.
    pub fn scripts(&self) -> &Scripts {
        &self.scripts
    }

    /// The error that stopped the game, if one has.
    pub fn error(&self) -> Option<&Error> {
        self.error.as_ref()
    }

    /// Runs the Luau `source` as a chunk of its own, named `test`, lent the context as `update`
    /// is lent it, and returns what it returns. It `require`s as the game's `main.luau` does.
    pub(crate) fn run_chunk<R: FromLuaMulti>(&self, ctx: &mut Context, source: &str) -> Result<R> {
        let chunk = self.scripts.lua().load(source).set_name("=test");
        self.lent.lend(ctx, || Ok(chunk.eval()?))
    }

    /// Calls the script's global function `name`, if it has one.
    fn callback(&self, name: &str, args: impl mlua::IntoLuaMulti) -> Result<()> {
        if self
            .scripts
            .lua()
            .globals()
            .get::<mlua::Value>(name)?
            .is_nil()
        {
            return Ok(());
        }
        self.scripts.call(name, args)
    }

    fn stop(&mut self, err: Error) {
        log::error!("{err}");
        self.error = Some(err);
    }

    /// Whether the script has a global `gui`, to lay out its interface.
    #[cfg(feature = "egui")]
    fn has_gui(&self) -> bool {
        let gui = self.scripts.lua().globals().get::<mlua::Value>("gui");
        gui.is_ok_and(|gui| !gui.is_nil())
    }
}

impl Game for LuauGame {
    fn update(&mut self, ctx: &mut Context, dt: f32) {
        // A game stopped by an error reloads too, as that's how it's fixed.
        #[cfg(not(target_arch = "wasm32"))]
        if self.watcher.as_ref().is_some_and(Watcher::take_changed) {
            self.reload(ctx);
        }
        if self.error.is_some() {
            return;
        }
        if let Err(err) = self.run_update(ctx, dt) {
            self.stop(err);
        }
        // A game may define `gui` as it plays, after frames have stopped running an interface
        // because it had none.
        #[cfg(feature = "egui")]
        if !ctx.gui.game_has_gui && self.error.is_none() && self.has_gui() {
            ctx.gui.game_has_gui = true;
        }
    }

    #[cfg(feature = "egui")]
    fn gui(&mut self, ctx: &mut Context, ui: &mut crate::gui::egui::Ui) {
        let function = match self.error {
            None => self.scripts.lua().globals().get("gui"),
            Some(_) => Ok(None),
        };
        let result = function.map_err(Error::from).and_then(|function| {
            let Some(function) = function else {
                // Frames needn't run egui at all, as for a game with no interface.
                ctx.gui.game_has_gui = false;
                return Ok(());
            };
            let lent = self.lent.clone();
            lent.lend(ctx, || {
                gui::call::<()>(self.scripts.lua(), ui, &function).map_err(Error::from)
            })
        });
        if let Err(err) = result {
            self.stop(err);
        }
    }

    fn draw(&mut self, canvas: &mut Canvas) {
        if self.error.is_none()
            && let Err(err) = self.run_draw(canvas)
        {
            self.stop(err);
        }
        if let Some(err) = &self.error {
            draw_error(canvas, &self.font, err);
        }
        // The frame's last word: what its libraries cost is the frame before's from here.
        self.natives.end_frame();
    }

    fn close_requested(&mut self, ctx: &mut Context) -> bool {
        // A game stopped by an error can't answer, so it closes.
        if self.error.is_some() {
            return true;
        }
        match self.run_close_requested(ctx) {
            Ok(close) => close,
            Err(err) => {
                self.stop(err);
                true
            }
        }
    }
}

impl LuauGame {
    /// Runs the libraries' hooks before `update`, the script's `update` and its tasks, then the
    /// hooks after.
    fn run_update(&mut self, ctx: &mut Context, dt: f32) -> Result<()> {
        // What scripts call of the libraries from here is counted with the frame's updates.
        self.natives.set_updating(true);
        let result = self.update_scripts(ctx, dt);
        self.natives.set_updating(false);
        result
    }

    fn update_scripts(&mut self, ctx: &mut Context, dt: f32) -> Result<()> {
        self.natives.update(Phase::BEFORE, ctx, dt)?;
        // Tasks started before this frame, such as while the game loaded, resume in it. Those
        // `update` starts have run until they first waited, and resume from the next frame.
        self.scripts.adopt_spawned();
        let lent = self.lent.clone();
        lent.lend(ctx, || {
            self.callback("update", dt)?;
            if let Err(err) = self.scripts.resume_tasks(dt) {
                log::error!("{err}");
            }
            Ok(())
        })?;
        self.natives.update(Phase::AFTER, ctx, dt)
    }

    /// Runs the libraries' hooks before `draw`, the script's `draw`, then the hooks after, which
    /// draw in canvas pixels and blend normally whatever camera and blend mode the script left set.
    fn run_draw(&mut self, canvas: &mut Canvas) -> Result<()> {
        self.natives.draw(Phase::BEFORE, canvas)?;
        let lent = self.lent.clone();
        lent.lend(canvas, || self.callback("draw", ()))?;
        canvas.set_camera(None);
        canvas.set_blend(Blend::Normal);
        self.natives.draw(Phase::AFTER, canvas)
    }

    /// Asks the script's `close_requested` whether the window closes, lending it the context as
    /// `update` is lent it. Without one, it does; with one, unless it returns false.
    fn run_close_requested(&mut self, ctx: &mut Context) -> Result<bool> {
        let globals = self.scripts.lua().globals();
        if globals.get::<mlua::Value>("close_requested")?.is_nil() {
            return Ok(true);
        }
        let lent = self.lent.clone();
        let answer: mlua::Value = lent.lend(ctx, || self.scripts.call("close_requested", ()))?;
        Ok(answer != mlua::Value::Boolean(false))
    }
}

/// Shows `err` in place of the game.
fn draw_error(canvas: &mut Canvas, font: &Font, err: &Error) {
    const MARGIN: f32 = 8.0;
    canvas.clear(Color::rgb8(0x30, 0x10, 0x18));
    canvas.set_camera(None);
    canvas.set_blend(Blend::Normal);
    // Tracebacks indent with tabs, which the font doesn't have.
    let text = format!("The game stopped with an error:\n\n{err}").replace('\t', "  ");
    let width = (canvas.width() - MARGIN * 2.0).max(MARGIN);
    let layout = font.layout(&text, Some(width));
    canvas.draw_text_layout(&layout, .., [MARGIN, MARGIN], Color::WHITE);
}

/// Finds the modules a game's scripts `require` as mlua's own requirer does, by their paths
/// relative to the script requiring them, but has chunks that aren't files, named `=name`, such as
/// those tests run, require as the game's `main.luau` does, so they get the modules the game has.
/// `@wingine` is the engine's API, whatever any `.luaurc` says.
struct Requirer {
    files: FileRequirer,
    api: ApiModule,
    /// The chunk name of the game's `main.luau`.
    main: String,
}

impl Requirer {
    /// The name of the chunk that a chunk called `name` requires as.
    fn requiring_as<'a>(main: &'a str, name: &'a str) -> &'a str {
        if name.starts_with('=') { main } else { name }
    }
}

impl Require for Requirer {
    fn is_require_allowed(&self, chunk_name: &str) -> bool {
        let name = Self::requiring_as(&self.main, chunk_name);
        self.files.is_require_allowed(name)
    }

    fn reset(&mut self, chunk_name: &str) -> Result<(), NavigateError> {
        self.api.leave();
        let name = Self::requiring_as(&self.main, chunk_name);
        self.files.reset(name)
    }

    fn jump_to_alias(&mut self, path: &str) -> Result<(), NavigateError> {
        self.api.leave();
        self.files.jump_to_alias(path)
    }

    fn to_alias_override(&mut self, alias: &str) -> Result<(), NavigateError> {
        match self.api.choose(alias) {
            Ok(()) => Ok(()),
            Err(_) => self.files.to_alias_override(alias),
        }
    }

    fn to_alias_fallback(&mut self, alias: &str) -> Result<(), NavigateError> {
        self.api.leave();
        self.files.to_alias_fallback(alias)
    }

    fn to_parent(&mut self) -> Result<(), NavigateError> {
        if self.api.leave() {
            return Err(NavigateError::NotFound);
        }
        self.files.to_parent()
    }

    fn to_child(&mut self, name: &str) -> Result<(), NavigateError> {
        if self.api.leave() {
            return Err(NavigateError::NotFound);
        }
        self.files.to_child(name)
    }

    fn has_module(&self) -> bool {
        self.api.is_chosen() || self.files.has_module()
    }

    fn cache_key(&self) -> String {
        if self.api.is_chosen() {
            return ApiModule::cache_key();
        }
        self.files.cache_key()
    }

    fn has_config(&self) -> bool {
        !self.api.is_chosen() && self.files.has_config()
    }

    fn config(&self) -> std::io::Result<Vec<u8>> {
        self.files.config()
    }

    fn loader(&self, lua: &mlua::Lua) -> mlua::Result<mlua::Function> {
        if self.api.is_chosen() {
            return self.api.loader(lua);
        }
        self.files.loader(lua)
    }
}

/// The images, fonts and model files a game has loaded, by path, so each is read once however
/// many sprite sheets, scenes and scripts use it.
#[derive(Clone, Default)]
struct Textures(Rc<Loaded>);

#[derive(Default)]
struct Loaded {
    textures: RefCell<HashMap<PathBuf, Texture>>,
    fonts: RefCell<HashMap<PathBuf, Font>>,
    /// TrueType and OpenType fonts by path and whether they're smooth, at the size they were
    /// first loaded at; other sizes share their files.
    vector_fonts: RefCell<HashMap<(PathBuf, bool), Font>>,
    models: RefCell<HashMap<PathBuf, Model>>,
}

impl Textures {
    /// The image at `path`, loading it if it hasn't been.
    fn load(&self, ctx: &mut Context, path: PathBuf) -> Result<Texture> {
        if let Some(texture) = self.0.textures.borrow().get(&path) {
            return Ok(texture.clone());
        }
        let texture = ctx.load_texture(&path)?;
        self.0.textures.borrow_mut().insert(path, texture.clone());
        Ok(texture)
    }

    /// The font file at `path`, loading it if it hasn't been.
    fn font(&self, ctx: &mut Context, path: PathBuf) -> Result<Font> {
        if let Some(font) = self.0.fonts.borrow().get(&path) {
            return Ok(font.clone());
        }
        let font = ctx.load_font(&path)?;
        self.0.fonts.borrow_mut().insert(path, font.clone());
        Ok(font)
    }

    /// The TrueType or OpenType font at `path`, drawn as `settings` say, loading it if it hasn't
    /// been as smooth or crisp, and otherwise sharing its file with the size loaded first.
    fn vector_font(
        &self,
        ctx: &mut Context,
        path: PathBuf,
        settings: FontSettings,
    ) -> Result<Font> {
        let key = (path, settings.smooth);
        if let Some(font) = self.0.vector_fonts.borrow().get(&key) {
            return match font.at_size(settings.size) {
                Some(font) => font.map_err(|message| Error::Font {
                    path: key.0.clone(),
                    message,
                }),
                None => Ok(font.clone()),
            };
        }
        let font = ctx.load_vector_font(&key.0, settings)?;
        self.0.vector_fonts.borrow_mut().insert(key, font.clone());
        Ok(font)
    }

    /// The model file at `path`, loading it if it hasn't been.
    fn model(&self, ctx: &mut Context, path: PathBuf) -> Result<Model> {
        if let Some(model) = self.0.models.borrow().get(&path) {
            return Ok(model.clone());
        }
        let model = crate::scene::load_model(ctx, &path)?;
        self.0.models.borrow_mut().insert(path, model.clone());
        Ok(model)
    }
}

/// What the engine lends its Luau functions while scripts may use it: the context while the game
/// loads and updates, and the canvas while it draws. Every engine function in a VM holds a handle
/// to the same one.
///
/// Engine functions run on every draw call, so reaching what's lent has to be cheap: it's a
/// pointer that [`Lent::lend`] sets for as long as it runs, which each function holds, rather than
/// a lookup in the VM's registry or app data.
#[derive(Clone, Default)]
struct Lent(Rc<Loans>);

impl Lent {
    /// Lends `value` to the engine's Luau functions while `f` runs. Called at any other time, they
    /// fail with a Lua error rather than reach it.
    fn lend<T: Lendable, R>(&self, value: &mut T, f: impl FnOnce() -> Result<R>) -> Result<R> {
        let loan = T::loan(&self.0);
        // Whatever an outer call lent comes back however `f` ends, so nothing reaches `value`
        // after.
        let _value = Restore(&loan.value, loan.value.replace(value));
        let _borrowed = Restore(&loan.borrowed, loan.borrowed.replace(false));
        f()
    }

    /// Runs `f` with what's lent of `T`, if anything is.
    fn with<T: Lendable, R>(&self, f: impl FnOnce(&mut T) -> R) -> mlua::Result<Option<R>> {
        T::loan(&self.0).with(f)
    }

    /// What an editor plugin's functions hold, which is never lent anything, as there's no game
    /// running: those that need one fail with [`NoGame`].
    #[cfg(all(feature = "egui", not(target_arch = "wasm32")))]
    fn for_plugin() -> Self {
        Self(Rc::new(Loans {
            plugin: true,
            ..Loans::default()
        }))
    }

    /// Whether this is an editor plugin's, which has no game running.
    #[cfg_attr(
        not(feature = "egui"),
        allow(dead_code, reason = "only interfaces ask, and plugins need them")
    )]
    fn is_plugin(&self) -> bool {
        self.0.plugin
    }

    /// Runs `f` with the context, which the engine lends while the game loads and updates.
    fn context<R>(&self, f: impl FnOnce(&mut Context) -> R) -> mlua::Result<R> {
        self.with(f)?.ok_or_else(|| {
            if self.0.plugin {
                return mlua::Error::external(NoGame);
            }
            mlua::Error::runtime(
                "this only works while the game loads or in `update`, not in `draw`",
            )
        })
    }

    /// Runs `f` with the canvas, which the engine lends while the game draws.
    fn canvas<R>(&self, f: impl FnOnce(&mut Canvas) -> R) -> mlua::Result<R> {
        self.with(f)?.ok_or_else(|| {
            if self.0.plugin {
                return mlua::Error::external(NoGame);
            }
            mlua::Error::runtime("drawing only works in `draw`")
        })
    }
}

/// Why something an editor plugin called failed: it needs a running game, and there's none.
#[derive(Debug)]
struct NoGame;

impl std::fmt::Display for NoGame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("this needs a running game, and editor plugins have none")
    }
}

impl std::error::Error for NoGame {}

#[derive(Default)]
struct Loans {
    context: Loan<Context>,
    canvas: Loan<Canvas>,
    /// Whether these are an editor plugin's, which are never lent anything.
    plugin: bool,
}

/// A value the engine lends its Luau functions, which one of them at a time may borrow.
struct Loan<T> {
    /// What's lent, or null.
    value: Cell<*mut T>,
    /// Whether a function has it borrowed.
    borrowed: Cell<bool>,
    /// What scripts call it, for errors.
    name: &'static str,
}

impl<T> Loan<T> {
    fn new(name: &'static str) -> Self {
        Self {
            value: Cell::new(ptr::null_mut()),
            borrowed: Cell::new(false),
            name,
        }
    }

    /// Runs `f` with what's lent, if anything is.
    fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> mlua::Result<Option<R>> {
        let value = self.value.get();
        if value.is_null() {
            return Ok(None);
        }
        if self.borrowed.replace(true) {
            let message = format!("the {} is already in use", self.name);
            return Err(mlua::Error::runtime(message));
        }
        let _borrowed = Restore(&self.borrowed, false);
        // SAFETY: `Lent::lend` keeps the value alive, and reaches it only through this, until it
        // takes it back, and `borrowed` makes this the only reference to it until `f` returns.
        Ok(Some(f(unsafe { &mut *value })))
    }
}

impl Default for Loan<Context> {
    fn default() -> Self {
        Self::new("context")
    }
}

impl Default for Loan<Canvas> {
    fn default() -> Self {
        Self::new("canvas")
    }
}

/// Sets a cell back to a value when dropped, however the scope it guards ends.
struct Restore<'a, T: Copy>(&'a Cell<T>, T);

impl<T: Copy> Drop for Restore<'_, T> {
    fn drop(&mut self) {
        self.0.set(self.1);
    }
}

/// The engine services its Luau functions can be lent.
trait Lendable: Sized + 'static {
    fn loan(loans: &Loans) -> &Loan<Self>;
}

impl Lendable for Context {
    fn loan(loans: &Loans) -> &Loan<Self> {
        &loans.context
    }
}

impl Lendable for Canvas {
    fn loan(loans: &Loans) -> &Loan<Self> {
        &loans.canvas
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_identities_name_folders_on_every_system() {
        assert_eq!(super::identity("Café Quest 2!"), "Caf__Quest_2_");
        assert_eq!(super::identity("con"), "con_", "a device on Windows");
        assert_eq!(super::identity("LPT1"), "LPT1_");
        assert_eq!(super::identity("console"), "console");
    }
}
