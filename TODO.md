# TODO

What's left from the code review of November 2026, after the crash and hang fixes in `6376aff`.
Places are named by file and function, since line numbers drift. Items marked (checked) were
confirmed by reading the code; the rest were traced by a reviewer but not checked again.

## Correctness

### Rendering and scenes

- [ ] **egui shows engine textures as if premultiplied** (checked). `Context::gui_texture`
  (`src/gui.rs`) registers straight-alpha textures, but egui-wgpu 0.37 blends `One,
  OneMinusSrcAlpha` without multiplying by alpha, so partly transparent texels are too bright and
  fully transparent ones with color add it. Shows in the editor's inspector and tiles dock on the
  bonfire, shmup and orchard sheets. Render the texture once through the quad pipeline into a
  render target (as `GuiCanvas` does, which is already right) and register that.
- [ ] **Panels draw in the blend mode left by the node before** (checked). `Kind::blend`
  (`src/scene/mod.rs`) has no `Panel` arm, so `draw_tree` doesn't set one, and a panel after an
  additive sibling draws additively. Give panels `Some(Blend::Normal)`, or restore the saved blend
  before drawing one.
- [ ] **An override of `props = {}` wipes the node's props** (checked). An empty table reads as
  `Value::List`, missing the `("props", Value::Map(_))` merge arm in the override code
  (`src/scene/file.rs`), so it replaces them. Treat an empty list as an empty map there.
- [ ] **A fixed resolution premultiplies twice** (checked). `Screen::render`
  (`src/graphics/screen.rs`) draws the already-premultiplied target through `quad.wgsl`, which
  multiplies by alpha again, so translucent clears render darker than at `Resolution::Window`.
  Present with a pipeline that doesn't premultiply, or force the target's alpha to 1.
- [ ] **Billboards are culled under steep perspective cameras.** `view3d.rs` draws them
  single-sided, but they face against the camera's horizontal look direction, not its position,
  so ones below a camera looking down show their backs and vanish. Draw them without culling.
- [ ] **`Ray::hit_box` reports a hit at 0 for a NaN ray.** `f32::max`/`min` drop NaN
  (`src/graphics/camera3d.rs`); Luau's `graphics.hit_box` doesn't validate its input. Return
  `None` for a NaN origin or direction.
- [ ] **Text wraps a pixel off.** `Font::lay_out` (`src/graphics/font.rs`) rounds the pen before a
  word and inside it separately for the fit check, but rounds their sum when placing, so a word
  can end 1px past `max_width`, or wrap when it fits. Measure from the actual pen.
- [ ] **Words without ink lose their advance.** The `word_ink == 0.0` `continue` in `lay_out`
  skips a word of U+00A0, U+2003 or U+3000 without adding its advance to `spaces`.
- [ ] **A `#` in a sheet name breaks sheet overrides.** Sheets are keyed `file#sheet` and split
  with `rsplit_once('#')` (`src/scene/mod.rs`). Key them by `(file, sheet)`.
- [ ] **`Scene::get` accepts another scene's `NodeId`.** Two fresh scenes share generations, so an
  id from one resolves to an unrelated node in the other, despite the docs. Add a scene id to
  `NodeId` or correct the docs. Generations also wrap (`wrapping_add`).
- [ ] **A top-level node placed in 3D loses its placement on round-trip.** In a prop file meant
  for instancing into views, a plain node with default 3D fields reads as `placement3 =
  Some(default)`, but the writer (`src/scene/writer.rs`) drops default 3D fields, so it reads
  back as 2D. Pass "parent is 3D" down the writer and always write `z` when it's otherwise lost.
- [ ] **`GuiCanvas` sizes aren't checked against the GPU's limit.** `GuiCanvas::render`
  (`src/gui.rs`) creates a render target at whatever size it began, which panics past 8192 px.
  Clamp it, or use `gpu::texture_size_problem`.

### Editor

- [ ] **Hovering a keyword redraws forever** (checked). luau-lsp answers keywords with null;
  `poll` sets `contents = None`, and `show_hover` (`script_editor.rs`) asks again the same frame.
  Remember the target was answered, for example `contents: Option<Option<String>>`.
