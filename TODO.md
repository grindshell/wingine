# TODO

What's left from the code review of November 2026, after the crash and hang fixes in `6376aff`
and the correctness fixes after them. Places are named by file and function, since line numbers
drift. Items marked (checked) were confirmed by reading the code; the rest were traced by a
reviewer but not checked again.

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
- [ ] **Text layout does each glyph twice.** `lay_out` measures each word with `measure` before
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
