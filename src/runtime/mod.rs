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
//! The engine's API is in ten global tables:
//!
//! `graphics`, for drawing in `draw`. `graphics.size`, `graphics.color`, `graphics.default_font`,
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
//!   table: `frame`, `update`, `draw` and `render` are seconds, and `quads` and `draw_calls`
//!   counts. Before the first frame has finished, all are 0. See `FrameStats` for what each
//!   measures.
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
//!   `color` and `ambient`, a sun's light and light from all around.
//! - `graphics.draw_mesh(mesh, position, options)` draws a mesh in the view begun last, its origin
//!   at `position`, a vector, scaled by `options.scale` (a number, or a vector for each axis),
//!   turned by `roll` about `z`, `pitch` about `x` and `yaw` about `y`, in that order, showing
//!   `sprite` multiplied by `tint`. Drawing one with no view open is an error.
//! - `graphics.draw_model(model, position, options)` draws each part of a model as `draw_mesh`
//!   would, placed in the model, and the model placed by `position` and `options` as a mesh is,
//!   all multiplied by `options.tint`. Its parts show their own sprites, so `options.sprite` is
//!   an error.
//! - `graphics.draw_billboard(sprite, position, tint, width, height)` draws a sprite standing
//!   upright in the view begun last, the middle of its bottom edge at `position`, turned to face
//!   the camera across the ground: `height` tall (1 by default) and `width` wide (by default, as
//!   its shape makes it).
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
//!   overlaps ask it where things are.
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
//!
//! `input`, while the game loads and in `update`:
//!
//! - `input.bind(action, controls, gamepad)` binds an action, such as `"jump"`, to a list of
//!   keys and gamepad buttons, such as `{ "Space", "W", "South" }`, replacing any it had. A
//!   gamepad's buttons include its sticks' directions, such as `"LeftStickLeft"`. With
//!   `gamepad`, a number from 1, only that gamepad's buttons count; without, any gamepad's do.
//! - `input.bindings(action)` returns what an action is bound to, a list of names in the order
//!   they were bound, empty if it isn't, and the gamepad it was bound to, if one, so a game shows
//!   the player its first: "Space: Examine".
//! - `input.pressed_controls()` returns the keys and gamepad buttons pressed this frame, in the
//!   order they were pressed, named as `input.bind` takes them, as a screen that lets players
//!   choose their controls asks which they pressed. Gamepad buttons are any gamepad's.
//! - `input.held(action)`, `input.pressed(action)` and `input.released(action)` tell whether any
//!   of the action's controls is held, or was pressed or released this frame.
//! - `input.strength(action)` tells how far it's pushed, from 0 to 1: 1 for a key, and for a
//!   trigger or a stick's direction, how far that is. `input.axis(negative, positive)` is how far
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
//!   cursor is outside the window.
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
//! `audio`, while the game loads and in `update`. `audio.load` works anywhere.
//!
//! - `audio.load(path)` loads a sound effect from a WAV or OGG Vorbis file.
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
//!   `ui:group(fn)`, `ui:collapsing(title, fn)`, `ui:scroll(fn)`, `ui:grid(name, fn)` with
//!   `ui:end_row()`, and `ui:enabled(enabled, fn)`.
//! - Text: `ui:label(text, color)`, `ui:heading(text)`, `ui:monospace(text)` and `ui:small(text)`.
//! - Widgets: `ui:button(text)` and `ui:sprite_button(sprite, text, selected, tint, width,
//!   height)`, a sprite in a frame, named `text` and framed as chosen when `selected`, return
//!   whether they were clicked; `ui:checkbox(text, checked)`,
//!   `ui:radio(current, value, text)`, `ui:combo(text, current, choices)`,
//!   `ui:slider(text, value, min, max, step)`, `ui:drag(text, value, speed, min, max)` and
//!   `ui:text_input(text, hint)` (which also returns whether Enter was pressed) return their
//!   values; `ui:progress(fraction, text)` and `ui:sprite(sprite, tint, width, height)` show them.
//! - `ui:separator()` and `ui:space(amount)`.
//!
//! A `Ui` works only until the function it was passed to returns, and inside a container's
//! function, only the `Ui` it's passed works. Without the `egui` feature, `gui` isn't called.
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
mod graphics;
mod grid;
#[cfg(feature = "egui")]
mod gui;
mod input;
mod native;
mod physics;
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
pub use native::link_library;
#[cfg(target_arch = "wasm32")]
use require::FilesRequirer as FileRequirer;