- [ ] **Undo keeps the selection by index, landing on another node.** Duplicate, then undo, and
  the next sibling is selected, so Delete deletes it (`Document::changed`, `document.rs`). The
  Animation panel's selected key (`animation.rs`) survives undo the same way. Keep the selection
  in each snapshot, or find it again by name path.
- [ ] **Showing a node can rewrite its file.** egui 0.37's `DragValue::range` clamps values as it
  shows them, so a node with `anchor = { 1.5, 0 }`, a large `limit`, a `font_scale` outside
  0.1..64 or a clip `duration` under 0.01 is changed, marked dirty and given an undo entry just by
  selecting it (`docks/inspector.rs`, `docks/animation.rs`). Use
  `.clamp_existing_to_range(false)`.
- [ ] **A game run with Play keeps every log line in memory** (checked). The `--run` child
  installs the editor's `Logger` (`main.rs`), which collects into `LOGGED`, drained only by the
  editor. Install a stderr-only logger for `--run`.
- [ ] **A tile stroke can paint later onto another layer.** `stroke` is cleared only in `paint`
  (`viewport.rs`), which runs only while a layer is selected, so Escape, Delete or Ctrl+Z
  mid-stroke leaves it to draw a line from its stale cell later. Clear it whenever `paint` isn't
  called.
- [ ] **The last lines a game prints can be lost as it exits.** Output is taken, then `exited()`
  checked, then `Play` dropped (`editor.rs`), dropping lines still in the reader threads. Drain
  until the channel disconnects, with a short timeout.

### Runtime and platform

- [ ] **`audio.set_volume(math.huge)` sends NaN or full scale to the device** (checked).
  `set_volume`, `set_sound_volume` and `set_music_volume` (`src/audio.rs`) pass infinity through
  `decibels`. Check them with `setting_problem`, as voices' setters do.
- [ ] **Scripted runs change a game's real saves** (checked). Without a `saves` command,
  `script::run` (`src/script.rs`) uses `config.save_folder`, though its docs promise a temporary
  folder. Always use a `TemporaryFolder`, seeded from it if wanted.
- [ ] **A Luau test's VM outlives its GPU lease** (checked). `Runner.lua` (`runtime/testing.rs`)
  clones the game's `Lua`, so the VM, and the GPU resources its globals hold, close after
  `drop(simulation)` released the lease. Drop the runner first, or hold a `WeakLua`.
- [ ] **Exports with identity `game` or `licenses` fail on Linux and macOS** (checked). With no
  `EXE_SUFFIX`, the executable's path is the `game/` or `licenses/` folder (`runtime/export.rs`).
  Rename or refuse those identities there.
- [ ] **Exports follow symlinks with no loop check.** `copy_game` uses `fs::metadata`, so `assets
  -> ..` recurses until paths are too long. Skip links, or track canonical folders.
- [ ] **Windows device names pass as save names.** `save::is_name` (`src/save.rs`) accepts `con`,
  `nul`, `aux`, `prn`, `com1`–`com9` and `lpt1`–`lpt9`, which name devices on Windows. Refuse them
  case-insensitively; it checks identities and library names too.
- [ ] **With `Redraw::OnDemand` and an `update_rate`, input waits for the next event** (latent).
  An event's frame within a step of the last runs no updates, and nothing asks for a frame when
  the next falls due (`src/app.rs`). Ask for one after the time left in the step.
- [ ] **Worlds that never step keep removed colliders.** Rapier's `ColliderSet::remove` adds to
  `removed_colliders`, emptied only by stepping (`src/physics/world2d.rs`), so a query-only world
  like the garden's grows by one per removed node. Run `detect_collisions` when it has no bodies.

## Performance

Measure before and after with `cargo bench` or the shmup's benchmark, as AGENTS.md asks.

- [ ] **Any `Tilemap::set` rebuilds the map's whole collider** (checked). `set` bumps `revision`
  even for an unchanged cell or a decorative layer (`src/tilemap.rs`), and Luau physics
  (`lua/physics/world2d.rs`, `follow`) reruns `solid_rects` and `set_tiles`, whose shape and
  position changes wake every body resting on it. Return early for an unchanged cell, keep a
  revision that changes only with solidity, skip `set_tiles` when the rects are equal, and skip
  `set_position` when the origin hasn't moved.
