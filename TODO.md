# TODO

What's left from the code review of November 2026, after the crash and hang fixes in `6376aff`
and the correctness fixes after them. Places are named by file and function, since line numbers
drift. Items marked (checked) were confirmed by reading the code; the rest were traced by a
reviewer but not checked again.

## Performance

The review's list is done. Two of its items were measured and left as they were, since the
changes it suggested measured no quicker. Measure any change to them with `cargo bench`
(`--bench drawing`, `physics`, `scenes` or `scripts`), as AGENTS.md asks, on the forge.

- [ ] **`Scene::pose`** was said to repeat work per track: texture handles cloned for every cell
  key, and each number track's transform worked out with libm. On the orchard's picker it takes
  about 45 ns a track (0.43-0.47 us a pose in `--bench scenes`), and checking only the cell set at
  `time` and updating each node once measured no quicker. Should posing ever show in a profile,
  finding each track's node by its path, every pose, is the likelier cost.
- [ ] **Native methods cost two callbacks.** A method call costs about 300 ns more than a function
  call (`--bench drawing`: 540-600 ns against 250-270), which is mlua calling `__index` on the
  object (`runtime/native.rs`). Keeping the class's method table in `NativeObject`, rather than
  looking it up, measured no quicker: the cost is the callback. mlua finds methods without one
  through `__namecall`, but only those its `UserData` registers, and a library's classes are only
  known as it registers. Making them as cheap as functions means objects of another make, such as
  a metatable for each class, which is a change to how scripts see them: ask first.
