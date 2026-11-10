# TODO

What's left from the code review of November 2026, after the crash and hang fixes in `6376aff`
and the correctness fixes after them. Places are named by file and function, since line numbers
drift. Items marked (checked) were confirmed by reading the code; the rest were traced by a
reviewer but not checked again.

## Performance

Measure before and after with `cargo bench` (`--bench drawing`, `--bench physics` or
`--bench scenes`) or the shmup's benchmark, as AGENTS.md asks, on the forge.

- **Measured and left:** `Scene::pose` was said to repeat work per track (texture handles cloned
  for every cell key, each number track's libm transform). On the orchard's picker it takes about
  45 ns a track (0.43-0.47 us a pose in `--bench scenes`), and checking only the cell set at
  `time` and updating each node once measured no quicker. Should posing ever show in a profile,
  finding each track's node by its path, every pose, is the likelier cost.
- [ ] **Native method calls cost two callbacks.** `__index` on native objects
  (`runtime/native.rs`) does a `HashMap` lookup and a `Table` clone per call. Keep the class's
  method table in `NativeObject`.
- [ ] **Waiting tasks resume every frame.** `wait` (`src/lua.rs`) yields and is resumed each
  update to check its time. Have the scheduler count it down and resume only when due, in `f64`,
  so results stay the same.
- [ ] **Small allocations:** screenshot readback copies byte by byte (`src/headless.rs`; copy a
  row at a time), `Watcher` sorts by `file_name` allocating each comparison (`src/watch.rs`; use
  `sort_by_cached_key`), and `Renderer::render` allocates its `viewports` and `views` each frame
  with a 3D view (keep them on `Renderer`).
