# Wingine

Wingine is a desktop game engine written in Rust, built as a library crate (`wingine`). It is
primarily a **2D** engine, with limited 3D: meshes and billboarded sprites drawn in 3D views
among the 2D, from Rust and Luau. 3D is not a focus — don't let 3D concerns drive the design of 2D
systems, and don't build more 3D features unless asked.

## Scope

Wingine has what its examples need: tilemaps and batched sprites, meshes and glTF models in 3D
views picked by boxes, sprite animation, a camera, keyboard, mouse and gamepad input bound to
actions, grid movement, physics with Rapier (collision in 2D; bodies and characters in 3D views), text, audio, saves, Luau scripting with tasks, native libraries written
in Rust, scenes (levels, screens and rigged characters), menus over scenes' nodes, egui
interfaces, and an editor built on
the engine. Each feature is finished with a game or example that uses it, with tests.

- Scenes are data: files of plain Luau values, with no calls or expressions, so tools write back
  what they read. Behavior stays in games, which find nodes by path, tag or kind and read their
  props; nodes don't have scripts.
- The shmup's two bullet systems are one game, and its Luau one is written as well as Luau allows
  (pooled bullets, `--!native`), so comparing it with Rust stays fair.
- Speedups come from measurements, and CI never asserts timings.

Ask before starting new features, and especially general-purpose ones such as physics engines,
networking or 3D pipelines.

## Tech stack

| Concern   | Crate   | Notes                                    |
|-----------|---------|------------------------------------------|
| Rendering | `wgpu`  | All GPU work goes through wgpu           |
| Windowing | `winit` | Window creation, event loop, raw input   |
| Audio     | `kira`  | Music and sound effects                  |
| Scripting | `mlua`  | Luau, with `luau-jit` off the web        |
| Native libraries | `libloading` | Loads them, and never unloads them |
| Interfaces | `egui` | With `egui-winit` and `egui-wgpu`, behind the default `egui` feature |
| Gamepads | `gilrs` | Read by the window; simulated without one |
| Models | `gltf` | glTF 2.0 files, behind the default `models` feature |
| Physics | `rapier2d`, `rapier3d` | Behind the default `physics` feature |

- Rust edition 2024.
- Licensed under MIT or Apache 2.0, at the user's option (`LICENSE-MIT`, `LICENSE-APACHE`), which
  every crate's manifest says with `license = "MIT OR Apache-2.0"`; new crates say so too.
- Target platforms: desktop (Windows, macOS, Linux), and the web (`wasm32-unknown-unknown`, with
  WebGPU), as `docs/web.md` describes; building for the web patches in mlua's master branch until
  a release supports it. Code that differs on the web is behind `cfg(target_arch = "wasm32")`, and
  files games load are read through `crate::files`, never `std::fs`.
- `wgpu`, `winit`, `mlua`, `kira` and `egui` change their APIs substantially between releases.
  Check the version pinned in `Cargo.toml` and that version's docs on docs.rs rather than relying
  on memory of older APIs. egui's crates must target the engine's wgpu and winit, so they're
  upgraded together; egui crates outside egui's own repository, such as `egui_dock`, often lag a
  release behind.
- The `egui` feature, on by default, brings egui in; `default-features = false` builds the engine
  without it, which CI lints. egui's own icon fonts are left out unless its
  `monochrome_emoji_fonts` feature is on, as the editor turns it on.
- The `models` feature, on by default, reads model files with `gltf`, built without its default
  features (whose `import` would bring a JPEG decoder): the engine reads buffers, data URIs and
  PNG textures itself. Without it, `Context::load_model` doesn't exist and Luau's
  `graphics.load_model` fails; the `meshes` example requires it.
- The `physics` feature, on by default, brings `rapier2d` and `rapier3d` in, built with their
  `enhanced-determinism`, without which their results differ between Windows and Linux; it costs
  a few percent. With it, meshes keep their triangles on the CPU too (`Mesh::geometry`), for
  bodies shaped like them. Without the feature, `wingine::physics` doesn't exist and Luau's
  `physics.world` and `physics.world_3d` fail; the garden and the course need it. Rapier breaks its
  API in most minor releases, and is Apache-2.0 only, shipping no license file.
- `kira` is built with only the WAV and OGG Vorbis decoders. Its output, `cpal`, links against
  ALSA on Linux, so building there needs `libasound2-dev` (CI installs it).
- `gilrs` reads gamepads through Windows Gaming Input on Windows, which hears them only while the
  process has a focused window, and evdev on Linux, through libudev, so building there needs
  `libudev-dev` too (CI installs it). The engine skips gilrs's filters but the D-pad's and the
  jitter's, applying its own dead zone, since gilrs's comes only from drivers that report one.
- Scripts are **Luau**, not standard Lua 5.x. Luau has its own syntax additions (type annotations,
  compound assignment, `continue`, generalized `for ... in t`) and a sandboxed standard library.
- `mlua` builds Luau from C++ source, so building needs a C++ compiler (MSVC on Windows, `g++` or
  `clang` elsewhere). The first build takes about 30 seconds longer because of it.
- The editor's Script screen runs [luau-lsp](https://github.com/JohnnyMorganz/luau-lsp), the Luau
  language server, if it's installed: a program on the `PATH`, or `WINGINE_LUAU_LSP`. Tests that
  need it check nothing without it, unless `WINGINE_LUAU_LSP` is set, as CI sets it to the release
  it downloads (pinned in `ci.yml`). Its definitions syntax is unstable, so check
  `wingine.d.luau` still loads after upgrading it.

## Layout

- `src/app.rs`: the public `run` entry point, the `Game` trait, and the winit event loop. `run`
  takes an init closure that builds the game once the GPU is ready. With `WINGINE_SCRIPT` set,
  `run` hands off to the script runner instead of opening a window. `run_frame` runs a frame for
  both the window and `Simulation`, and `Timestep` decides its updates: one by the frame's time,
  or with `WindowConfig::update_rate`, as many fixed steps as the frame's time holds (possibly
  none). Frames close to a whole number of steps count as exactly that, and updates fall due a
  quarter step after frames at the rate end, so jitter doesn't move updates between frames.
  After the updates, `run_frame` runs the egui pass (`Game::gui`), then `draw`. The window's close
  button asks `Game::close_requested`, and `Context::quit` closes the window after the frame.
  Frames come one after another, or with `Redraw::OnDemand` (`WindowConfig::redraw`, for tools),
  only when something may have changed: the window's events request a redraw, and `Redraws`
  (shared by the `Context`, `Waker`s and egui's repaint callback) holds when the next frame is
  due, which `about_to_wait` waits until, woken early by an `EventLoopProxy` when a frame is
  wanted sooner. Each frame forgets what was asked for as it begins.
- `src/context.rs`: `Context`, the engine services passed to games: input, audio, saves, the
  canvas size, the built-in font, texture loading, and `FrameStats` for the frame before (CPU time
  spent updating, drawing and rendering, the whole frame's time, and quads and draw calls).
  Games get it in their init closure and in `Game::update`; Luau games call
  `graphics.frame_stats()`. Games that redraw on demand ask for frames with
  `request_redraw` and `request_redraw_after`, and from other threads with a `Waker`.
- `src/save.rs`: `Saves`, a game's saves: named files of plain `Value`s, `<name>.luau` in a
  folder of the game's own, read with the scene reader and written in its canonical layout
  (`scene::evaluate_any`, `plain` and `write_value`), each to a file of its own and then renamed
  over the old one. A window keeps them in `Wingine/<identity>` among the system's applications'
  data (`WindowConfig::identity`, `identity` in `conf.luau`), found from environment variables
  rather than a crate; `WindowConfig::save_folder` puts them elsewhere, and a `Simulation` without
  one keeps them in a `TemporaryFolder` of its own, removed with it, so tests and scripted runs
  start with none and never touch the player's. Names, and identities and library names, are
  letters, digits, `_` and `-` (`is_name`). Its unit tests don't need a GPU.
- `src/gui.rs`: egui over games, `wingine::gui`, which re-exports `egui`. `Gui` (in the
  `Context`) holds egui's context, the input for its next pass, and what the last pass drew; its
  renderer is made the first time there's something to draw, and games that don't override
  `Game::gui` skip the pass entirely while the debug window is closed. egui renders over the
  finished frame, at the output's resolution, through a non-sRGB view of the frame (egui blends in
  sRGB space), and samples engine textures through non-sRGB views of them for the same reason,
  which is why textures keep their format without sRGB among their view formats. The window feeds
  it through `egui_winit`, which says which events egui wants, and those presses don't reach
  `Input`; a `Simulation` builds its input from simulated events (`SimulatedInput`) and decides
  the same by where the pointer is (`wants_pointer_at`). `GuiCanvas` renders a `Canvas` into a
  texture egui shows, and `Context::gui_texture` shows a `Texture`. The debug window
  (`WindowConfig::debug_key`, `debug_key` in `conf.luau`) shows the frame stats. Simulations turn
  on egui's AccessKit output, which gives every widget's label and bounds, so tests find widgets
  by label (`Simulation::gui_rect`, `click_gui`).
- `src/audio.rs`: `Audio` (kira's mixer, with separate sound-effect and music tracks) and
  `Sound`. Sound effects are decoded into memory when loaded; music streams from its file and
  loops, crossfading when it changes. Without a sound device, and always headless, audio plays
  to a silent mixer that the engine advances by game time each frame, so sounds still start and
  finish on schedule and tests can check them (`sounds_playing`, `music_playing`).
- `src/input.rs`: `Input` (keyboard, mouse and gamepad state per frame), `Key`, `MouseButton`,
  `Gamepad`, `GamepadButton` and `Stick`. The event loop feeds it winit events, maps the cursor to
  canvas pixels, and clears per-frame presses and releases after each `update`. The system's
  repeats of a held key aren't presses but repeats (`key_repeated`, and `repeated` for actions,
  whose gamepad buttons never repeat); a repeat of a key not held, as after gaining focus, presses
  it. A `Simulation` repeats the last key pressed itself, by a `KeyRepeat`. What keys type is
  `text`, kept until an update has seen it: the window's `KeyEvent::text` (which composes dead
  keys) and input methods' commits, without control characters or the letters of shortcuts
  (Control without Alt, which is AltGr on Windows, or Super). Input methods are off unless the
  game takes one (`set_input_method`, an area of the canvas, which egui's own platform output
  carries to `egui_winit` when egui isn't using one), since they change which key events arrive;
  `composition` is what one is composing. Actions bind to
  `Control`s, keys and gamepad buttons, of any gamepad or one, and have a strength from 0 to 1,
  which `axis` makes an axis of two. Gamepads are numbered from 1, keeping their number while
  disconnected. Their buttons include the sticks' directions (`LeftStickLeft`), pushed as far as
  the stick is that way through a radial dead zone (`dead_zoned`); every button is held from 0.5
  of the way and released below 0.4, so a trigger or stick near the line doesn't flicker.
  `src/gamepads.rs` feeds it from gilrs, polled at the start of each of a window's frames (so a
  window that redraws on demand reads them only in frames that come anyway), and only while it's
  focused; a `Simulation` connects and pushes simulated ones.
