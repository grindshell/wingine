# Wingine

Wingine is a desktop game engine written in Rust, built as a library crate (`wingine`). It is
primarily a **2D** engine, with limited 3D: meshes and billboarded sprites drawn in 3D views
among the 2D, from Rust and Luau. 3D is not a focus — don't let 3D concerns drive the design of 2D
systems, and don't build more 3D features unless asked.

## Scope

Wingine has what its examples need: tilemaps and batched sprites, meshes and glTF models in 3D
views picked by boxes, sprite animation, a camera, keyboard, mouse and gamepad input bound to
actions, grid movement, text, audio, saves, Luau scripting with tasks, native libraries written
in Rust, scenes (levels, screens and rigged characters), egui interfaces, and an editor built on
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
| Scripting | `mlua`  | Luau, with the `luau-jit` feature        |
| Native libraries | `libloading` | Loads them, and never unloads them |
| Interfaces | `egui` | With `egui-winit` and `egui-wgpu`, behind the default `egui` feature |
| Gamepads | `gilrs` | Read by the window; simulated without one |
| Models | `gltf` | glTF 2.0 files, behind the default `models` feature |

- Rust edition 2024.
- Licensed under MIT or Apache 2.0, at the user's option (`LICENSE-MIT`, `LICENSE-APACHE`), which
  every crate's manifest says with `license = "MIT OR Apache-2.0"`; new crates say so too.
- Target platforms: desktop only (Windows, macOS, Linux).
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
  canvas pixels, and clears per-frame presses and releases after each `update`. Actions bind to
  `Control`s, keys and gamepad buttons, of any gamepad or one, and have a strength from 0 to 1,
  which `axis` makes an axis of two. Gamepads are numbered from 1, keeping their number while
  disconnected. Their buttons include the sticks' directions (`LeftStickLeft`), pushed as far as
  the stick is that way through a radial dead zone (`dead_zoned`); every button is held from 0.5
  of the way and released below 0.4, so a trigger or stick near the line doesn't flicker.
  `src/gamepads.rs` feeds it from gilrs, polled at the start of each of a window's frames (so a
  window that redraws on demand reads them only in frames that come anyway), and only while it's
  focused; a `Simulation` connects and pushes simulated ones.
- `src/graphics/`: `Gpu` (adapter + device), `WindowSurface` (presents to the window),
  `Renderer` (wgpu pipelines), `Screen` (applies the `Resolution`), and the public drawing API:
  `Canvas` (what games draw to each frame), `Camera`, `Resolution`, `Texture`, `Sprite`,
  `SpriteSheet` and `Animation`. An animation doesn't keep time: games keep how long it has played
  and ask it for the frame to draw, so one animation can be shared. Rotated sprites turn about the
  center of their destination, on the CPU as quads are recorded, and transformed sprites and fills
  (`draw_sprite_transformed`, `fill_rect_transformed`) are placed by an `Affine` the same way,
  which is how scenes draw. Outlines, lines and circles are quads too: an outline four that meet
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
  no vertices on the CPU: `Bounds3` (`math.rs`) from `MeshData::bounds`, kept by `Mesh` and
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
- `src/headless.rs`: `Headless`, which renders a `Canvas` to an in-memory image without a window,
  and `Simulation`, which runs a whole `Game` without a window: fixed 60 fps frames, simulated
  keys, mouse and gamepads, and screenshots on demand. Its frames render only for screenshots, unless
  `set_render_every_frame` asks for every frame to render and wait for the GPU, as benchmarks
  want. All of them in a process share one GPU device, and
  only one thread at a time may hold any (a reentrant lease): lavapipe crashes with a device per
  thread and WARP renders wrongly with a shared one, so parallel GPU tests take turns. Simulated
  input reaches egui too, with `type_text` and `scroll` for it alone; screenshots show the
  interface; `request_close` presses the window's close button and `has_quit` tells whether the
  game called `Context::quit`. Each keeps its game's saves in a temporary folder of its own,
  unless its config gives one. Simulations run every frame they're asked to, but
  `next_frame_in` tells when a game that redraws on demand would want its next (input wants one
  at once), so tests check it waits while nothing changes.
