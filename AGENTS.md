# Wingine

Wingine is a desktop game engine written in Rust, built as a library crate (`wingine`). It is
primarily a **2D** engine, with limited 3D: meshes and billboarded sprites drawn in 3D views
among the 2D, from Rust and Luau. 3D is not a focus — don't let 3D concerns drive the design of 2D
systems, and don't build more 3D features unless asked.

## Scope

Wingine has what its examples need: tilemaps and batched sprites in blend modes, meshes and glTF
models in 3D views picked by boxes, lit by a sun and by lights at places that cast shadows,
glowing, and fading into fog under a sky,
sprite animation, particles, a camera, keyboard, mouse and gamepad input bound to
actions, grid movement, physics with Rapier (bodies and characters, in 2D and in 3D views),
text in the built-in font, bitmap fonts and TrueType fonts, audio, songs of the engine's own
instruments baked or played live, saves, folders the player picks, Luau scripting with
tasks, native libraries written in Rust, scenes (levels, screens and rigged characters), menus
over scenes' nodes, egui interfaces, and an editor built on the engine. Each feature is finished
with a game or example that uses it, with tests.

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
| Fonts | `ab_glyph` | TrueType and OpenType fonts, rasterized as text needs their glyphs |
| Folder dialogs | `rfd` | The system's dialog for picking a folder, off the web; on Linux, the desktop's portal, not GTK |

- Rust edition 2024. Licensed under MIT or Apache 2.0 (`LICENSE-MIT`, `LICENSE-APACHE`); every
  crate's manifest says `license = "MIT OR Apache-2.0"`, and new crates say so too.
- Targets: Windows, macOS, Linux, and the web (`wasm32-unknown-unknown` with WebGPU, as
  `docs/web.md` describes, built by the `web/` workspace, which patches in mlua's master branch
  until a release supports it). Code
  that differs on the web is behind `cfg(target_arch = "wasm32")`, and files games load are read
  through `crate::files`, never `std::fs`.
- `wgpu`, `winit`, `mlua`, `kira` and `egui` change their APIs substantially between releases:
  check the version pinned in `Cargo.toml` and its docs on docs.rs rather than memory. egui's
  crates must target the engine's wgpu and winit, so they're upgraded together; egui crates from
  outside its repository, such as `egui_dock`, often lag a release behind.
- Default features, each of which CI also lints without:
  - `egui` brings egui in. Its icon fonts are left out unless `monochrome_emoji_fonts` is on, as
    the editor turns it on.
  - `models` reads glTF with `gltf` built without its default features (whose `import` brings a
    JPEG decoder); the engine reads buffers, data URIs and PNG textures itself. Without it,
    `Context::load_model` doesn't exist and `graphics.load_model` fails.
  - `physics` brings `rapier2d` and `rapier3d` in, with `enhanced-determinism`, without which
    their results differ between Windows and Linux. Meshes then keep their triangles on the CPU
    too (`Mesh::geometry`). Rapier breaks its API in most minor releases, and is Apache-2.0 only,
    shipping no license file.
- `kira` is built with only the WAV and OGG Vorbis decoders. On Linux, `cpal` needs
  `libasound2-dev` and `gilrs` needs `libudev-dev` (CI installs both). gilrs hears gamepads on
  Windows only while the process has a focused window; the engine skips gilrs's filters but the
  D-pad's and the jitter's, applying its own dead zone.
- Scripts are **Luau**, not Lua 5.x: type annotations, compound assignment, `continue`,
  generalized `for ... in t`, and a sandboxed standard library. `mlua` builds Luau from C++, so
  building needs a C++ compiler.
- The editor's Script screen runs [luau-lsp](https://github.com/JohnnyMorganz/luau-lsp) if it's on
  the `PATH` or in `WINGINE_LUAU_LSP`. Tests that need it check nothing without it, unless
  `WINGINE_LUAU_LSP` is set, as CI sets it to the release pinned in `ci.yml`. Its definitions
  syntax is unstable, so check `wingine.d.luau` still loads after upgrading it.

## Layout

A map of where things are and the rules that aren't obvious from the code. Module and item docs
hold the rest; the module docs of `src/runtime/mod.rs`, `src/lua.rs`, `src/scene/mod.rs`,
`src/runtime/testing.rs`, `src/script.rs` and `crates/wingine-native` are references in their
own right.

### Engine

- `src/app.rs`: `run`, the `Game` trait and the winit event loop. `run_frame` runs a frame for the
  window and `Simulation` alike: updates as `Timestep` decides (one by the frame's time, or with
  `WindowConfig::update_rate`, as many fixed steps as the frame holds, tolerant of jitter), then
  egui (`Game::gui`), then `draw`. `WINGINE_SCRIPT` hands off to the script runner. Fullscreen
  falls back to borderless where there's no exclusive mode. With `Redraw::OnDemand`, frames come
  only when asked for (`Redraws`, shared with the `Context`, `Waker`s and egui).
- `src/context.rs`: `Context`, the services games get: input, audio, saves, canvas size, fonts,
  loading textures, fonts and models, `FrameStats`, and redraw requests.
- `src/graphics/`: `Canvas` records quads, all through one textured shader (fills use a white
  texture), batched by texture and `Blend` into draw calls, a pipeline for each blend mode, all
  blending premultiplied colors (normal is premultiplied alpha blending, which renders as straight
  alpha did); rotation, transforms, slicing, outlines, lines
  and circles are worked out on the CPU as quads are recorded. Vertices are 20 bytes, colors
  sRGB bytes made linear in the shader. A fixed resolution renders offscreen, its target cleared
  opaque since drawing it onto the output premultiplies again, then onto the output as its
  `Scaling` says. Sprites aren't instanced: rendering is a fifth of the shmup's
  frame, so it isn't the limit. An `Animation` doesn't keep time; games ask it for the frame at a
  time. Shaders are WGSL files beside the code that loads them.