- `src/menu.rs`: `Menu`, a selection among `MenuItem`s (a rect and whether it's enabled) that
  games drive: each `update` takes the items as they are now and the `Input`, and returns a
  `MenuEvent` (moved, picked or cancelled). It holds no items, only the selected index and where
  the cursor was, so the mouse selects only as it moves (`track_cursor` notes where it is as a
  menu opens). Directions go by where items are (`neighbor`): those lined up across first, the
  nearest whose middle is past the selected one's far edge, else wrapping to the farthest before
  its near edge, and only then those not lined up, so lists, rows and grids, ragged ones too,
  move as they look. Held keys move again by `Input::repeated`. Its unit tests don't need a GPU.
- `src/graphics/`: `Gpu` (adapter + device), `WindowSurface` (presents to the window),
  `Renderer` (wgpu pipelines), `Screen` (applies the `Resolution`), and the public drawing API:
  `Canvas` (what games draw to each frame), `Camera`, `Resolution`, `Texture`, `Sprite`,
  `SpriteSheet` and `Animation`. An animation doesn't keep time: games keep how long it has played
  and ask it for the frame to draw, so one animation can be shared. Rotated sprites turn about the
  center of their destination, on the CPU as quads are recorded, and transformed sprites and fills
  (`draw_sprite_transformed`, `fill_rect_transformed`) are placed by an `Affine` the same way,
  which is how scenes draw. Sliced sprites (`draw_sprite_sliced`, and `_transformed`) are up to
  nine quads whose edges keep a texel to a unit, shrinking alike where there's no room (`slices`),
  as frames and panels are drawn. Outlines, lines and circles are quads too: an outline four that meet
  without overlapping, so translucent ones are even, a line one, and a circle a polygon with sides
  enough for its size on the canvas (`Polygon`), filled as a fan of triangles two to a quad or
  outlined as a ring of quads. All quads go through one
  textured pipeline; solid fills use a 1x1 white texture,
  and consecutive quads with the same texture are batched into one draw call. Vertices are 20
  bytes, their colors sRGB bytes that the vertex shader converts to linear space, and every quad
  shares one static index buffer, which grows only when a frame has more quads than ever. A fixed
  resolution renders the canvas to an offscreen texture, then draws that texture scaled onto the
  output with the same pipeline. Shaders are WGSL files next to the code that loads them. Sprites
  aren't instanced: rendering is a fifth of the shmup's frame with its bullets in Rust, so it
  isn't what limits it.
- 3D (`camera3d.rs`, `mesh.rs`, `view3d.rs`, `mesh.wgsl`): `Canvas::begin_3d` takes a `View3d`
  (a `Camera3d`, the rect of the canvas it shows on, moved by the 2D camera, and its `Lighting`:
  one sun and ambient light) and returns a `Canvas3d`, which draws `Mesh`es, made from `MeshData`
  with `Context::create_mesh`, placed by an `Affine3`, showing a `Sprite` (its UVs from 0 to 1
  span the sprite's region) times their vertices' colors and a tint, and billboards: sprites
  standing upright, turned across the ground toward the camera (`Camera3d::billboard_axes`),
  drawn as the renderer's own quad mesh (a `MeshDraw` without a mesh), with normals that all
  point up, so they're lit as the ground is. Texels less than half opaque are discarded, and
  nothing blends, so billboards need no sorting. 3D coordinates are right-handed with `y` up, as
  glTF's are.
  The canvas records each view as a `Segment3d` before the batch of quads that follows it, and
  `begin_3d` starts that batch empty, so quads drawn after a view never join a batch drawn before
  it; `push_quad` doesn't know about 3D. `Renderer::render` draws the quads up to each view in a
  pass of their own, then the view in a pass that keeps the color and clears a depth buffer (kept
  for the last few target sizes), with its viewport and scissor cut to the target and its
  projection corrected to match (`Viewport`), so a view's projection is the same however much of
  it shows. Meshes keep their vertices on the GPU (36 bytes each); each draw's transform, normal
  matrix, sprite region and tint go into an instance buffer written each frame, in the order
  they're drawn, and a run of draws of the same mesh showing the same texture the same way round
  (`MeshDraw::joins`) is one instanced draw call, so batches of billboards or of one mesh cost
  little more than one. Mirroring transforms draw with a second pipeline whose front faces wind
  the other way. Billboards' axes are worked out once a view (`Segment3d::billboard_axes`).
  Views place points and cast rays in their rects' coordinates (`View3d::project` and `ray`),
  which `Scene::view`, `project` and `ray` and the editor use. Rendering is at the target's
  resolution, so a fixed
  resolution renders 3D at the canvas's pixels. The `meshes` example uses all of it from Rust,
  and `cellar` from Luau, whose `graphics.begin_3d` records a view and whose `draw_mesh` and
  `draw_billboard` add to the last one while nothing else has been drawn since
  (`Canvas::open_view`), and fail otherwise. Luau's 3D points are its native `vector`s
  (`mlua::Vector`), and views, cameras and lighting are plain tables read each call.
- Models (`model.rs`, `model_file.rs`): a `Model` is `ModelPart`s, each a `Mesh`, a sprite or
  none, an `Affine3` and whether it's double-sided, flattened from a glTF file's default scene by
  `Context::load_model` and drawn with `Canvas3d::draw_model`, a draw call a part (a model of one
  part drawn several times in a row, as a mesh is, takes one for them all). Double-sided
  parts draw with pipelines that cull nothing, and the shader turns the normals of the backs they
  show (`front_facing`). `model_file::read` reads the file without a GPU into `ModelData`
  (unit-tested with glTF the tests write, as JSON with base64 buffers, and the example props):
  primitives that the file's nodes share stay one mesh, a material's base color factor and the
  vertex colors are multiplied and made sRGB into the vertices' colors, missing normals are made
  flat, strips and fans become triangles, `KHR_texture_transform`s move texture coordinates,
  opaque materials drop their textures' alpha, a texture wraps as its sampler says only along the
  axes its mesh's coordinates leave it on (`Wrap`; it clamps along the rest, so edges don't pick
  up the far side's texels), made with a sampler of its own (`Renderer::create_wrapped_texture`),
  and points, lines, JPEG textures and required extensions are errors saying what to export
  instead. It lists the files beside the model it read (`Model::files`), which the editor's
  `AssetCache` checks the times of too. Picking is by boxes, not triangles, since meshes keep
  no vertices on the CPU but for physics: `Bounds3` (`math.rs`) from `MeshData::bounds`, kept by `Mesh` and
  `Model`, `transformed` by an `Affine3`, and `Ray::hit_box`, the distance to where a ray enters
  one. Luau has `graphics.load_model` (cached by path in `runtime/graphics.rs`, as textures are),
  `graphics.draw_model` (`draw_mesh`'s options but `sprite`), `mesh:bounds()` and
  `model:bounds()`, two vectors, and `graphics.hit_box`.
- `src/graphics/font.rs` and `default_font.rs`: `Font` (glyphs packed into one texture, with
  their metrics) and `TextLayout`. `Font::layout` wraps text between words to a width
  (`layout_aligned` also lines its lines up by an `Align`, moving each by whole pixels), and
  `Canvas::draw_text_layout` draws a range of its glyphs, which is how dialogue reveals text
  without words jumping lines. The built-in pixel font is authored in `default_font.rs` as rows of
  `#` and `.`, and built into a texture when the `Context` is created. Layout logic is separate
  from the texture (`Metrics`), so its unit tests don't need a GPU.
- `src/tilemap.rs`: `Tilemap`, a grid of cells in layers, built from a `Tileset` of `Tile`s
  (a sprite, a tint, and whether it's solid). Layers draw in order, culled to what the canvas
  shows, and `draw_layer` lets games draw characters between them. `is_solid` is the collision
  check: a cell is solid if a tile on any layer is, and everything outside the map is solid. Its
  unit tests create a texture, so like the screenshot tests they need a GPU adapter.
- `src/grid.rs`: `Direction` and `GridMover`, for characters that walk tile by tile. Each frame a
  game calls `update`, then `step` when the mover is idle; a step carries over time left from the
  one before, so continuous walking keeps an even pace.
- `src/physics.rs`: `World`, collision by Rapier (the `physics` feature): fixed colliders without
  bodies, each a `Rect` placed by an `Affine` as `Scene::placement` places a node's, solid all over
  or only along its edges (`Solid`), and `slide`, which moves a box by up to a motion, stopping a
  hundredth of a unit (`GAP`) short of what's solid and sliding along it. It casts the box through
  Rapier's queries itself, not with Rapier's `KinematicCharacterController`, which is for
  characters on floors and keeps its gap by rounding the box, so the normals it slides along come
  back a little turned and a box pushed at a wall creeps along it; and it snaps the normals GJK
  finds to the sides of the box or of what it hit, within a hair (`true_normal`). Rapier catches
  up with changed colliders (`detect_collisions`) before the next slide. Its unit tests don't need
  a GPU. Luau's `physics.world()` returns a `lua::PhysicsWorld`, colliders of one scene's nodes,
  which before each move re-reads every node's placement to follow it and drops those of removed
  nodes, and whose `move` slides a box around a node's position, without the node's own collider
  in the way, then moves the node within its parent.
- `src/physics/world3d.rs`: `World3d`, physics in 3D views by Rapier: bodies (`Body3d`), fixed,
  dynamic or kinematic (`BodyKind`, kinematic ones never asleep, since Rapier doesn't wake one
  that dynamic bodies bump into and leave), each a `Shape3d` (triangles, which only fixed bodies
  have, a hull, a box, a ball or a character's upright cylinder) placed by a `Pose3d`, a position
  and yaw, pitch and roll as nodes have, turned to and from Rapier's quaternions with `libm`.
  `step` takes steps longer than a sixtieth of a second in equal parts, putting kinematic bodies
  an equal part of the way along in each (Rapier would move one all the way in the first, then
  hold it still). `walk` walks a `Character`, an upright cylinder with flat feet (a capsule's
  rounded bottom slides off stair edges), by casts of its own as the 2D `slide` does: on the
  ground (just below it, or as far as what's below sank by its velocity over the last step,
  `sank`, which it follows down, as on a lift), along it first, up slopes no steeper than its
  `slope` and along walls and steeper slopes as though upright, and only if that hits something,
  over it as well (up a `step`, across, and down onto something to stand on), keeping whichever
  went farther; then down to the ground by up to a step, or falling off a ledge; off the ground,
  or rising, as it's told, landing on what it can stand on. It pushes the dynamic
  bodies it walks into at the height of their centers of mass, as a collision of its mass and
  theirs would, at the speed it walked over the last step's time. Its `true_normal` snaps GJK's
  normals, off by up to a hundredth of a radian, to a face of what it hit within a few degrees
  (the triangles near where it touched, from either side, a cuboid's sides, or a hull's face), or
  to the cylinder's flat ends or its side. Rapier's `detect_collisions` would catch up with
  bodies added or moved since the last step, but it forgets new bodies as the step that follows
  looks for them, so they're never simulated: `refresh` puts those colliders in the query tree
  itself (`set_aabb`). Queries refresh first too: `cast_ray` (a `RayHit`, by Rapier's ray casts,
  solid, its normal turned to face back along the ray), `overlapping` (a `Shape3d` in the world,
  made by `collider`) and `inside` (what overlaps a body), never finding sensors (`Body3d::sensor`),
  whose colliders are Rapier's sensors with no active collision types, so steps leave them alone
  and nothing bumps into them; what's in one is asked for on demand. Sensors are fixed or
  kinematic and not triangles, and `set_pose` puts a kinematic one at once, since it pushes
  nothing. Its unit tests, which pin a hash of a pile of bodies and a walk so CI checks
  it's the same on Windows and Linux, don't need a GPU. Luau's `physics.world_3d()` returns a
  `lua::PhysicsWorld3d` (`src/lua/physics.rs`, with the 2D world), bodies of one view's nodes,
  shaped like the mesh or model they show (`geometry`, a model's parts placed in it): fixed ones
  baked through the node's whole transform, composed with `libm`'s sines (`exact_transform`), and
  those that move placed by their node's position and turn, stretched by its scale, inside nodes
  that only move them (`parent_origin`). Before each walk and step it follows nodes scripts moved
  (comparing their placements, and for bodies that move, their parents' origins, to what it last
  read or wrote) and drops those removed: a character's body goes where it walked over the next
  step, pushing what's in its way, and anywhere else a script puts its node, at once
  (`World3d::put`, `Bodied::walked`). After each step it moves dynamic bodies' nodes to them,
  leaving those at rest. Its queries, `ray`, `overlap_ball`, `overlap_box` and a sensor's
  `inside`, follow the nodes first (`followed`) and give back nodes in the order they were added
  (`nodes`), skipping those their `except` option names; a 3D view node's `ray` (`Scene::ray`)
  starts one where the mouse is.
- `src/headless.rs`: `Headless`, which renders a `Canvas` to an in-memory image without a window,
  and `Simulation`, which runs a whole `Game` without a window: fixed 60 fps frames, simulated
  keys, mouse and gamepads, and screenshots on demand. Held keys repeat as a system repeats them,
  only the last pressed, after a delay and at a rate in game time (`set_key_repeat`, by default
  half a second, then 30 a second), so repeats land on the same frames every run, and keys type
  as on a US keyboard (`Key::us_text`); `type_text` types any text, and `compose` composes as an
  input method would. Actions are pressed by name (`action_down`, `tap_action`, `hold_action`),
  through their first key, or without one, their first gamepad button, so tests needn't copy a
  game's bindings. A `Simulation` of a `LuauGame` runs tests' Luau between frames (`exec`, `eval`,
  and `try_` versions returning the error), each a chunk of its own named `test`, lent the context
  as `update` is (`LuauGame::run_chunk`); the runtime's `Requirer` has chunks that aren't files
  (named `=...`) `require` as `main.luau` does, so they get the game's modules. Its frames render
  only for screenshots, unless
  `set_render_every_frame` asks for every frame to render and wait for the GPU, as benchmarks
  want. All of them in a process share one GPU device, and
  only one thread at a time may hold any (a reentrant lease): lavapipe crashes with a device per
  thread and WARP renders wrongly with a shared one, so parallel GPU tests take turns. Simulated
  input reaches egui too, typing as well, with `scroll` for it alone; screenshots show the
  interface; `request_close` presses the window's close button and `has_quit` tells whether the
  game called `Context::quit`. Each keeps its game's saves in a temporary folder of its own,
  unless its config gives one. Simulations run every frame they're asked to, but
  `next_frame_in` tells when a game that redraws on demand would want its next (input wants one
  at once), so tests check it waits while nothing changes. Frames are `Image`s, which games'
  tests compare as the screenshot tests do: `Image::compare` counts the pixels that differ by more
  than a per-channel tolerance (a `Comparison`), `diff` shows them in magenta, and `load_png`
  reads a reference back as 8-bit RGBA, whatever the file holds. Its unit tests of them don't
  need a GPU.
- `src/lua.rs`: `Scripts`, a sandboxed Luau VM for gameplay scripts, and its task scheduler.
  Games register Rust functions (`set_function`), load script files, and start script functions
  as tasks (`spawn`): coroutines that `update` resumes once a frame with the frame's duration.
  The built-ins `wait`, `wait_until`, `spawn` and `print` are documented at the top of the file,
  with `exact`, a table of math functions from the `libm` crate, which give the same bits on
  every platform, unlike `math`'s from the C library (Windows' and Linux's differ). They can't
  replace `math`'s: Luau calls those as builtins without looking them up, which also makes them
  about five times quicker, so games call `exact`'s for what must replay and `math`'s for show.
  `rect` (`overlaps` and `contains`, edge for edge as `Rect`'s) and `approach` are built-ins too,
  written in Luau in the prelude with `wait` and `wait_until`, so calling them doesn't leave the
  VM. Scripts compile at Luau's optimization level 2, whose inlining halves calls between Luau
  functions. Its unit tests don't need a GPU.
- `src/runtime/`: games written entirely in Luau, run from a folder holding `main.luau` and an
  optional `conf.luau` of settings: the window's, the update rate, the identity that names its
  save folder, and the native libraries to load, any of them optional (skipped if there's no such
  library, but an error if it's there and fails to load). `LuauGame`
  is the `Game` that runs one, and the engine's API for it is the global tables `graphics`,
  `input`, `audio`, `tilemap`, `grid`, `physics`, `scene`, `ui`, `save` and `window`, one file
  each. `ui.menu` hands out a `Menu` (`ui.rs`'s `ScriptMenu`), which holds the `Lent` to read
  input, so it's documented with its table rather than in `src/lua.rs`: a `menu::Menu` over a
  node's children or tagged descendants, found again each call (those shown, at
  `Scene::extent`), keeping its selection by `NodeId` as they come and go. The close
  button asks the game's global `close_requested`, lent the context as `update` is, and closes
  unless it returns false; a game stopped by an error always closes. `LuauGame::reload` loads the
  game again in its place, a whole new `LuauGame` (VM, caches and libraries' state, made before
  the old one drops), after calling the old one's global `reloading`, lent the context, for plain
  data (`scene::plain`) the new one's `window.reloaded()` returns, and resetting what the game set
  in the `Context` (`Audio::reset`, `Input::clear_bindings`); it rereads `conf.luau` for its
  libraries, keeping the window's settings as they were. One that fails to load becomes a stopped
  game with an empty VM (`LuauGame::stopped`), holding the kept data for the next reload.
  `load_watching` (`run_watching`, `wingine --watch`) starts one stopped rather than failing, and
  reloads it in `update` when its `Watcher` sees the folder change, before the stopped game's
  early return; tests call `Simulation::reload`. The module docs are the
  reference for script authors; the engine types those tables hand out (sprites, animations,
  meshes, tilemaps, grid movers, text layouts, scene nodes, physics worlds) are userdata, documented with their
  methods in `src/lua.rs`. Sprite sheets, scenes and `graphics.load_model` share one cache of
  images and models (`Textures`), so each is read once. `wingine.d.luau` (`runtime::DEFINITIONS`) declares that API's types, with short docs, as
  a definitions file for luau-lsp (`declare extern type ... with` for userdata, `declare name: {}`
  for tables; methods can't be generic there, so generic ones are properties of generic function
  types); `tests/runtime.rs` checks it declares exactly the functions the tables hold, and
  the editor's tests that the examples type-check against it. A game's global `gui(ui)` lays out
  an egui interface (`gui.rs`, with the `egui` feature): `LuauGame::gui` lends the context and
  calls it with egui's `Ui` as userdata borrowed for the call through `Lua::scope`
  (`create_any_userdata_ref_mut`), whose methods are registered once with
  `register_userdata_type`. Containers call their function with the inner `Ui` in a nested scope,
  so a `Ui` kept past its call fails rather than dangle. Widgets that change a value take it and
  return it; `gui.rs`'s unit test checks `wingine.d.luau` declares exactly `Ui`'s methods.
- `src/scene/`: scenes. `SceneFile` is what a scene file holds (`NodeDef`s, `SheetDef`s and props
  of `Value`s), and `TilesetFile` a tileset file: `file.rs` reads one by running it as Luau in an
  empty environment, turning what it returns into plain `Value`s, then reading those, with errors
  naming the node's path; `writer.rs` writes either in the canonical layout, and their `write`
  puts it in a file of its own, then renames that over the old one, as saves are. The module docs
  are the formats' reference. `Scene::new` builds a scene in two passes: `Expand` replaces instances
  with the nodes of the files they place, then makes their overrides by turning the node back
  into `Value`s (`node_fields`), setting the fields and reading it again, so overrides are
  checked as files are (`NodeDef::overridden` does the same for tools, and `NodeDef::fields`
  gives the values); sprites then name sheets `file#sheet`, since nodes from many files are
  together. `Build` then makes the live nodes, loading sheets, tilesets and model files and
  making the meshes of shapes through `Assets` (`FolderAssets` reads a folder; the runtime caches
  files), and sharing a mesh between nodes of one shape. `Scene` keeps nodes in slots with
  generations, so `NodeId`s of removed nodes never reach nodes added since; each node keeps its
  local `Affine` up to date as it moves. A tilemap node holds a `Tilemap` whose layers are its
  `tile_layer` children, in order, which draw as children, so other nodes draw between layers.
  Drawing walks the tree, composing transforms from parent to child; text and tilemaps only move.
  3D views (format 2) are `view3d` nodes, laid out as areas, whose children are `mesh`, `model`,
  `billboard` and plain nodes placed in 3D: `NodeDef::placement3` (`z`, `yaw`, `pitch`, `roll`,
  `scale_z`, with `x`, `y`, `scale_x` and `scale_y`) is `Some` for them, which the reader decides
  from the node's kind, its parent, or its fields, so a prop's own file, placed by an instance,
  can hold them at its top; `check_3d` checks where 3D kinds are once instances have expanded.
  Live nodes keep a local `Affine3` (`Spatial`); drawing a view begins it on the canvas
  (`begin_3d`) and draws its subtree in it (`draw_3d`), and `transform_3d`, `bounds_3d`, `ray`
  and `pick` (nearest box first) work in its world.
  Layout is worked out as the tree is walked, from the root down (`Placed`): a node's anchor
  places it in its parent's rectangle, and its stretch sizes it by it, where the root's is the
  scene's `size` (the canvas's: drawing sets it, and `scene.load` starts it there;
  `draw_with_size` draws it laid out for another canvas, as the editor's viewport does) and a plain
  node passes its parent's on. So `transform`, `bounds` and `contains` walk down from the root,
  as does `extent`, the bounds of a node and what's shown inside it, where menus' items are.
  Stacks and panels (format 3; a panel is a stack that draws a fill and border or a sliced
  sprite) place their children themselves (`lay_out_stack`): one after another along their
  `direction` with a `gap`, inside their `padding`, each taking the space it covers as drawn,
  lined up across by its anchor's fraction, nudged by its `x` and `y`, and hidden ones taking
  none; a plain node there has its origin at the start of its space and takes what its children
  reach past it. Along an axis without a `width` or `height` a stack fits what it holds, which
  the scene measures (`Scene::rect_in`, `size_of`; `Node::size` is `None` for them), laying the
  children out in no space that way, and its pivot is from the point its anchor names, so an
  anchored stack keeps that point in place as it grows. Their shared fields are `Stack`, the same
  in files and live scenes.
  `clip.rs` holds `Clip`s, a file's tracks of keyframes on nodes' properties, which a file names
  apart from its nodes, so a clip poses whatever node it's given: `Scene::pose` finds every
  track's node first, failing before changing anything, then samples each, moving numbers by its
  weight (`Keys::value_at` samples one track, as tools do); nodes in views have 3D properties
  and no `rotation`. Sprite, billboard and mesh nodes keep the sheet their file gave them, so cell
  tracks and `set_cell` pick from it. Scripts hold nodes as `lua::SceneNode`,
  an `Rc<RefCell<Scene>>` and a `NodeId`, so
  every node of a scene shares it, and clips as `Clip` userdata, copied from the cached file. Live
  scenes' tests are in `tests/scene.rs` and need a GPU; the formats' are unit tests that don't.
- `src/runtime/native.rs`: native libraries, compiled Rust that adds a global table of functions
  and classes to a Luau game, and hooks before and after its `update` and `draw`. Each is
  registered once per process and never unloaded, and each game that loads one gets the
  library's state of its own. Games load them from their `native` folder; tests link them in
  with `runtime::link_library`. The engine's side of the ABI is the `HOST` table of functions.
- `crates/wingine-abi/`: the C ABI between the engine and native libraries: `#[repr(C)]` types,
  the library's descriptor and the engine's `Host` table, versioned by `ABI_MAJOR` and
  `ABI_MINOR` (0.3, which added 3D: meshes, vectors, `begin_3d`, and batched `draw_meshes` and
  `draw_billboards` into the view begun last, whoever began it). Its crate docs give the rules
  both sides follow.
- `crates/wingine-native/`: the crate native libraries are written with, which wraps the ABI:
  the `Library` and `Class` traits, a `Registry` for functions, methods and hooks, argument and
  result conversions (`Mesh` and `Vec3` among them), the 3D calls on `Canvas` and
  `Context::create_mesh`, and `export!`. Libraries depend on it alone, never on `wingine`.
  Models stay out of the ABI.
- `src/main.rs`: the `wingine` executable, which runs a Luau game: `cargo run -- examples/pong`,
  or with `--watch`, reloading it as its files change; `wingine test [--update] <folder>
  [filter]` runs its tests, printing them as `cargo test` does; `wingine export <folder> <out>`
  exports it; and with no arguments, it runs the `game` folder beside it
  (`runtime::shipped_game`) with `runtime::run_shipped`, which shows errors loading the game, or
  reading its `conf.luau` (in a window of the default settings), in its window
  (`LuauGame::load_showing_errors`, which `load_watching` uses too).
- `src/runtime/export.rs`: `runtime::export`, a folder to ship: the given executable (the CLI's
  own) copied as the game's identity, beside the game's folder copied as `game/` without hidden
  entries or `*.test.luau` (skipping the output folder if it's inside), and `licenses/` (the
  engine's two, built in with `include_str!`, and `THIRD-PARTY-NOTICES.txt` if it's beside the
  executable). On Windows the copy's PE header is patched to the GUI subsystem
  (`gui_subsystem`), so it opens no console. It refuses a game with no `main.luau` or settings that
  don't read, and an output folder that isn't empty, and warns of a debug build, missing notices
  and native libraries the game needs that aren't built. Its unit tests export a game with the
  test executable; `tests/export.rs` runs the exported executable end to end.
- `src/runtime/testing.rs`: `runtime::testing`, Luau games' own tests, in `*.test.luau` files
  anywhere in their folders (not hidden ones), whose module docs are the reference for writing
  them. `run` runs each test against a fresh `Simulation` of the game (the first one loaded also
  finds the file's tests; each after loads the game and the file again, checking the names come
  out the same), reporting `Outcome`s as they come and a `Report`. A test file loads in the game's
  VM with an environment of its own (`environment`): a table whose metatable reads and writes
  through to the game's globals, and which, not being Luau's "safe" environment, makes the file's
  global field reads look up each time rather than as imports resolved at load, so tests see the
  game's state as it is. The test-only globals are raw fields of it: `test`, a Rust function, and
  `sim`, `assert_equal` and `assert_near` from the Luau `PRELUDE`, where each of `sim`'s functions
  (generated from `REQUESTS`) yields its request; the `Runner` resumes the test's coroutine lent
  the context, carries requests out on the `Simulation` while it waits, and resumes it with `true`
  and the results or `false` and a message the wrapper raises at the test's line. A bare yield,
  as `wait` makes, runs a frame. Frames go one at a time, checking the game hasn't stopped and the
  test's time limit (`Refusal::Fail`). Requests that would panic in Rust (an unbound action, a
  gamepad not connected, a widget not there) are checked first (`Simulation::try_action_down`).
  `matches_screenshot` compares with `Image::compare`, saving frames that don't match, and diffs,
  in the game's `.test-output` folder, cleared as a run starts, or with `update_screenshots`,
  over the reference. Failures name paths from the game's folder and leave out the prelude's
  lines (`tidy`). Its unit tests check the definitions declare `sim`'s fields.
- `src/watch.rs`: `Watcher`, a thread that hashes the paths, sizes and modified times of a
  folder's files that aren't hidden, eight folders deep, every so often, and sets a flag and wakes
  the game when the hash changes. Its first hash is taken before the thread starts, so a change
  right after counts. Games watching their folders and the editor use it; not on wasm32.
- `crates/wingine-editor/`: the editor, a library and the `wingine-editor` executable
  (`cargo run -p wingine-editor -- examples/garden`). `Editor` is a `Game` whose `gui` is the whole
  interface, laid out like Godot's: the top bar (menus, the main screens, Play), the Scene dock
  (`docks/scene_tree.rs`) over the FileSystem dock (`docks/filesystem.rs`) in a left panel, the
  Inspector (`docks/inspector.rs`) on the right, Output (`docks/output.rs`), Tiles and Animation
  in a collapsible bottom panel, and the open scenes' tabs over the viewport (`viewport.rs`) in the
  middle. It
  edits `SceneFile`s, never live scenes: a `Document` (`document.rs`) holds the file, its undo
  history as snapshots of the whole file (a change runs from the first `edit()` until a frame with
  no button held and no field focused, so a drag or a field's typing is one), the selection as a
  `NodePath` (indices into children, `edit.rs`, whose pure operations add, remove, duplicate,
  rename and move nodes, keeping names unique and clips' tracks pointed at their nodes, and paint
  tile layers' cells, keeping their legends to the characters used), and a
  preview `Scene`, built again from the file whenever it changes, through `AssetCache`
  (`project.rs`), which rereads a file only when its modified time changes; painting changes the
  preview's tiles alike instead (`Document::edit_with_preview`), so strokes don't rebuild it. A
  selection can be a node inside an instance, by its path of names (`Document::select_inner`):
  the Scene dock lists an instance's nodes from the preview, `instance.rs` finds the node in the
  file it's from, through nested instances and their overrides, and the Inspector shows it with
  the instance's override made, writing back as the override whatever differs from the scene's
  node. Docks return actions
  that the editor applies, each a change of its own; the Inspector edits a copy of the node and
  puts it back only if it changed. The viewport renders the preview into a `GuiCanvas` with
  `Scene::draw_with_size` at the game's canvas size (from `conf.luau`), mapping between the
  scene and the window as `Camera` does (`ViewMap`), and picks what can be seen before areas,
  plain nodes' origins and tilemaps, clicking again in place reaching the next node down. With a
  tile layer selected, it paints instead, by the Tiles panel's tool (`docks/tiles.rs`, in the
  bottom panel, with a `Palette` of the tileset's tiles), and a selected tilemap has handles that
  resize it. Scene files dragged from the FileSystem dock carry a `ScenePath`, which the viewport
  and the Scene dock take as a drop. For screens, the toolbar's Anchors menu applies Godot's
  anchor presets (`layout.rs`, pure functions of the node and its drawn size: an anchor, a pivot
  at the same point of the node, and its size and stretch), the Select tool resizes sprites,
  rects and areas by handles on their edges, and the toolbar's canvas size, or dragging the
  canvas's corner, previews scenes laid out on canvases of other sizes (`Viewport::preview`),
  which never changes the file. Stacks and panels take presets by their anchors alone, with no
  pivot, keeping on fitting what they hold unless stretched across; resizing one by a handle
  gives it a size of its own that way, its origin making up for its pivot moving with its size. In 3D views, seen through their own cameras (the editor has
  none), the viewport picks by boxes (`Scene::pick`, a view's nodes before the view), outlines the
  selection's box as the camera projects it, and drags with the Move tool across the ground at
  the node's height, or with Shift, an upright plane facing the camera (`Drag3d`); model files
  dragged from the FileSystem dock carry a `ModelPath`, and they and scene files dropped in a view
  are placed where the ray meets the ground. Instances are added only of scenes whose nodes fit
  where they go, through the instances they place (`edit::check_placed`, the rules `check_3d`
  applies once the scene is built). `edit::add` fits nodes to where they go
  (`place_for`), the Scene dock offers `KINDS_3D` inside views, the Inspector shows
  `transform_3d` and sections for views, meshes, models and billboards (a node an instance places
  is shown in 3D when the preview has it there, `Document::is_in_3d`), and `AssetCache` rereads
  model files as it does images, keeping the latest meshes of shapes. For rigs, the Animation panel (`docks/animation.rs`) shows one of
  the file's clips on a timeline (`Document::timeline`, the clip and the playhead), whose tracks'
  keys it moves and removes through `clips.rs`, pure operations on clips that keep keys in order
  and a key to a time. While the panel shows, the clip poses the preview at the playhead
  (`Document::posing`; the preview is built again when the clip changes or stops posing), and
  nodes are read and changed through `Document::shown_node` and `set_node`: as they're posed, with
  changes to properties the clip tracks keyed at the playhead and the rest made to the node, so
  the Inspector's fields and the viewport's tools key what the clip animates. The Inspector's ♦
  buttons key a property, adding a track (`Document::key`). The Script screen
  (`script_editor.rs`, `ScriptEditor`) edits `Script`s (`script.rs`: the text with `\n` line
  endings, kept as the file had them when saved, and the language server's diagnostics) in an
  egui `TextEdit` whose layouter colors `luau.rs`'s tokens and underlines problems, with line
  numbers painted over the scroll area's left edge so they stay put scrolling sideways. It takes
  Tab and the keys of the completion list before the field sees them, and indents new lines after
  the field puts them in, so typing in the same frame keeps its order. `lsp.rs` runs luau-lsp in a
  process of its own with the engine's definitions and a base `.luaurc`
  (`lsp::support_files`, written to a temporary folder), threads writing and reading its framed
  JSON, and `LanguageServer::poll` turning what it says into `Event`s; scripts are sent to it
  whole each time they change (`send_changes`, before any request about them), and positions
  convert between characters and UTF-16 columns. Without luau-lsp, scripts are edited unchecked.
  Play
  (`play.rs`) runs the editor's own executable with `--run`, which runs the game watching its
  folder (`runtime::run_watching`), so it's a process of its own that reloads as the editor
  saves; tests point it at the built executable with `set_runner`. The editor
  redraws on demand: threads that read the game played and luau-lsp wake it with a `Waker` when
  they have something, as does the engine's `Watcher` (`project::watch`), looking every second,
  so the editor reads the folder again only when it's changed;
  the Animation panel asks for frames while a clip plays. Its tests
  (`tests/editor.rs`) drive it through `Simulation`, clicking widgets by label, and copy example
  games to change them; those of luau-lsp, Play and the watcher wait in real time, running frames
  only when the editor wants them, as its window would (`wait_for`), so they fail if nothing wakes
  it, and `settle` checks it stops wanting frames. Number fields give their name to AccessKit
  themselves, since egui doesn't carry a label's text over to what it labels.
- `examples/rpg/`: a top-down RPG, written entirely in Luau. Maps in `maps/` are scenes: a
  tilemap over `world.tileset.luau` whose layers hold each cell's kind of ground (`floor`), the
  ground's sprite (`ground`) and objects (`objects`), with tiles named for their kind and sprite
  (`tree.2`), and nodes tagged `place`, `exit`, `character` and `foe` on the cells they're about,
  whose props say the rest. `world.luau` builds levels from them, and holds each kind's
  descriptions and the tiles it varies between, picked by a deterministic noise, so changing a
  cell at runtime picks what the map would have; examining any cell shows its description.
  `story.luau` holds conversations and events, which maps refer to by name, and
  `dialogue.luau` the dialogue box, whose `say` and `ask` wait for the player. `battle.luau` runs
  one-on-one, turn-based battles against the creatures in `foes.luau`, which maps place and
  which fight when the player stands beside them; its messages and menus go through the dialogue
  box, and tests make its dice predictable by replacing `battle.roll`. `items.luau` holds things
  to use and gear, whose bonuses `items.stats` adds to the player's; `shop.luau` buys and sells
  them, and `bag.luau` (the menu key) equips gear and uses items. `look.luau` holds the panels'
  shared look (named so it doesn't hide the engine's `ui`). `dialogue.ask` lays its choices out in
  `dialogue.scene.luau`, a panel of rows copied from hidden ones, details in a column beside them
  and the highlighted choice's note to their left, which a `ui.menu` made once the text is
  revealed picks from; the box itself and its speaker are drawn in code, revealing the text glyph
  by glyph. `story.reached(stage)` compares quest stages, which run in the order of
  `story.STAGES`. F1 shows `debug_panel.luau`, an egui window from the game's `gui` that sets hit
  points, coins, items and the quest's stage, and jumps between maps (`ARRIVALS` in `main.luau`).
  Its panels show `frame.png`, a 5x5 frame with cut corners, sliced: `look.panel` draws it with
  `graphics.draw_sprite_sliced`, and the panels of `title.scene.luau` (the title's name and menu),
  `dialogue.scene.luau` and `bag.scene.luau` (the bag's stats) show it, fitting rows that
  `title.luau`, `dialogue.luau` and `bag.luau` copy from a hidden one with `node:add`. The game
  opens on `title.luau`, a title screen whose woods are drawn in code, whose menu is a `ui.menu`
  of its panel's rows, and whose Continue (offered when there's a save) restores the save
  `progress`, which `progress.luau` writes after every event
  (`run_event`) once the title has closed: the player's state, where they are, and each visited
  level's beaten foes and found places (by cell). Continuing builds those levels afresh, marks
  them, and `story.restore` makes again what reaching the quest's stage changed, such as the open
  gate; change it with the story's events. New Game over a save asks first. The game's state is
  the global `rpg`, which its `.luaurc` names for luau-lsp, as the shmup's
  names `shmup` and `danmaku`.
- `examples/shmup/`: a Touhou-style bullet hell, written entirely in Luau, at a fixed 60 updates
  a second. Its enemy bullets go through one interface, documented at the top of
  `bullets.luau`, which is the Luau implementation (parallel arrays of numbers, which fit nearly
  three times as many bullets as a table per bullet) and which the `danmaku` library implements
  too; `main.luau` lists the systems there are, registers the bullet kinds
  of `art.luau` with each in the same order, and `shmup.bullets` is the one running. Switching
  hands every bullet to the next system through `bullet` and `add`, so they carry on exactly as
  they would have (the stress test starts again instead). Bullets move
  by parameters alone, so a game's bullets depend only on its inputs and `shmup.seed`: nothing
  else may take numbers from `math.random`. Enemies, the stage and the boss's attacks are
  coroutines the game resumes each update with `dt`, so the engine's `wait` works in them
  (`enemies.resume`, `enemies.frame`). `boss.luau` holds the spell cards, `stage.luau` the waves,
  `stress.luau` the stress test, which prints the limit it finds for runs without a window, and
  `hud.luau` the panel and screens, laid out in `hud.scene.luau` (in canvas pixels, so
  `hud.menu`, a `ui.menu` of the title menu's items, areas in a stack, takes the mouse too; the
  title shows from the start, so the menu has its items before the first draw; the panel's rows
  are stacks,
  the play's hidden to show the stress test's), which it fills in from the game's state and draws
  each frame; `shmup.hud` is the scene's root, for tests, which check the file
  stays in the canonical layout. `shmup.start("boss")` is
  boss practice, and `shmup.cheats` (`invincible`, `damage`) are for tests. The game's state is
  the global `shmup`.
- `examples/native-danmaku/`: the shmup's bullets as a native library, `danmaku`, a `cdylib` the
  game loads if it's in `examples/shmup/native/` (which git ignores). Its arithmetic follows
  `bullets.luau` step for step in `f64`, so the two produce the same bullets to the bit: change
  them together. It draws bullets in their order, with a `draw_sprites` call for each run of one
  kind, so both draw the same pixels.
- `src/script.rs`: scripted runs, which drive a game with simulated input from a text file
  (unrelated to Luau scripts), built on `Simulation`. Their `benchmark` command renders every
  frame for a while and reports how long frames took. A run can start somewhere: `saves`, first,
  copies a folder's saves into a `TemporaryFolder` the run uses, and `click-gui` clicks egui
  widgets by label (`Simulation::click_gui`), such as a debug window's.
- `examples/garden/`: a garden around a windmill, laid out in `garden.scene.luau`, which
  `main.luau` loads and plays: it turns the mill's sails and a flock of birds (nodes whose
  children turn with them), walks the player, which waddles and mirrors about its feet, in a
  physics world (`garden.world`) of the nodes tagged `solid` and the ground's edges, which a strip
  across its feet (`FEET`) slides along, and shows the `text` of areas tagged `note`. Its trees
  and flowers are
  instances of `tree.scene.luau` and `flower.scene.luau`, changed by overrides, and Space plants
  a flower (`add`) or picks one (`remove`). F1 draws a debug view over it (`garden.debug`):
  outlines of the solid nodes and note areas, the player's feet, and the turning nodes' circles.
  The scene files are in the canonical layout, as an
  editor would write them, and `tests/garden.rs` checks they stay so: after editing one, write it
  again with `SceneFile::write`. The game's state is the global `garden`.
- `examples/orchard/`: a rigged character, a picker with a basket on their head who catches
  falling apples. `picker.scene.luau` is the rig, sprites of its parts from
  `orchard.png` turning about their joints, and its clips: `idle` has a track for everything the
  others change, so `main.luau` poses with it first each frame, then `walk` weighted by
  `orchard.stride`, `blink`, and the reactions (`catch`, `miss`) faded in and out, or `slump`
  once the day is over. Its near arm hangs from `body_front`, a node the clips turn as they turn
  `body`, so it draws in front of the near leg. `orchard.scene.luau` places the picker as an
  instance, and the apples are copies of a hidden node, caught against the bounds of the
  basket's `mouth` area as it's posed. The orchard draws through a camera (`orchard.camera`) and
  the HUD over it without one; once the day is over, the camera eases in on the picker to a zoom
  of 2. `orchard.grow(x)` starts an apple growing, and tests turn
  `orchard.growing` off so only theirs grow. The scene files are in the canonical layout, and
  `tests/orchard.rs` checks they stay so. The game's state is the global `orchard`.
- `examples/cellar/`: a 2.5D game in a 3D view: the hero walks a cellar tile by tile with a
  `GridMover` and lights its candles by standing on them. Its level is `cellar.scene.luau`, a
  view filling the canvas, which the editor edits: a floor (a plane), walls (low cuboids, so the
  hero shows over them) tagged `solid`, barrels, crates and a table, models from its `models`
  folder (copies of `examples/models`) tagged `solid` and `prop`, candles tagged `candle`, toward
  the back of their tiles so the hero standing on one isn't in the same place, and the hero, the
  last two billboards. `main.luau` finds each node's tile from its position (a tile is a world
  unit), moves the hero's node and mirrors it, and the mouse names the prop it points at, the
  nearest whose box is under it (`view:pick`, `cellar.pointed`). The camera follows the hero from
  above and behind (`cellar.view()`, set on the view each frame), and `view:project` places the
  message over their head once every candle is lit. The scene file is in the canonical layout,
  and `tests/cellar.rs` checks it stays so. The game's state is the global `cellar`, its view
  `cellar.world`.
- `examples/course/`: a course in 3D with physics, written entirely in Luau, at a fixed 60
  updates a second so its steps replay. `course.scene.luau` is a view of models from Kenney's
  Prototype Kit, copied into its `models` folder with their colormap texture and license: a
  ground, low walls, a platform, stairs and a slope up to it, and a block, tagged `solid` and made
  fixed bodies by their triangles; crates tagged `crate`, dynamic boxes, on the platform's edge
  and stacked; coins tagged `coin`; and the player, a figurine, a character. `main.luau` keeps how
  fast the player is rising (`course.rising`, with gravity of its own, quicker than the
  crates'), walks it with `world:walk`, jumping only from the ground, faces it the way it walks
  (`exact.atan2`) and waddles it, and starts it again when it falls off into `fall`, a hidden
  box under the course made a sensor, which takes away crates that fall off too. A ray straight
  down from the player's middle puts `shadow`, a dark sphere squashed flat, on whatever is below,
  tilted to it, so a jump shows where it lands. The coins aren't bodies: they spin and bob, and
  the player picks one up by coming near it, into `world:overlap_ball` around it. The camera
  follows the player from above and behind (`course.camera()`). The scene
  file is in the canonical layout, and `tests/course.rs` checks it stays so. The game's state is
  the global `course`, its physics world `course.world`.
- `src/math.rs`: geometry types: `Rect`, `Affine` and `Affine3`, 2D and 3D transforms that
  compose with `*`, and `Bounds3`, a box in 3D.
- `examples/`: small runnable programs that exercise engine features, plus their assets.
  `sprites` has tests (enabled with `test = true` in `Cargo.toml`) that drive it with
  `Simulation`, and its events and NPC are scripted in `sprites.luau`. `pong/` is a game written
  entirely in Luau, with its computer opponent in `opponent.luau`, played with the keys, the mouse
  or a gamepad, whose stick moves the paddle as fast as it's pushed (`input.axis`). Nothing in it
  is random, and its ball's angles come from `exact`, so the same inputs play the same match on
  every platform. Its `pong` table is plain data, which its `reloading` returns and the reloaded
  game copies back over a fresh one, so the match carries on as it reloads. Its own tests are in
  Luau, `pong.test.luau`, one of them against `screenshots/ready.png`, and its `.luaurc` names
  `pong` so luau-lsp checks them. The sounds and music in
  `examples/sounds/` were synthesized for `sprites`; `pong`, `rpg` and `shmup` have copies of
  the ones they use, and `rpg`, `shmup`, `garden` and `cellar` copies of the tileset.
  `kenney_prototype-kit/` is Kenney's Prototype Kit (CC0), models for 3D examples, which copy the
  ones they use, as `course` does.
  `shmup/bullets.png`
  was drawn pixel by pixel for it: pellets, orbs and rice in eight colors, the hitbox, and the
  player's shot. `orchard/orchard.png` was drawn pixel by pixel for it too: the picker's parts and
  faces and their basket in 16x16 cells, apples, and clumps of leaves, a cloud and a bush in 32x32
  cells below them, which the scenes divide with two sheets of the one image. `rpg/strides.png` is
  another copy with its people and creatures mid-stride (raised a pixel on stretched legs), the
  frame characters show for the first half of each step. `models/` holds props made in Blender for
  the examples, under the project's licenses: a barrel colored by its vertices, a crate showing
  `crate.png`, and a table whose legs share a mesh, each standing on its origin, a tile being a
  metre. `props.blend` is their source, each prop a collection whose exporter writes its `.glb`
  beside it (File > Export All Collections); after changing it, export them again and copy them
  to `cellar/models`. Keep the live Blender session's work safe: build or save props in a Blender
  of their own (`blender --background --factory-startup --python ...`), and never write a
  `.blend` from a session holding someone's unsaved work. `native-particles/` is a native library,
  a `cdylib` built as `particles`, which `tests/native.rs` builds and loads; its sparks are
  billboards in 3D views. `gui.rs` shows egui
  over a game: sprites bouncing about, tuned from a panel that shows a `GuiCanvas` map of them,
  and the debug window on F3. `shapes.rs` draws each kind of shape: fills, outlines, a line and
  circles. `meshes.rs` draws meshes in a 3D view at a fixed resolution, with a HUD over it: crates,
  a post and a bouncing ball on a field it builds as a mesh of its own, and the table and two
  barrels from `models/`, and it has tests too.
- `README.md`: the project's overview, with a screenshot of each example in `docs/screenshots/`.
  They were taken headlessly at 640x360 (768x432 for `sprites`, so its 256x144 canvas scales
  without bars, 960x720 for `shmup` and 960x540 for `course`, twice their canvases, and 1280x720
  for the editor, with the
  garden open and its pond selected with the Move tool); retake them when an example changes
  how it looks. The shmup's results, how many bullets fit in a frame and what each costs, come
  from the stress test, run with the script beside them, and the per-bullet costs from fitting
  its benchmark's CSV against the quad count; the split of the update time was measured by timing
  `step` and `collide` with `os.clock`. Measure them again on the same machine, a Ryzen 7 5800X
  with an RTX 3080, after changes that could move them.
- `docs/`: the user docs for making games in Luau: a quick start, a game's folder and frame,
  running and checking games, scenes, the editor, native libraries with a quick start, and in
  `docs/api/` a reference for every table, function and type scripts use, `Ui` among them
  (`gui.md`), and for `wingine-native`. They restate the module docs (`src/runtime/mod.rs`,
  `src/lua.rs`, `src/scene/mod.rs`, `crates/wingine-native`) for game authors, and the
  definitions file (`src/runtime/wingine.d.luau`) gives the same API its types, so change them
  together; the editor's (`docs/editor.md`) follows what the editor does, so change it with the
  editor. Check their examples by running them headlessly.
- `tests/screenshots.rs`: screenshot tests, with reference images in `tests/screenshots/`.
- `tests/simulation.rs`: tests for `Simulation` and scripted runs.
- `tests/export.rs`: tests for exports, end to end: `wingine export` of a small game, then the
  exported executable started with no arguments and `WINGINE_SCRIPT` set, checking from its
  screenshot that it found and ran the game beside it, or showed the error loading it or its
  settings; and an output folder in use refused.
- `tests/testing.rs`: tests for `runtime::testing`, with small games and their tests written to
  temporary folders: finding and reporting tests, each starting afresh, reading the game's state
  as it is, waits and input, the game stopping and time limits, refused requests, files that don't
  load, modules, filters, the interface, and screenshots matching, differing and updated.
- `tests/gui.rs`: tests for egui over games, through `Simulation`: clicking widgets by label,
  typing into a focused field, input egui takes not reaching the game, and the debug key.
- `tests/runtime.rs`: tests for the Luau runtime, with small games written to temporary folders,
  their saves, their 3D views, models and picking, scenes' views and their nodes, menus over
  nodes, in its
  `physics` module (with the `physics` feature), physics worlds of their nodes, 2D and 3D (bodies
  falling, characters walking, fixed bodies following and placed by turned parents, models' triangles,
  rays, overlaps, sensors and views' rays, and the errors), and in its
  `gui` module (with the `egui` feature), their interfaces, clicked by label, and in its
  `reload` module, reloading: changed scripts and assets, what `reloading` keeps, what's reset,
  failures, settings, and a watching game reloading in real time. A hash of what
  `exact` gives for spreads of arguments is pinned, so CI checks it's the same on Windows and
  Linux.
- `tests/scene.rs`: tests for scenes loaded to play: finding nodes, transforms, hit testing,
  anchors and stretching, stacks and panels laying out and fitting what they hold, tilemaps, instances and overrides, adding and removing nodes, posing
  with clips, and 3D views: placement composing, instances in views, posing in 3D, picking
  nearest first, and where 3D nodes may be, with files held in memory.
- `tests/native.rs`: tests for native libraries, most of them linked into the test, its probe
  library making meshes and drawing them and billboards in views of its own and the script's. One
  builds `native-particles` with a nested `cargo build` into a target folder of its own and loads
  it from a file.
- `tests/pong.rs`: tests for the `pong` example, which check its Luau state by evaluating
  expressions such as `pong.ball.vx` (`Simulation::eval`), and play it with simulated keys, mouse
  and gamepad, and reload it mid-match; and run its Luau tests, `pong.test.luau`, through
  `runtime::testing::run` (saving their reference screenshot again with `UPDATE_SCREENSHOTS=1`).
  One
  plays a whole match and checks a hash of it, pinned, so CI checks it's the same to the bit on
  Windows and Linux; a change to how Pong plays changes the hash.
- `tests/rpg.rs`: tests for the `rpg` example, the same way, playing through the quest, using
  its debug panel, its title menu and bag's panels fitting what they hold, questions answered by
  mouse, its scene files being in the canonical layout, and continuing a saved game in a second simulation sharing the first's
  save folder. Its `simulation()` starts a new game from the title screen.
- `tests/garden.rs`: tests for the `garden` example, the same way: walking, stopping at and
  sliding along what's solid and the ground's edges, its F1 view, and that its scene files are in
  the canonical layout.
- `tests/orchard.rs`: tests for the `orchard` example, the same way: its clips, the picker's walk
  easing in and out, catching and missing apples, the reactions, the end of the day and the
  camera's close-up, and that its scene files are in the canonical layout.
- `tests/cellar.rs`: tests for the `cellar` example, the same way: walls and props stopping the
  hero, walking through the cellar lighting every candle, what each frame draws, from its frame
  stats, the mouse pointing at props, the camera following the hero, and that its level is in the
  canonical layout.
- `tests/course.rs`: tests for the `course` example, the same way: crates settling, walls
  stopping the player without its creeping along them, climbing the stairs, walking down the
  slope on the ground all the way, jumping onto the block, pushing crates off the platform and
  toppling the stack, falling off and starting again, finding every coin, and the shadow under
  the player, on the ground, the slope and the block, and hidden off the course. One plays a run and
  checks a hash of every crate's pose and the player's feet, pinned, so CI checks physics is the
  same to the bit on Windows and Linux; a change to how the course plays changes it.
- `tests/shmup.rs`: tests for the `shmup` example, the same way: the title menu by key and by
  mouse, its HUD's scene, the player, the bullet systems
  through `shmup.bullets`, spell cards, clearing the stage with cheats, the same seed firing the
  same bullets, and the stress test. They run copies of the game in `target/tmp`, one with the
  `danmaku` library, which they build with a nested `cargo build` as `tests/native.rs` builds
  `particles`, and one without, so a library in `examples/shmup/native` changes nothing. Tests
  of the bullet systems run each of them, and others check that both fire the same bullets, to
  the bit, through the stage and the boss, draw the same pixels, and that switching between them
  mid-fight changes nothing.
- `benches/drawing.rs`: what drawing a sprite costs from Rust, from Luau, and through a native
  library linked in, what moving a bullet costs in Luau, and what drawing and rendering 10,000
  sprites, billboards in a 3D view and cubes in one cost, measured with frame stats in a
  `Simulation`. A plain program (`harness = false`); CI doesn't
  run it. Its numbers shift by a third with unrelated changes to the program, from how the
  compiler lays it out, so compare runs of the same build, or confirm a change against the
  commit before it.

## Conventions

- Drawing coordinates are canvas pixels with the origin at the top-left and `y` pointing down.
  The canvas is the window's size in logical pixels (`Resolution::Window`), or a fixed size scaled
  up by whole numbers with black bars (`Resolution::Fixed`, the choice for pixel art).
- `Canvas::set_camera` switches drawing to world coordinates; the camera's translation and zoom
  are applied on the CPU as quads are recorded, so switching costs nothing. Draw the world with a
  camera, then call `set_camera(None)` and draw the HUD in canvas pixels.
- Tiles are addressed as `[column, row]` (`[i32; 2]`, so neighbors off the map's edge are
  representable), with the map's top-left corner at the world origin. A `GridMover` occupies its
  destination tile from the start of a step. Movement asks a `blocked` closure whether it may
  enter a tile, so games can combine `Tilemap::is_solid` with their own rules, such as NPCs.
- Text positions are the top-left of the first line, rounded to whole units so pixel fonts stay
  crisp. Measured widths run to the edge of the ink, not the trailing advance, so centering is
  exact.
- `Color` values are sRGB, as authored. The renderer converts them to linear space, and the
  surface is viewed through an sRGB format so blending happens in linear space. Textures are
  uploaded as sRGB for the same reason.
- Recording a quad runs for every sprite drawn, so keep it free of calls into the math library:
  `f32::round`, `powf` and the like are function calls on baseline x86-64, and each cost as
  much as the rest of the quad. Measure changes to it with `cargo bench`.
- Textures are sampled nearest-neighbor so pixel art stays crisp.
- Luau scripts reach game state only through functions the game registers. Those functions share
  state with the game through `Rc<RefCell<_>>`, so the game must not hold a borrow while scripts
  run (`Scripts::update`, `spawn` or `call`). A game function that waits, such as showing
  dialogue until it's closed, is a Rust function that starts the action plus a few lines of Luau
  that `wait_until` it's done.
- Script errors don't stop the game: a failing task stops, the rest carry on, and `update`
  returns the error to log.
- Volumes are linear (1 is as recorded, 0 is silent) and converted to kira's decibels inside
  `Audio`. Failing to play a sound effect (too many at once) logs a warning rather than
  returning an error; loading and music return errors.
- Keys are physical (by position, named for US QWERTY), so WASD works on any layout. Games
  bind named actions (strings, so Lua scripts can use them too) to keys and query the actions.
- The mouse position is in canvas pixels (through any fixed-resolution scaling and bars).
  `Camera::canvas_to_world` converts it to world coordinates, using `Context::canvas_size`.
- egui interfaces draw over the whole frame at the output's resolution, not on the canvas: their
  points are logical window pixels, through any fixed resolution's scaling. Engine drawing shows
  in them through `GuiCanvas`. Tests find their widgets by label, so give a widget whose label egui
  doesn't know, such as a number field beside a label, one of its own (`Response::widget_info`).
  egui sizes a new window or menu in its first frame and shows it in the next, and fades windows
  in and out, so tests run frames before looking for them.
- wgpu and winit types stay internal (`pub(crate)`). Games use the engine's own API. `mlua` and
  `egui` are the exceptions: `wingine::lua` re-exports mlua, since registering functions and
  converting values is its API, and `wingine::gui` re-exports egui, whose widgets are. Engine
  types scripts use implement `FromLua`/`IntoLua` or `UserData` in
  `src/lua.rs` (`Direction` is a string such as `"up"`, `Color` a hex string such as
  `"#f2c14e"`, a table or a number `0xRRGGBBAA`, and `Font` userdata with methods).
- The Luau runtime lends its functions the `Context` while the game loads, updates and lays out
  its interface, and the `Canvas` while it draws, through `Lent` in `src/runtime/mod.rs`: a
  pointer that `Lent::lend` sets while it runs, with a borrow flag, which every engine function
  captures a handle to (`lent.context(...)`, `lent.canvas(...)`). Called at any other time, they
  fail with a Lua error rather than reaching stale state. Looking what's lent up in the Lua
  registry would cost more than drawing; so do conversions mlua makes through `Value`, such as
  reading a string's bytes, which is why scripts that draw a lot pass colors as numbers. Each
  update runs `update(dt)`, then resumes tasks; each frame runs its updates (one, or with an
  update rate, as many as its time holds), then `gui(ui)`, then `draw()`. Presses and releases
  last until an update has seen them.
- Luau reads a global's fields named in a chunk, such as `shmup.system`, when the chunk loads,
  if the global exists then (its "imports": the sandbox lets it assume globals don't change). A
  chunk that changes such a field, such as by calling a function, and reads it back sees the old
  value, and so does a module loaded after a game's global table exists, if the field is replaced
  later. Games create their global table after requiring their modules, and tests read state
  back with a chunk of its own (`Simulation::eval`), not in the chunk that changed it.
- Native libraries reach the engine only through the ABI's function pointers, and get the same
  lent services scripts do: a library function called from `draw` gets the canvas, not the
  context. Hooks run in the order libraries are listed, before and after each `update` (and its
  tasks) and `draw`; hooks after `draw` draw in canvas pixels. Drawing many sprites goes through
  one `draw_sprites` call with a slice of `SpriteDraw`s, since each call from Luau into a library
  costs far more than drawing a sprite. Errors cross the ABI as strings,
  set with `set_error` before returning `Status::ERROR`, and the helper crate catches panics so
  they never unwind into the engine. Change the ABI only by adding to the end of `Host` and
  bumping `ABI_MINOR`; anything else bumps `ABI_MAJOR`.

## Commands

```bash
cargo build
cargo test
cargo clippy --all-targets
cargo clippy -p wingine --no-default-features --all-targets
cargo fmt
cargo run --example shapes
cargo run --example sprites
cargo run --example gui
cargo run -p wingine-editor -- examples/garden
cargo run -- examples/pong
cargo run -- --watch examples/pong
cargo run -- test examples/pong
cargo run --release -- export examples/pong target/export/pong
cargo run -- examples/rpg
cargo run -- examples/garden
cargo run -- examples/orchard
cargo run -- examples/cellar
cargo run -- examples/course
cargo run --example meshes
cargo run --release -- examples/shmup
cargo build --release -p native-danmaku
cargo bench
```

Run `cargo fmt` and `cargo clippy --all-targets` before considering a change finished, and keep
both clean, with the engine's `egui` feature off too. CI (`.github/workflows/ci.yml`) runs fmt,
clippy with `-D warnings` with and without the feature, and `cargo test` on Linux and Windows.
`release.yml` builds the runtime and the editor in release for Windows and Linux, and pushing a
`v*` tag publishes them as a GitHub release of it (run by hand, it only keeps them as artifacts).
Linux builds on the oldest Ubuntu runner, since the binaries need a glibc as new as the one they're
built against, and Windows links the C runtime statically, so they don't need the Visual C++
Redistributable. Each archive has `THIRD-PARTY-NOTICES.txt`, the licenses of the crates built into
its binaries, which `cargo about` (pinned in `release.yml`) writes for the target from the
editor's dependencies, with `about.toml` and `about.hbs`. It fails on a license `about.toml`
doesn't accept, so a new dependency's license may need adding there, and its clarifications point
crates whose license files it doesn't recognize at them by checksum, which upgrading those crates
can change. The template leaves out the workspace's own crates (those without a `source`).

## Checking game behavior

Check how a game behaves headlessly instead of opening a window and driving it by hand:

- **Ad hoc:** write a script and run the game with `WINGINE_SCRIPT` pointing at it. The game
  runs without a window, follows the script's simulated input, and saves screenshots next to the
  script, then exits. The format is documented in `src/script.rs`:

  ```text
  screenshot shots/start.png
  hold Right 0.5s
  tap Up
  click 88 72
  wait 1s
  resize 1900 650
  screenshot shots/after.png
  ```

  ```bash
  WINGINE_SCRIPT=path/to/check.txt cargo run --example sprites
  WINGINE_SCRIPT=path/to/check.txt cargo run -- examples/pong
  ```

  Mouse positions are canvas pixels; screenshots are window-sized, so with a fixed resolution
  divide screenshot coordinates by the scale (and subtract the bars) to aim a click.

  `benchmark 10s results.csv` renders every frame for that long and prints the mean, median,
  99th percentile and longest of each frame time, optionally saving each frame's stats as CSV.
  Benchmark release builds (`cargo run --release`): Luau and the engine are far slower in debug.
- **As tests:** drive the game with `Simulation` (`tap`, `hold`, `tap_action`, `click`,
  `run_for`, `game()`) and assert on its state, as `examples/sprites.rs` does. For a Luau game,
  evaluate Luau expressions against its state and set it up with `eval` and `exec`, as
  `tests/pong.rs` does, or write its tests in Luau, in `*.test.luau` files that `cargo run --
  test path/to/game` runs, as `examples/pong/pong.test.luau` is. Prefer this for anything worth
  keeping, so CI catches regressions.

Windowing itself (focus, minimizing, real resizes, DPI) still needs a real window. Audio in these
runs is silent; `cargo test -- --ignored` runs a check that plays (muted) through the real sound
device.

## Screenshot tests

Rendering changes are covered by screenshot tests: each renders a scene with `Headless` and
compares it with a reference PNG (`Image::compare`, the comparator games get too), allowing a
per-channel difference of 2 so references pass on any GPU. Add a test there when adding a drawing
feature.

- A mismatch writes the rendered frame and a diff (mismatched pixels in magenta) to
  `target/tmp/screenshots/`. In CI they are uploaded as a build artifact.
- To create or update references, run with `UPDATE_SCREENSHOTS=1`. Only missing or mismatched
  references are rewritten. Look at every changed PNG before committing it.
- Where a pixel's middle lands on the edge between two texels, GPUs pick either, so 2D scenes keep
  sprites on whole pixels at whole scales. Textures in perspective can't, so `meshes_textured`
  lets a few pixels differ (`assert_screenshot_allowing`); the other 3D scenes are untextured.
- CI runners have no GPU: Linux uses lavapipe (`mesa-vulkan-drivers`) and Windows uses WARP. To
  reproduce locally, pick the adapter with wgpu's environment variables, e.g. on Windows:
  `WGPU_BACKEND=dx12 WGPU_ADAPTER_NAME="Microsoft Basic Render Driver" WGPU_DX12_COMPILER=fxc`.
  Set `WGPU_DX12_COMPILER=fxc` whenever testing on WARP: the Vulkan SDK's `dxcompiler.dll`
  crashes it if it's on `PATH`.