- `src/lua.rs`: `Scripts`, a sandboxed Luau VM for gameplay scripts, and its task scheduler.
  Games register Rust functions (`set_function`), load script files, and start script functions
  as tasks (`spawn`): coroutines that `update` resumes once a frame with the frame's duration.
  The built-ins `wait`, `wait_until`, `spawn` and `print` are documented at the top of the file,
  with `exact`, a table of math functions from the `libm` crate, which give the same bits on
  every platform, unlike `math`'s from the C library (Windows' and Linux's differ). They can't
  replace `math`'s: Luau calls those as builtins without looking them up, which also makes them
  about five times quicker, so games call `exact`'s for what must replay and `math`'s for show.
  Scripts compile at Luau's optimization level 2, whose inlining halves calls between Luau
  functions. Its unit tests don't need a GPU.
- `src/runtime/`: games written entirely in Luau, run from a folder holding `main.luau` and an
  optional `conf.luau` of settings: the window's, the update rate, the identity that names its
  save folder, and the native libraries to load, any of them optional (skipped if there's no such
  library, but an error if it's there and fails to load). `LuauGame`
  is the `Game` that runs one, and the engine's API for it is the global tables `graphics`,
  `input`, `audio`, `tilemap`, `grid`, `scene`, `save` and `window`, one file each. The close
  button asks the game's global `close_requested`, lent the context as `update` is, and closes
  unless it returns false; a game stopped by an error always closes. The module docs are the
  reference for script authors; the engine types those tables hand out (sprites, animations,
  meshes, tilemaps, grid movers, text layouts, scene nodes) are userdata, documented with their
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
  node passes its parent's on. So `transform`, `bounds` and `contains` walk down from the root.
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
- `src/main.rs`: the `wingine` executable, which runs a Luau game: `cargo run -- examples/pong`.
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
  which never changes the file. In 3D views, seen through their own cameras (the editor has
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
  (`play.rs`) runs the editor's own executable with `--run`, which runs the game, so it's a
  process of its own; tests point it at the built executable with `set_runner`. The editor
  redraws on demand: threads that read the game played and luau-lsp wake it with a `Waker` when
  they have something, as does `project::Watcher`, a thread that hashes the folder's files' sizes
  and modified times every second, so the editor reads the folder again only when it's changed;
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
  them, and `bag.luau` (the menu key) equips gear and uses items. `ui.luau` holds the panels'
  shared look. `story.reached(stage)` compares quest stages, which run in the order of
  `story.STAGES`. F1 shows `debug_panel.luau`, an egui window from the game's `gui` that sets hit
  points, coins, items and the quest's stage, and jumps between maps (`ARRIVALS` in `main.luau`).
  The game opens on `title.luau`, a title screen drawn in code, whose Continue (offered when
  there's a save) restores the save `progress`, which `progress.luau` writes after every event
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
  `hud.luau` the panel and screens, laid out in `hud.scene.luau` (in canvas pixels, so the title
  menu's items, areas, are hit-tested against the mouse), which it fills in from the game's
  state and draws each frame; `shmup.hud` is the scene's root, for tests, which check the file
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
  frame for a while and reports how long frames took.
- `examples/garden/`: a garden around a windmill, laid out in `garden.scene.luau`, which
  `main.luau` loads and plays: it turns the mill's sails and a flock of birds (nodes whose
  children turn with them), walks the player, which waddles and mirrors about its feet, stops it
  at nodes tagged `solid`, and shows the `text` of areas tagged `note`. Its trees and flowers are
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
- `src/math.rs`: geometry types: `Rect`, `Affine` and `Affine3`, 2D and 3D transforms that
  compose with `*`, and `Bounds3`, a box in 3D.
- `examples/`: small runnable programs that exercise engine features, plus their assets.
  `sprites` has tests (enabled with `test = true` in `Cargo.toml`) that drive it with
  `Simulation`, and its events and NPC are scripted in `sprites.luau`. `pong/` is a game written
  entirely in Luau, with its computer opponent in `opponent.luau`, played with the keys, the mouse
  or a gamepad, whose stick moves the paddle as fast as it's pushed (`input.axis`). Nothing in it
  is random, and its ball's angles come from `exact`, so the same inputs play the same match on
  every platform. The sounds and music in
  `examples/sounds/` were synthesized for `sprites`; `pong`, `rpg` and `shmup` have copies of
  the ones they use, and `rpg`, `shmup`, `garden` and `cellar` copies of the tileset.
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
  without bars, 960x720 for `shmup`, twice its canvas, and 1280x720 for the editor, with the
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
- `tests/gui.rs`: tests for egui over games, through `Simulation`: clicking widgets by label,
  typing into a focused field, input egui takes not reaching the game, and the debug key.
- `tests/runtime.rs`: tests for the Luau runtime, with small games written to temporary folders,
  their saves, their 3D views, models and picking, scenes' views and their nodes, and in its
  `gui` module (with the `egui` feature), their interfaces, clicked by label. A hash of what
  `exact` gives for spreads of arguments is pinned, so CI checks it's the same on Windows and
  Linux.
- `tests/scene.rs`: tests for scenes loaded to play: finding nodes, transforms, hit testing,
  anchors and stretching, tilemaps, instances and overrides, adding and removing nodes, posing
  with clips, and 3D views: placement composing, instances in views, posing in 3D, picking
  nearest first, and where 3D nodes may be, with files held in memory.
- `tests/native.rs`: tests for native libraries, most of them linked into the test, its probe
  library making meshes and drawing them and billboards in views of its own and the script's. One
  builds `native-particles` with a nested `cargo build` into a target folder of its own and loads
  it from a file.
- `tests/pong.rs`: tests for the `pong` example, which check its Luau state by evaluating
  expressions such as `pong.ball.vx`, and play it with simulated keys, mouse and gamepad. One
  plays a whole match and checks a hash of it, pinned, so CI checks it's the same to the bit on
  Windows and Linux; a change to how Pong plays changes the hash.
- `tests/rpg.rs`: tests for the `rpg` example, the same way, playing through the quest, using
  its debug panel, and continuing a saved game in a second simulation sharing the first's save
  folder. Its `simulation()` starts a new game from the title screen.
- `tests/garden.rs`: tests for the `garden` example, the same way, its F1 view, and that its scene
  files are in the canonical layout.
- `tests/orchard.rs`: tests for the `orchard` example, the same way: its clips, the picker's walk
  easing in and out, catching and missing apples, the reactions, the end of the day and the
  camera's close-up, and that its scene files are in the canonical layout.
- `tests/cellar.rs`: tests for the `cellar` example, the same way: walls and props stopping the
  hero, walking through the cellar lighting every candle, what each frame draws, from its frame
  stats, the mouse pointing at props, the camera following the hero, and that its level is in the
  canonical layout.
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
  back with a chunk of its own (`eval`), not in the chunk that changed it.
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
cargo run -- examples/rpg
cargo run -- examples/garden
cargo run -- examples/orchard
cargo run -- examples/cellar
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
- **As tests:** drive the game with `Simulation` (`tap`, `hold`, `click`, `run_for`, `game()`)
  and assert on its state, as `examples/sprites.rs` does. For a Luau game, evaluate Luau
  expressions against its state, as `tests/pong.rs` does. Prefer this for anything worth
  keeping, so CI catches regressions.

Windowing itself (focus, minimizing, real resizes, DPI) still needs a real window. Audio in these
runs is silent; `cargo test -- --ignored` runs a check that plays (muted) through the real sound
device.

## Screenshot tests

Rendering changes are covered by screenshot tests: each renders a scene with `Headless` and
compares it with a reference PNG, allowing a per-channel difference of 2 so references pass on any
GPU. Add a test there when adding a drawing feature.

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