- [ ] **Physics steps split in two just past a sixtieth** (checked). `World::step` and
  `World3d::step` allow 0.1% over `STEP` before splitting, so a 59.94 Hz display or ordinary
  vsync jitter doubles Rapier's work for games without an `update_rate`. Allow a few percent.
- [ ] **Every Luau physics query re-reads every collider's node from the root.** `follow`
  (`lua/physics/world2d.rs`, `lua/physics.rs`) runs before each ray, overlap, move, walk and step,
  with `local()` walking to the root three times per body and 3D recomputing `exact_transform`.
  Skip it when the scene hasn't changed, and walk once in `local()`.
- [ ] **The editor rebuilds the preview every frame of a drag.** Each `set_node` marks it stale
  (`document.rs`), so `Scene::new` runs each frame, warming particles for seconds each time (about
  660 steps an emitter in the bonfire); the dragged subtree is cloned three times a frame
  (`viewport.rs`, `document.rs`) and the Inspector clones the selection every frame even idle.
  Patch transforms into the live preview, as tiles already are, and carry particle state over.
- [ ] **The script editor lays its text out every frame.** The layouter rebuilds and sorts every
  token cut (`script_editor.rs`), and `underlines` scans from the start of the text for each
  diagnostic, twice. Memoize the job by the text's hash and the diagnostics.
- [ ] **The editor walks the project every frame.** About ten tree walks a frame (`files_ending`,
  `model_files` in `editor.rs`), `Palette::load` while a tilemap is selected, `read_dir` while the
  Open dialog shows, and `Output::push` shifting a 5000-line `Vec` (`docks/output.rs`). Cache the
  lists on rescan, the palette by tileset, and use a `VecDeque`.
- [ ] **Text layout does each glyph twice.** `lay_out` runs `ink_width` over each word before
  placing it (`src/graphics/font.rs`), even without a wrap width, and vector fonts take a mutex
  and a SipHash lookup per glyph each time. Skip the pass without a wrap width, cache the
  `GlyphId`, and lock once per layout.
- [ ] **3D draws are instanced only when consecutive.** `chunk_by(MeshDraw::joins)`
  (`canvas.rs`, `renderer.rs`) so 100 two-part tables are 200 draws. Stable-sort each view's draws
  by mesh, texture and sidedness first; there's no blending in 3D.
- [ ] **`Scene::pose` repeats work per track.** `check_cells` clones a texture handle for every
  key of every cell track, and each number track recomputes the node's transform with libm
  (`src/scene/mod.rs`). Check only the cells set at `time`, and update each node once.
- [ ] **`Scene::add` builds a path string for every copied node**, though only particles nodes use
  it. Check the kind first.
- [ ] **`Scene::load` reads an instanced file again for each instance.** `FolderAssets::scene` has
  no cache, so 50 villagers are 50 reads and VMs. Cache `Rc<SceneFile>` by path.
- [ ] **Particles and fitting stacks redo layout each frame.** `advance_particles` calls `placed`
  per particles node, each walking to the root, and fitting stacks are measured then laid out
  again. Walk once from the top carrying `Placed`.
- [ ] **Native method calls cost two callbacks.** `__index` on native objects
  (`runtime/native.rs`) does a `HashMap` lookup and a `Table` clone per call. Keep the class's
  method table in `NativeObject`.
- [ ] **Waiting tasks resume every frame.** `wait` (`src/lua.rs`) yields and is resumed each
  update to check its time. Have the scheduler count it down and resume only when due, in `f64`,
  so results stay the same.
- [ ] **Physics allocates per cast and scans linearly.** `true_normal` builds a `Vec` per cast
  (up to about 12 a 2D walk); `nodes()`, ray hits and `walk` find nodes by linear scans. Use an
  iterator or array, and a map from handle to index.
- [ ] **Small allocations:** screenshot readback copies byte by byte (`src/headless.rs`; copy a
  row at a time), `Watcher` sorts by `file_name` allocating each comparison (`src/watch.rs`; use
  `sort_by_cached_key`), and `Renderer::render` allocates its `viewports` and `views` each frame
  with a 3D view (keep them on `Renderer`).