- 3D (`camera3d.rs`, `mesh.rs`, `view3d.rs`, `model.rs`, `model_file.rs`): `Canvas::begin_3d`
  records a `View3d` and starts the next batch of quads empty, so 2D never joins across a view;
  `Renderer::render` draws each view in a pass of its own with a depth buffer. Right-handed, `y`
  up, as glTF. Texels under half opaque are discarded and no mesh blends, so billboards need no
  sorting, and a view's draws of the same mesh and texture are one instanced draw wherever they
  were drawn, each kind in the order it was first drawn (`MeshOrder`). Only particles blend: a
  draw's `blend`, part of its kind, puts it after every opaque draw, drawn by a pipeline of its
  mode that tests depth and doesn't write it, through `mesh.wgsl`'s `fs_blended`, whose
  `fades_to` constant fogs what adds or screens to black and what multiplies to white; those
  blended normally sort far to near by `MeshDraw::depth`, each run of a kind a draw call. `model_file::read` reads
  glTF without a GPU into `ModelData`. Picking is by boxes (`Bounds3`, `Ray::hit_box`), not
  triangles. Luau's points are native `vector`s; `draw_mesh` and `draw_billboard` add to the view
  begun last only while nothing else has been drawn since (`Canvas::open_view`).
- Lights (`Light`, in `view3d.rs`): `Canvas3d::add_light` records point and spot lights for a
  view, whose uniform carries up to `Light::MAX` of them, chosen by `lights_of` (those reaching
  what the camera sees, the nearest first, for the frame's stats and the renderer alike), and
  `mesh.wgsl` loops over them for every pixel. Light falls off in a straight line to a light's
  range, adds, and stops at white. `Light::level_at` is the same rule on the CPU, which knows
  nothing of shadows. The shader knows a billboard, which takes a light's whole level, by the
  normal matrix only billboards have. In scenes a light is a `light` node (`LightDef`), a spot
  shining along its node's `+z`. `Lighting` and `View3d` stay `Copy`, and native libraries can't
  add lights: the ABI's lighting is unchanged.
- Bounds: a light may keep to a box (`Light::bounds`, an `Affine3` taking the world onto the cube
  from -1 to 1, made by `Light::within`), lighting nothing outside it, faces inclusive, so one
  without shadows doesn't light through walls. The uniform carries the affine's three rows, and
  `shadow.w` says a light has one; `mesh.wgsl` skips a light for a pixel outside it, `level_at`
  gives 0 there, and `lights_of` leaves out a light whose box's corners are all outside one side
  of what the camera sees. In scenes it's a light node's `bounds`, six numbers in its parent's
  coordinates (`LightDef::bounds`, a `Bounds3`), so the box moves, turns and stretches with the
  parent; `draw_3d` hands `LightDef::light` the parent's placement, and a box placed with no room
  in it lights nothing.
- Shadows: a light with `Light::shadows` takes one shadow map, or six (`Light::shadow_maps`), of
  the `Light::SHADOW_MAPS` a view has, handed out by `shadows_of`, the nearest the camera first;
  a light left without lights the view without shadows, and the sun casts none. The maps are the
  layers of one depth texture, made when a light first has shadows and the same ones for every
  view, so the renderer draws a view's just before the view (`draw_shadows`, through
  `shadow.wgsl`, culling nothing). `mesh.wgsl` reads a map once for a pixel, so shadows' edges
  are hard, after moving the surface off itself by a few of the map's texels, which keeps it from
  shading itself. Whether a mesh casts is each draw's (`MeshDraw::casts_shadow`, part of its
  kind, as `Canvas3d::set_casts_shadow` had it when it was drawn); billboards never do. In
  scenes it's a mesh or model node's `casts_shadow`, and a light node's `shadows`.
- Glow: a model's parts give off their file's emission (`ModelPart::emission`, times its
  `emission_sprite`), and what's drawn shows `Canvas3d::set_glow` of its own colors, both added
  to what light shows of a pixel in `mesh.wgsl`, unshaded, before fog, and capped at white with
  it. Each rides its draw's `Instance` (112 bytes), so glow never splits a draw call; an
  emission texture is part of a draw's kind, bound as the mesh pipeline's group 2 (white for
  none). `model_file` reads `emissiveFactor`, `KHR_materials_emissive_strength` and an emission
  texture on the base color's coordinates. In scenes it's a mesh, model or billboard node's
  `glow`, written only if it isn't black. Native libraries' draws never glow: `native.rs` sets
  it black before them.