#[cfg(not(target_arch = "wasm32"))]
use crate::lua::mlua::luau::FsRequirer as FileRequirer;
#[cfg(not(target_arch = "wasm32"))]
use crate::watch::Watcher;
use crate::{
    Context, Error, Game, Result, WindowConfig, WindowIcon,
    graphics::{Blend, Canvas, Color, Font, FontSettings, Model, Resolution, Texture},
    lua::{
        Scripts, mlua,
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
/// [luau-lsp](https://github.com/JohnnyMorganz/luau-lsp): the global tables and built-ins, the
/// userdata they hand out and the values they take, with their docs, so a language server checks
/// scripts against the engine and shows its docs on hover. It's `wingine.d.luau`, next to this
/// file.
pub const DEFINITIONS: &str = include_str!("wingine.d.luau");

/// Runs the Luau game in `folder` until its window is closed. See the [module docs](self).
///
/// On the web, it starts the game in a canvas added to the page and returns at once, and an
/// error loading the game is logged: see [`crate::run`].
pub fn run(folder: impl AsRef<Path>) -> Result<()> {
    let folder = folder.as_ref().to_owned();
    let config = config(&folder)?;
    crate::run(config.window.clone(), move |ctx| {
        LuauGame::load_with(ctx, &folder, &config)
    })
}

/// Runs the Luau game in `folder` until its window is closed, as [`run`] does, but reloading it
/// whenever a file in the folder changes, as [`LuauGame::load_watching`] does. Only an error
/// reading its `conf.luau` as it starts ends it with that error: one loading the game shows in the
/// window until the next change.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_watching(folder: impl AsRef<Path>) -> Result<()> {
    let folder = folder.as_ref().to_owned();
    let config = config(&folder)?;
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
/// window with the default settings.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_shipped(folder: impl AsRef<Path>) -> Result<()> {
    let folder = folder.as_ref().to_owned();
    let (config, problem) = match config(&folder) {
        Ok(config) => (config, None),
        Err(err) => {
            let mut window = WindowConfig::default();
            if let Some(name) = folder_name(&folder) {
                window.identity = identity(&name);
                window.title = name;
            }
            let config = Config {
                window,
                libraries: Vec::new(),
            };
            (config, Some(err))
        }
    };
    crate::run(config.window.clone(), move |ctx| match problem {
        Some(err) => LuauGame::stopped(ctx, &folder, &config, err),
        None => LuauGame::load_showing_errors(ctx, &folder, &config),
    })
}

/// A Luau game's settings, from its `conf.luau`.
#[derive(Clone, Debug)]
pub struct Config {
    pub window: WindowConfig,
    /// The native libraries to load, in order.
    pub libraries: Vec<LibraryConfig>,
}

/// A native library a game loads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryConfig {
    pub name: String,
    /// Whether the game runs without it if there's no such library. One that's there but fails to
    /// load is an error either way.
    pub optional: bool,
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
        Self::load_keeping(ctx, folder.as_ref(), config, None)
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
    /// libraries; its window settings change only when the game starts again. An error loading it
    /// stops it, showing the error, with an empty VM.
    pub fn reload(&mut self, ctx: &mut Context) {
        let kept = self.keep(ctx).or_else(|| self.kept.take());
        let config = config(&self.folder).map(|mut config| {
            if config.window != self.config.window {
                log::warn!("conf.luau's window settings change when the game starts again");
                config.window = self.config.window.clone();
            }
            config
        });
        ctx.audio.reset();
        ctx.input.clear_bindings();
        #[cfg(feature = "egui")]
        {
            // Frames ask the new game for an interface.
            ctx.gui.game_has_gui = true;
        }
        let loaded =
            config.and_then(|config| Self::load_keeping(ctx, &self.folder, &config, kept.clone()));
        let mut game = match loaded {
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
        {
            game.watcher = self.watcher.take();
        }
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
    /// then its `main.luau`, whose `window.reloaded()` returns `kept`.
    fn load_keeping(
        ctx: &mut Context,
        folder: &Path,
        config: &Config,
        kept: Option<Value>,
    ) -> Result<Self> {
        let scripts = Scripts::new()?;
        let lua = scripts.lua();
        let lent = Lent::default();
        let font = ctx.default_font().clone();
        let textures = Textures::default();
        let globals = lua.globals();
        let graphics = graphics::module(lua, &lent, folder, textures.clone(), font.clone())?;
        globals.set("graphics", graphics)?;
        globals.set("input", input::module(lua, &lent)?)?;
        globals.set("audio", audio::module(lua, &lent, folder)?)?;
        globals.set("tilemap", tilemap::module(lua)?)?;
        globals.set("grid", grid::module(lua)?)?;
        globals.set("physics", physics::module(lua)?)?;
        let scene = scene::module(lua, &lent, folder.to_owned(), textures)?;
        globals.set("scene", scene)?;
        globals.set("ui", ui::module(lua, &lent)?)?;
        globals.set("save", save::module(lua, ctx.saves())?)?;
        globals.set("window", window::module(lua, &lent, kept)?)?;
        #[cfg(feature = "egui")]
        gui::register(lua, &lent)?;
        let main = folder.join("main.luau");
        let requirer = Requirer {
            files: FileRequirer::new(),
            main: format!("@{}", main.display()),
        };
        globals.set("require", lua.create_require_function(requirer)?)?;
        let natives = Natives::load(lua, &lent, ctx, folder, &config.libraries)?;
        lent.lend(ctx, || scripts.load(&main))?;
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
struct Requirer {
    files: FileRequirer,
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
        let name = Self::requiring_as(&self.main, chunk_name);
        self.files.reset(name)
    }

    fn jump_to_alias(&mut self, path: &str) -> Result<(), NavigateError> {
        self.files.jump_to_alias(path)
    }

    fn to_alias_override(&mut self, alias: &str) -> Result<(), NavigateError> {
        self.files.to_alias_override(alias)
    }

    fn to_alias_fallback(&mut self, alias: &str) -> Result<(), NavigateError> {
        self.files.to_alias_fallback(alias)
    }

    fn to_parent(&mut self) -> Result<(), NavigateError> {
        self.files.to_parent()
    }

    fn to_child(&mut self, name: &str) -> Result<(), NavigateError> {
        self.files.to_child(name)
    }

    fn has_module(&self) -> bool {
        self.files.has_module()
    }

    fn cache_key(&self) -> String {
        self.files.cache_key()
    }

    fn has_config(&self) -> bool {
        self.files.has_config()
    }

    fn config(&self) -> std::io::Result<Vec<u8>> {
        self.files.config()
    }

    fn loader(&self, lua: &mlua::Lua) -> mlua::Result<mlua::Function> {
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

    /// Runs `f` with the context, which the engine lends while the game loads and updates.
    fn context<R>(&self, f: impl FnOnce(&mut Context) -> R) -> mlua::Result<R> {
        self.with(f)?.ok_or_else(|| {
            mlua::Error::runtime(
                "this only works while the game loads or in `update`, not in `draw`",
            )
        })
    }

    /// Runs `f` with the canvas, which the engine lends while the game draws.
    fn canvas<R>(&self, f: impl FnOnce(&mut Canvas) -> R) -> mlua::Result<R> {
        self.with(f)?
            .ok_or_else(|| mlua::Error::runtime("drawing only works in `draw`"))
    }
}

#[derive(Default)]
struct Loans {
    context: Loan<Context>,
    canvas: Loan<Canvas>,
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