- Fog (`Fog`, a `View3d`'s `fog`): `mesh.wgsl` mixes what a pixel shows, lit and shaded, with the
  fog's color by its distance from the camera, in a straight line from `near` to `far`, in linear
  light; from where the camera is (`View3d::distance`), or ahead of an orthographic one. A view
  without a sky draws no background, so a game clears the canvas to the fog's color itself.
  `lights_of` leaves out a light all of whose reach is `far` or more away, uncounted, so
  `shadows_of` gives it no maps. Fog that can't be (`Fog::problem`) is none. In scenes it's a
  view's `fog` (`View3dDef::fog`), written only if it has one. Native libraries can't give a view
  fog: the ABI's view is unchanged.
- Sky (`Sky`, a `View3d`'s `sky`): a gradient by the height of the way each pixel looks, from
  `horizon` to `zenith` or `ground` over `band` radians, in a straight line in the height, in
  linear light (`Sky::color_at`); `Sky::flat` is one color. It's one triangle over the view
  (`mesh.wgsl`'s `vs_sky`, placed through the view's crop, `sky_crop`), at the far depth, drawn
  by its own pipeline, which reads only the view's group, after the view's opaque runs and before
  its blended ones (`MeshRenderer::draw`), passing only where the depth is as cleared; so a view
  with a sky covers its rect, and is drawn with nothing in it. The uniform carries its colors,
  `1 / sin(band)` (`Sky::steepness`), and the camera's right and up scaled by its angle, from
  which each corner carries the way it looks; an orthographic camera's sky is one color. Fog fades
  into it (`fades_into`), the way each surface is seen, instead of into its color. It's a draw
  call of its own in `draw_call_count`. A sky that can't be (`Sky::problem`, a band not above 0)
  is none. In scenes it's a view's `sky` (`View3dDef::sky`), written with its horizon, and the
  rest unless it's the horizon's or a quarter turn. Native libraries can't give a view one.
- `src/graphics/font.rs`: `Font` and `TextLayout`. Built-in and bitmap fonts have their glyphs in
  one texture (`Metrics`; bitmap glyphs cut from a font file's PNG, as wide as their ink).
  TrueType and OpenType fonts (`Face`, `FaceSize`) rasterize a glyph with `ab_glyph` the first time
  layout asks for it, into pages written by `GlyphPages` (`renderer.rs`), whose writes land before
  the frame's draw calls. `scaled` rasterizes a vector font again at the new size, keeping the
  last few sizes. Layout rounds the pen to whole pixels and kerns by the `kern` table. The
  built-in font is drawn as rows of `#` in `default_font.rs`.
- `src/gui.rs`: egui over games (`wingine::gui` re-exports egui). egui blends in sRGB, so it
  renders through a non-sRGB view of the frame and samples engine textures through non-sRGB views
  of them, which is why textures keep their format without sRGB among their view formats.
  egui expects premultiplied colors, so `Context::gui_texture` shows a copy drawn through the quad
  pipeline, as `GuiCanvas` does.
  Events egui wants don't reach `Input`. Tab reaches egui only while one of its widgets has focus
  (`Gui::hears_key`), since egui focuses its first widget on Tab even when none has focus, which
  would take Tab, and the keys after it, from a game showing an interface. `GuiCanvas` renders a
  `Canvas` into a texture egui shows. Simulations turn on AccessKit, so tests find widgets by
  label.
- `src/audio.rs`: `Audio`, `Sound` and `Voice`. kira's state catches up only when its mixer
  renders, so `Audio` keeps what the game asked of each voice (`Playing`), and a voice is over as
  soon as it's stopped. A `Sound` is a file's, or made of samples a game works out
  (`Sound::from_samples`, Luau's `audio.make`, which takes a `buffer` of 32-bit floats),
  checked to be numbers, since one NaN would fill the mix. Headless, and without a sound device, audio plays to a silent mixer
  advanced by game time, so tests can check what's playing.
- `src/music/`: songs (`*.song.luau`), whose module docs are the format's reference.
  - `SongFile` (`file.rs`) is the plain data the editor edits, read through `scene::file`'s
    helpers and written canonically by `scene::writer`'s `Writer`, with a format version of its
    own; `Song::new` checks its names and resolves them to indices, so a file can name what it
    lacks and say so only when it's played.
  - `synth.rs` renders a note: an oscillator, an envelope, a pitch sweep and vibrato, and a
    state-variable filter. `sequencer.rs` walks sections and patterns in control steps of 32
    frames from the song's start, placing events by its tempo's segments, so what it renders
    never depends on how the mixer cuts its buffers. All of it is `libm`'s math, so hashes of a
    note and a bake pinned in tests check platforms agree.
  - `render.rs` bakes a song to a 16-bit WAV, a loop's tails mixed into its start
    (`wingine bake`). `live.rs` plays one on the audio thread (`Audio::play_song`,
    `PlayingSong`): commands in and reports out over bounded channels, nothing allocated there,
    the sequencer made at the mixer's rate on its first `process`. A live song and a bake at that
    rate are the same samples.
  - Events and positions reach the game in `Audio::advance`, after each update, so an update
    hears what the mixer played to the last. A Luau test resumes after its frame's advance, so it
    hears a song's events an update before the game does.
- `src/input.rs` and `gamepads.rs`: `Input`, keys, mouse, gamepads and actions. A system's repeat
  of a held key is a repeat, not a press. `text` is what keys typed; input methods are off unless a
  game takes one, since they change which key events arrive. Actions keep their controls in the
  order bound, so the first names them; a mouse button is a control by a name of its own
  (`MouseLeft`), since `Left` is the arrow key. Sticks and triggers go through a radial dead zone
  and hold from 0.5 and release below 0.4. Gamepads are numbered from 1 and polled only while the
  window is focused. A captured mouse (`set_mouse_captured`) is the game's wish until the window
  grants it after the frame (`State::capture_mouse`), locking the cursor or else confining it,
  and goes with the window's focus. Its motion is winit's `DeviceEvent::MouseMotion`, counted
  only while it's captured, when egui hears nothing of the mouse but its buttons' releases.
- `src/particles.rs`: `Particles`, an emitter with `ParticleSettings` that keeps no time:
  `advance` steps it in parts of at most 1/60 s, spreading particles started as its origin moved
  along the way. Each emitter has a xorshift seeded by its settings or its owner (a scene node's
  path, a game's count of emitters), and starting particles uses `libm`, so a hash pinned in its
  tests checks Windows and Linux agree. `Canvas::draw_particles` draws them, culling each, taking
  `sin_cos` only for turned ones. Attached particles are in the emitter's coordinates.
  `particles/view.rs`'s `Particles3d` is the same in a 3D view, sharing the 2D one's steps,
  generator and colors and scales over a life, and nothing of the 2D one changes for it: it sends
  particles along its origin's `+z` within a cone, evenly over a sphere's cap, and with `bounce`
  sends them back off the plane each started on. `Canvas3d::draw_particles` pushes an instance
  for each, a billboard facing the camera, a streak along its velocity or a mesh turned about
  its own axis; an unlit one's colors ride its emission, its tint keeping only its alpha, so no
  shader knows it's a particle.
- `src/menu.rs`: `Menu`, a selection that holds no items: each `update` takes them as they are.
  Directions go by where items are, so lists, rows and ragged grids move as they look.
- `src/physics.rs`, `physics/world2d.rs` and `physics/world3d.rs`: `World` and `World3d`, by
  Rapier, sharing `BodyKind` and `Material`.
  - Both have fixed, dynamic and kinematic bodies (kinematic ones never asleep, since Rapier
    doesn't wake one a body leaves) and `walk` for characters, flat-footed boxes in 2D and
    cylinders in 3D, that step and slope. `step` splits long steps and moves kinematic bodies a
    share each. Rapier forgets new bodies in the step after `detect_collisions`, so `refresh`
    puts them in the query tree itself, and queries skip disabled colliders it leaves there.
  - 2D also has fixed rects without bodies, solid, edges or sensors, and tiles (a compound of
    boxes); every collider is one `ColliderId`. `slide` casts the box itself rather than using
    Rapier's `KinematicCharacterController`, whose rounded box turns normals so a box pushed at
    a wall creeps along it; `true_normal` snaps GJK's normals to the sides. A 2D character is
    carried by what it stands on as far as that moved in the last step (3D's only follows it
    down). Its units are pixels, so `set_length_unit` tunes Rapier, and Luau's densities are of
    square metres.
  - A query goes through what its `Passing` says: colliders or bodies by id, and by tags, bits
    each collider holds as its `user_data` (`set_tags`), which 3D's `set_shape` carries over
    as it makes a body a new collider. Luau's worlds give a tag a bit the first time a query's
    `except_tagged` names it, once they've followed their nodes, and every body or collider of
    a node with it that bit, and a body its node's bits as it's added (`Tags`); a tag past the
    128th is asked of each body's node by name, the same answer, slower.
  - Queries never find sensors. Unit tests pin hashes so CI checks Windows and Linux agree;
    scene nodes' 2D transforms take their sines from `libm` (`Affine::from_parts_exact`) so
    colliders placed by them do too.
  - Luau's worlds (`src/lua/physics.rs`, and `physics/world2d.rs` beside it) are of scene nodes,
    followed before each move, walk, step or query to where scripts moved them, unless the
    scene's `revision` (bumped by every `get_mut`, but not by particles) hasn't changed; they give
    nodes back in the order they were added. Tile maps are solid by `Tilemap::solid_rects`,
    reshaped only when the map's `solid_revision` changes and the boxes differ.
  - The 2D world then reads every collider's node again, placed as they are by layout. The 3D
    world reads only the bodies of nodes placed anew and of what's inside them: its scene keeps
    which nodes change (`Scene::keep_changes`, a list of the last few thousand, which a world
    left further behind than that makes up for by reading every body), and the world keeps a
    `Holder` for each node with bodies inside, with how it was placed and its parent, so a
    change where no body is costs a lookup and a body that goes is counted out though its node
    is gone. Both ways tell Rapier the same things in the same order, bodies in the order added,
    which `tests/runtime.rs` pins; debug builds check after each follow that reading every body
    would find nothing more (`check_followed`). `walk` and `step` follow what they moved before
    they return (`follow_moved`), which `is_resting` and `velocity`, following nothing, show.
  - The 3D world makes each model's or mesh's shape once for each way it's shaped and stretched
    (`Shapes`, keyed by its `Arc`, held), a `Shape3d::Made` that every body of a copy shares,
    posed where its node is: a fixed body too, its node's whole transform split into a pose and
    a shaping (`split`), unless a parent stretches it across how it's turned, when its shape is
    made where it is in the world, as every fixed body's once was. One turned by quarter turns
    is only moved, its shaping turning its shape, since a ray turned into each of two pieces'
    coordinates rounds otherwise in each, and slipped through the seam between them. Only
    dynamic bodies are weighed (`weighed`): weighing a mesh costs a twentieth of making it, and
    was done twice. Posing changed fixed bodies' answers in their last bits, and where a falling
    body lands by up to millimetres:
    `worlds_of_copies_of_models_end_the_same_to_the_bit_on_every_platform` says what it was
    measured against.
  - A step up to 5% past a sixtieth is taken whole (`physics::parts`), so a 59.94 Hz display
    doesn't double Rapier's work.
- `src/headless.rs`: `Headless` renders a canvas to an image; `Simulation` runs a whole `Game` at
  60 fps with simulated input, egui included, and screenshots. Key repeats run in game time, and
  typing is as on a US keyboard. All simulations in a process share one GPU device, held by one
  thread at a time (a reentrant lease): lavapipe crashes with a device per thread and WARP
  renders wrongly with a shared one. Frames render only for screenshots unless asked. `Image`
  compares frames (`compare`, `diff`), as screenshot tests and games' tests do.
- `src/lua.rs`: `Scripts`, a sandboxed Luau VM with tasks (`wait`, `wait_until`, `spawn`), and
  the conversions and userdata of engine types. Its built-ins are the start of the API module
  (`Scripts::api`), which its own `require` answers when a game gives it no other. `wait` tells the scheduler its seconds before it
  yields, and the scheduler counts them down in `f64`, as `wait` once did in Luau, resuming the
  task only once they've passed; Luau tests' runner does the same. `exact` holds `libm`'s math,
  the same bits on every platform, for what must replay; `math`'s are builtins about five times
  quicker, for show. Scripts compile at optimization level 2.
- `src/save.rs`: `Saves`, files of plain values in `Wingine/<identity>` among the system's
  applications' data, each written to a file of its own then renamed over the old. Simulations
  keep theirs in a temporary folder, so tests never touch the player's.
- `src/folders.rs`: `Folder`, a folder the player picked, reached by paths relative to it that
  can't leave it (`resolve` follows every link on the way and checks where it led), and
  `Folders`, those picked and the dialog for picking another, shared by clones. Scripts are
  sandboxed, so a `Folder` never tells them its path. A dialog asked for (`Folders::pick`, a
  ticket) is opened by the window after the frame (`State::pick_folder`, `rfd`'s
  `AsyncFileDialog` over the window, polled each frame, waking it when it closes), and its
  answer waits for the ticket's owner; a simulation opens none, and `Simulation::pick` answers
  with a copy of a folder, so tests never touch the player's files. The folders picked are
  remembered in `.folders.luau` beside the saves, which isn't a save; simulations remember them
  only in memory, so a scripted run started from copies of a player's saves never reaches their
  folders.
- `src/icon.rs`: `WindowIcon`, scaled up by whole numbers so systems only shrink it.
- Also: `tilemap.rs` (`Tilemap` and `Tileset`; outside the map is solid; `solid_rects` merges
  solid cells into boxes for physics), `grid.rs` (`GridMover`,
  carrying leftover time between steps), `math.rs` (`Rect`, `Affine`, `Affine3`, `Bounds3`),
  `watch.rs` (`Watcher`, hashing a folder's files in a thread), `script.rs` (scripted runs).

### Luau runtime

- `src/runtime/`: games written entirely in Luau, from a folder with `main.luau` and an optional
  `conf.luau`. `LuauGame` runs one; its API is a module, `require("@wingine")`, not globals:
  `Scripts::api`'s built-ins, and the tables `graphics`, `input`, `audio`, `tilemap`, `grid`,
  `physics`, `scene`, `ui`, `save`, `files` and `window`, a file each, frozen as the first script
  loads. `files.pick_folder` waits in Luau, over a ticket, as a function that waits does.
  Requirers answer `@wingine` first (`lua::ApiModule`, through `to_alias_override`), so a game's
  `.luaurc` alias of it, for luau-lsp, is never followed. The module docs are the reference for
  script authors, and `wingine.d.luau` declares the API's types for luau-lsp, every one exported,
  the module as `Wingine`, which `wingine.luau` returns for luau-lsp to read through that alias;
  `tests/runtime.rs` checks it declares exactly what the module and its tables hold.
- `Textures` caches images, fonts and models by path, so each is read once.
- `ui.menu`, `audio.play` and `audio.play_song` hand out userdata holding the `Lent`, so they're
  documented with their tables. A performance's `on`, `off` and `wait_for` are Luau
  (`SONG_LISTENERS`, in `runtime/audio.rs`) over its `events`, reached through field getters from
  a table in the VM's app data; a listener is a task reading each update's events, starting each
  call as a task of its own. A game's `gui(ui)` gets egui's `Ui` borrowed through `Lua::scope`, so a `Ui` kept
  past its call fails rather than dangle. `Ui`'s methods are functions that borrow the `Ui`
  they're called on, not mlua mutable methods, which refuse to run inside themselves, so a
  container can hold another of its kind. A canvas (`ui:canvas`) lends its function a `Paint`
  the same way: egui's painter clipped to it, which takes positions from its corner, and a table
  of what the pointer did, read from its `Response` before it's painted. `paint:bounds()` is its
  rect mapped to canvas pixels through the `Screen`, as a simulation's `gui_rects` are, so
  `draw`, after `gui`, can draw behind it.
- Text in a 3D view (`Canvas3d::draw_text`, Luau's `graphics.draw_text_3d`) is laid out by the
  font as 2D text is, and each glyph pushed as a billboard of the font's page, all with the
  facing the first would have (`billboard_axes_at`), so a sign turns as one.
- `LuauGame::reload` loads a whole new game in place, after the old one's `reloading` returns
  plain data for the new one's `window.reloaded()`. A game that fails to load becomes a stopped
  game holding that data for the next reload. `--watch` reloads on changes.
- `native.rs`: native libraries, registered once per process and never unloaded, each game
  getting state of its own. `crates/wingine-abi` is the C ABI (`ABI_MAJOR`, `ABI_MINOR`), and
  `crates/wingine-native` the crate libraries are written with, which never depends on `wingine`.
- `export.rs`: `wingine export`, the CLI's executable copied as the game, beside `game/` and
  `licenses/`; on Windows it's patched to open no console and given the game's icon.
- `testing.rs`: Luau games' own tests in `*.test.luau`, each against a fresh `Simulation`. A test
  file's environment isn't Luau's "safe" one, so it reads the game's globals as they are. A run
  shares one `Reuse` between its games: `require`'s lookups and each script's bytecode, by its
  source (`lua::Bytecode`), never anything a script made.
- `fs_require.rs`: `require` off the web, navigating as mlua's `FsRequirer` does (a test holds it
  to mlua's answers), but remembering whether each path is a file or folder (`Lookups`), once a
  load in a window and once a run in tests: asking the file system for every `require` was a third
  of a large game's load. `require.rs` is the web's, over the page's files.
- `src/main.rs`: the `wingine` executable: run a game, `--watch`, `test`, `export`, `bake`, and
  with no arguments, the `game` folder beside it.

### Web

- `web/`: a Cargo workspace of its own, with its own `Cargo.lock`, whose root patches in mlua's
  master branch: a patch counts only in the root of the workspace being built, and Cargo can't
  take mlua from two places in one, so the engine's workspace keeps the release. Experimental:
  CI neither builds nor lints it, and `cargo fmt` at the root doesn't reach it.
- `wingine-web` is what a game's module needs beside the engine: WASI libc's allocator as Rust's,
  the C++ constructors run once from the module's start function, the console log, `start`, and
  `embed_game` for build scripts (host-only), which `game!` reads back. `player` is the module of
  a game without native libraries of its own; a game with some builds its own module, in a
  workspace with the same patch. `build.sh` builds a module into a page with `page/`, sourcing
  `wasm-env.sh` for WASI SDK's compilers and flags.

### Scenes

- `src/scene/`: `file.rs` reads scene, tileset and font files by running them as Luau with nothing
  in scope and reading the plain values back, errors naming the node; `writer.rs` writes the
  canonical layout. The module docs are the formats' reference.
- `Scene::new` expands instances, applying overrides by writing the node back to values, setting
  the fields and reading it again, so overrides are checked as files are; then builds live nodes
  through `Assets`. `NodeId`s name their scene and have generations, so an id never reaches
  another scene's node or one added since its own was removed.
- Layout walks down from the root: anchors and stretch place nodes in their parent's rect, stacks
  and panels place their children and fit what they hold, and 3D views' children are placed in
  3D (`placement3`). `Scene::pose` applies `Clip`s, finding every track's node before changing any.
- Drawing nodes carry a `blend`, which `draw_tree` sets before each, and `draw_from` restores the
  canvas's after. Particles nodes are laid out as areas (their rect is where particles start, 0 by
  0 by default) and keep their emitter's state; scenes keep no time, so games call
  `advance_particles`, which lays emitters out in one walk down through the nodes holding them.
  Particle settings are read by the file reader for files and for Luau alike
  (`file::particle_settings`, through `plain`), so both take the same fields with the same errors.
  A particles node in a view holds an `Emitter::View`, a `Particles3d` placed by
  `transform_3d`; its file's fields only a view's emitter has are a `ViewParticlesDef`, whose
  `gravity_z` is a third number of the node's `gravity`, and `check_3d` refuses them outside one.
- Scripts hold nodes as `lua::SceneNode`, an `Rc<RefCell<Scene>>` and a `NodeId`.

### Editor

- `crates/wingine-editor/`: an `Editor` `Game` whose `gui` is the whole interface, laid out like
  Godot's. It edits `SceneFile`s, never live scenes: a `Document` holds the file, its undo
  history of whole-file snapshots (a change lasts until a frame with no button held and no field
  focused, so a drag is one), the selection as a `NodePath`, and a preview `Scene` built through
  `AssetCache`, which rereads a file only when it changes. Painting tiles, and moving, turning or
  stretching a node, change the preview as well as the file rather than building it again, and a
  preview built again keeps its particles going. `edit.rs`, `clips.rs`, `layout.rs` and
  `instance.rs` are pure operations on files.
- The viewport renders the preview into a `GuiCanvas` and picks, paints tiles, resizes, and drags
  in 3D. The Animation panel keys what the clip it shows tracks. The Script screen checks scripts
  with luau-lsp (`lsp.rs`, with the engine's definitions). Play runs the editor's own executable
  with `--run`, watching the folder.
- It redraws on demand: threads (luau-lsp, the game played, the `Watcher`) wake it with a `Waker`.
  A rebuilt preview runs its particles a few seconds to show them; they animate, asking for frames,
  only while the selection holds a particles node (`Document::animate_particles`).
  Its tests (`crates/wingine-editor/tests/editor.rs`) click widgets by label; those that wait in
  real time run frames
  only when it wants them (`wait_for`), and `settle` checks it stops wanting them.
- The Music screen (`music.rs`) edits `SongFile`s: a `SongDocument` (`song_document.rs`) holds
  one, what's selected in it by name, and the song it makes, made again only once the file
  changes. `history.rs`'s `History<S>`, undo of whole-file snapshots, is shared with `Document`.
  `songs.rs` holds the pure edits, which keep names in step as things are renamed. A song changed
  as it plays plays again from where it was once the change is made, and the screen asks for
  frames only while one plays (`MusicEditor::keep_playing`).

### Examples

In `examples/`. Each Luau game keeps its state in one global table named for it, which its
`.luaurc` names for luau-lsp; `examples/.luaurc` aliases `wingine` for all of them. Scripts
require the API as `local wg = require("@wingine")`. Scene files in the examples are in the
canonical layout, which each
game's tests check: after editing one, write it again with `SceneFile::write`.

- `rpg/`: a top-down RPG: maps are scenes over `world.tileset.luau`, conversations and events in
  `story.luau`, turn-based battles (tests replace `battle.roll`), a shop, a bag, saves after
  every event, a controls screen, and a title whose name is a bitmap font (`title.png`).
- `shmup/`: a bullet hell at 60 updates a second. Bullets go through one interface, in Luau
  (`bullets.luau`) and in the `danmaku` native library (`native-danmaku/`), which follows the Luau
  step for step in `f64`, so both produce the same bullets to the bit: change them together. Only
  the game's seed may feed `math.random`.
- `pong/`: Pong in Luau, with Luau tests and an icon; its match hash is pinned in `tests/pong.rs`.
- `garden/`, `orchard/`, `cellar/`, `course/`, `quarry/`, `bonfire/`, `lettering/`: a scene with
  2D physics seen from above, a rigged character, a 2.5D cellar whose candles smoke in 3D, a 3D
  course with physics, a level seen from the side with 2D bodies (both with their run's hash
  pinned), a night of particles and blend modes with fireworks, and a type specimen of the
  engine's fonts.
- `bandstand/`: a rhythm game played to a song played live: layers faded in as the player keeps
  time, the chorus at the next bar after a long streak, quicker each verse, the lights flashing
  on the drums' notes and confetti at a marker. Presses are judged against beats counted from the
  song's events and its `position()`, the mixer's time rather than the game's. Its song is in the
  canonical layout, which its tests check: after editing it by hand, save it in the editor.
- `sprites.rs`, `shapes.rs`, `meshes.rs`, `gui.rs`: examples in Rust; `native-particles/` a native
  library that `tests/native.rs` builds.
- Assets: art drawn for the examples is under the project's licenses; Kenney's packs are CC0 and
  Hack ships with its license. `models/props.blend` is the source of the Blender props: export
  again and copy them to `cellar/models` after changing it. Build or save props in a Blender of
  their own (`blender --background --factory-startup --python ...`), and never write a `.blend`
  from a session holding someone's unsaved work.

### Tests, benches and docs

- `tests/`: a file per area (`runtime`, `scene`, `screenshots`, `simulation`, `testing`, `gui`,
  `export`, `native`) and per example game. Games' tests check their Luau state through
  `Simulation::eval` and `exec`, and run their own Luau tests through `runtime::testing::run`.
- `benches/drawing.rs`: what drawing sprites and text costs from Rust, Luau and native libraries;
  `physics.rs`, what Luau worlds' steps, tiles, queries and walks cost, what a 3D world of 2,000
  bodies costs as nodes without bodies move, rigs are posed and a room comes and goes, what
  adding a hundred copies of a model costs, and Rust steps of a sixtieth and a 59.94 Hz frame; `scenes.rs`, what posing, adding, loading instanced scenes and
  moving particles in stacks cost; `scripts.rs`, what waiting tasks cost; `music.rs`, what a second
  of a song costs to render at 48 kHz, the bandstand's and songs of 64 notes at once of each
  wave, as the audio thread renders one played live. CI doesn't run them.
  Their numbers shift by a third with unrelated changes, and by a quarter between runs of one
  build, so run each a few times, and compare builds by running them in turn. A build started
  before an edit is newer than it, so touch the sources before rebuilding to measure.
- `docs/`: the user docs, restating the module docs for game authors, with `docs/api/` the API
  reference. Change the docs, the module docs and `wingine.d.luau` together; `docs/editor.md`
  follows the editor. Check docs' examples by running them headlessly.
- `README.md`: a screenshot of each example in `docs/screenshots/`, taken headlessly at 640x360
  (768x432 for `sprites`, 960x720 for `shmup`, 960x540 for `course` and `quarry`, 1280x720 for the
  editor with the garden open and its pond selected); retake them when an example changes how it
  looks. The shmup's numbers come from its stress test and benchmark, measured on a Ryzen 7 5800X
  with an RTX 3080; measure again on that machine after changes that could move them.

## Conventions

- Drawing coordinates are canvas pixels, origin top-left, `y` down. The canvas is the window's
  logical size (`Resolution::Window`), or a fixed size scaled up by whole numbers with bars
  (`Resolution::Fixed`, for pixel art), or by `Scaling::Fit` or `Stretch`.
- `Canvas::set_camera` switches to world coordinates, applied on the CPU as quads are recorded,
  so switching costs nothing. Draw the world with a camera, then `set_camera(None)` for the HUD.
- Tiles are `[column, row]` (`[i32; 2]`), the map's top-left at the world origin. A `GridMover`
  occupies its destination from the start of a step, and asks a `blocked` closure where it may go.
- Text positions are the top-left of the first line, rounded to whole pixels; measured widths run
  to the edge of the ink, so centering is exact.
- `Color`s are sRGB, as authored; rendering and blending are linear, and textures are uploaded as
  sRGB. Textures are sampled nearest-neighbor.
- Recording a quad runs for every sprite, so keep it free of math-library calls (`f32::round`,
  `powf` are calls on baseline x86-64, each costing as much as the rest of the quad). Measure
  changes to it with `cargo bench`.
- Luau reaches game state only through registered functions, which share it through
  `Rc<RefCell<_>>`, so never hold a borrow while scripts run. A function that waits is a Rust
  function that starts the action plus Luau that `wait_until`s it's done.
- Script errors don't stop the game: a failing task stops and `update` returns the error to log.
- Volumes are linear (1 as recorded). Failing to play a sound effect logs a warning; loading and
  music return errors.
- Keys are physical, named for US QWERTY. Games bind named actions and query those. The mouse is
  in canvas pixels; `Camera::canvas_to_world` converts it. A captured mouse's motion is in the
  mouse's own units, which differ between mice and systems, so games scale it by a sensitivity.
- egui draws over the whole frame at the output's resolution, in logical window pixels. Tests find
  widgets by label, so give one whose label egui doesn't know a label of its own
  (`Response::widget_info`). egui sizes a new window or menu in its first frame and shows it in
  the next, so tests run frames before looking for them.
- wgpu and winit types stay `pub(crate)`. `wingine::lua` re-exports mlua and `wingine::gui` egui,
  whose types are their APIs. Engine types scripts use implement `FromLua`/`IntoLua` or
  `UserData` in `src/lua.rs`.
- The Luau runtime lends its functions the `Context` while the game loads, updates and lays out
  its interface, and the `Canvas` while it draws (`Lent`, `lent.context(...)`,
  `lent.canvas(...)`); called at other times, they fail with a Lua error. Each update runs
  `update(dt)` then resumes tasks; each frame runs its updates, then `gui(ui)`, then `draw()`.
  Presses last until an update has seen them. Conversions through mlua's `Value` cost more than
  drawing, so code that draws a lot passes colors as numbers.
- Luau resolves a global's fields named in a chunk when it loads, if the global exists then, so a
  chunk that changes a field and reads it back sees the old value. Games create their global table
  after requiring their modules, and tests read state back in a chunk of its own
  (`Simulation::eval`). The API being a module costs a script a lookup or two where a global's
  field cost none, some 2.7 nanoseconds each, which no game measured has shown.
- Native libraries reach the engine only through the ABI, getting the same lent services scripts
  do. Hooks run in the order libraries are listed. Drawing many sprites goes through one
  `draw_sprites` call, since each call from Luau into a library costs far more than a sprite.
  Errors cross as strings (`set_error`, `Status::ERROR`), and panics never unwind into the engine.
  Change the ABI only by adding to the end of `Host` and bumping `ABI_MINOR`; anything else bumps
  `ABI_MAJOR`.

## Commands

```bash
cargo build
cargo test
cargo clippy --all-targets
cargo clippy -p wingine --no-default-features --all-targets
cargo fmt
cargo run -- examples/pong
cargo run -- --watch examples/pong
cargo run -- test examples/pong
cargo run --release -- export examples/pong target/export/pong
cargo run -- bake examples/bandstand/bandstand.song.luau target/bandstand.wav
cargo run --release -- examples/shmup
cargo run -p wingine-editor -- examples/garden
cargo run --example sprites
cargo build --release -p native-danmaku
cargo bench
web/build.sh --game examples/pong
cargo fmt --manifest-path web/Cargo.toml --all
```

The README's examples table lists how to run each example.

Run `cargo fmt` and `cargo clippy --all-targets` before considering a change finished, and keep
both clean with the `egui` feature off too. After changing code that differs on the web, lint it
there too, with `web/wasm-env.sh` sourced:
`cargo clippy --manifest-path web/Cargo.toml --target wasm32-unknown-unknown`. CI (`.github/workflows/ci.yml`) runs fmt, clippy with
`-D warnings` with and without default features, and `cargo test` on Linux and Windows.
`release.yml` builds the runtime and editor for Windows and Linux (on the oldest Ubuntu runner,
for glibc; Windows links the C runtime statically), and a `v*` tag publishes them. Each archive
has `THIRD-PARTY-NOTICES.txt`, written by `cargo about` (pinned in `release.yml`) with `about.toml`
and `about.hbs`. It fails on a license `about.toml` doesn't accept, so a new dependency may need
its license added there; its clarifications match license files by checksum, which upgrading
those crates can change.

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
  screenshot shots/after.png
  ```

  ```bash
  WINGINE_SCRIPT=path/to/check.txt cargo run -- examples/pong
  ```

  Mouse positions are canvas pixels; screenshots are window-sized, so with a fixed resolution
  divide screenshot coordinates by the scale (and subtract the bars) to aim a click.
  `benchmark 10s results.csv` renders every frame for that long and reports frame times; benchmark
  release builds, since Luau and the engine are far slower in debug.
- **As tests:** drive the game with `Simulation` and assert on its state, as
  `examples/sprites.rs` does; for a Luau game, use `eval` and `exec`, as `tests/pong.rs` does, or
  write its tests in Luau, in `*.test.luau` files that `cargo run -- test path/to/game` runs.
  Prefer this for anything worth keeping, so CI catches regressions.

Windowing itself (focus, minimizing, real resizes, DPI, capturing the mouse) still needs a real
window: a simulation only notes that the mouse is captured, and takes its motion from
`move_mouse_by`. Audio in these
runs is silent; `cargo test -- --ignored` runs a check that plays (muted) through the real sound
device.

## Screenshot tests

Rendering changes are covered by screenshot tests: each renders a scene with `Headless` and
compares it with a reference PNG in `tests/screenshots/` (`Image::compare`), allowing a
per-channel difference of 2 so references pass on any GPU. Add a test there when adding a drawing
feature.

- A mismatch writes the rendered frame and a diff (mismatched pixels in magenta) to
  `target/tmp/screenshots/`; CI uploads them, and the examples' `.test-output` folders, as an
  artifact.
- `UPDATE_SCREENSHOTS=1` creates or rewrites missing or mismatched references. Look at every
  changed PNG before committing it.
- Where a pixel's middle lands on the edge between two texels, GPUs pick either, so 2D scenes keep
  sprites on whole pixels at whole scales; `meshes_textured` lets a few pixels differ
  (`assert_screenshot_allowing`), since textures in perspective can't.
- CI runners have no GPU: Linux uses lavapipe (`mesa-vulkan-drivers`) and Windows uses WARP. To
  reproduce WARP on Windows: `WGPU_BACKEND=dx12 WGPU_ADAPTER_NAME="Microsoft Basic Render Driver"
  WGPU_DX12_COMPILER=fxc`. Always set `WGPU_DX12_COMPILER=fxc` on WARP: the Vulkan SDK's
  `dxcompiler.dll` crashes it if it's on `PATH`.
