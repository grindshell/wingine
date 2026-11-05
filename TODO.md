# TODO

## 3D correctness review — 2026-11-03

Reviewed commits `6e56e35` and `5f9f60f`. All three findings below reproduced on
NVIDIA Vulkan and Microsoft WARP. Existing library, runtime, cellar, meshes-example,
and screenshot suites passed: 276 tests passed, one ignored.

- [x] **P2: Preserve pixel coverage at half-pixel view bounds.**
  Source: [Viewport::new](src/graphics/renderer.rs), currently line 704.
  Rounding every viewport edge excludes pixel centers exactly on the left or top
  boundary. With an orthographic camera of height 2, draw an opaque front-facing quad
  spanning the view into `Rect::new(0.5, 0.5, 10.0, 10.0)` on a 16×16 canvas: only
  9×9 pixels render, while a 2D fill of the same rect covers 10×10. This can leave gaps
  at fractional positions or display scales. Calculate bounds using the rasterizer's
  pixel-center inclusion rules. Acceptance: compare 2D and 3D coverage at offsets
  0.49, 0.5 and 0.51, including clipped views and fractional canvas-to-target scaling.

- [x] **P2: Make vertex normal length independent of lighting.**
  Source: [mesh vertex shader](src/graphics/mesh.wgsl), currently lines 62 and 70.
  The API says a normal's length does not matter, but raw normals are interpolated
  before fragment normalization, so longer ones bias the shading. Reproduce with a
  triangle at `(-1, -1, 0)`, `(1, -1, 0)`, `(0, 1, 0)`, normals `(0, 0, 1)`,
  `(0, 1, 0)`, `(0, 1, 0)`, white sunlight directed `(0, -1, 0)` and black ambient
  light. Multiplying only the first normal by 10 changes 2,016 pixels in a 64×64
  orthographic view of height 2, despite preserving every normal's direction.
  Normalize source normals before interpolation, preserving zero normals.
  Acceptance: independently rescaling vertex normals leaves lighting unchanged;
  nonuniform and mirrored transforms still produce correct normals.

- [x] **P2: Keep billboards facing the camera when it rolls.**
  Source: [Camera3d::billboard_axes](src/graphics/camera3d.rs), currently lines 124–127.
  Billboard facing is derived from the camera's rolled right vector. With an
  orthographic camera at `(0, 1, 5)`, target `(0, 0.5, 0)` and height 2, a billboard
  at the origin disappears when `up` changes from `(0, 1, 0)` to `(0, -1, 0)`:
  its front turns away and is culled. Setting `up` to `(1, 0, 0)` turns it edge-on
  and also makes it disappear. Derive horizontal facing from the viewing direction
  independently of roll, retaining a fallback for vertical views. Acceptance:
  billboards remain visible through quarter- and half-turn camera rolls, stay
  upright in world space, and retain negative-width mirroring.

## Asked for by Paradise Lost — 2026-11-05

Gaps that [Paradise Lost](../paradise-lost)'s port to Luau met and deliberately did
not work around (its `AGENTS.md`, *Engine gaps*). The game is waiting on each one.
The suggested shapes are only suggestions; the design is the engine's to make.

- [x] **Key repeat for scripts.**
  Source: [Buttons::down](src/input.rs), currently lines 436–441, and
  [key_event](src/app.rs), currently line 567.
  The window's repeated presses are dropped, so a held key is exactly one press. Paradise
  Lost takes one action per key, and its old window walked while a direction was held. In
  the port, holding `l` takes one step. The game could repeat keys itself only by timing
  held keys in Luau against a delay and rate it can't read from the OS, which is the
  engine's job. Wanted: a way for a script to hear repeats, such as `input.key_repeated(key)`
  or a pressed-or-repeated query, perhaps for actions too. It should be driven by the OS's
  repeats in a window and be simulable in a `Simulation` and a `WINGINE_SCRIPT` script.
  Acceptance: holding a key in a window gives its first press, then repeats at the OS's
  delay and rate. `key_pressed` and `pressed` still mean the first press only, so no
  existing game changes.

- [x] **Typed characters for the game, not only for egui.**
  Source: [key_event](src/app.rs), currently line 567, reads `event.physical_key` only.
  The window's typed text reaches egui alone, and
  [Simulation::type_text](src/headless.rs), currently line 537, says so: "The game's own
  input doesn't see typing."
  Paradise Lost's vocabulary is characters: `>` takes the stairs, `?` opens the controls,
  shift and a direction letter turns. So `game/controls.luau` writes a US layout out by
  hand, and on a German or French keyboard `>` is shift plus whatever key sits where a US
  full stop does. Wanted: the characters typed for this update, in order, as the OS's
  layout produced them, for Luau and Rust games. A `Simulation` and a script would also
  need a way to type into the game. Acceptance: on a non-US layout, typing `>` delivers
  `>`. Dead keys and IME input deliver what they compose. Physical keys behave exactly as
  they do now.

- [x] **Handing a scripted run somewhere to start.**
  Source: [script::run](src/script.rs), currently line 86, builds its `Simulation` from
  the config alone. Its commands are input, `resize`, `screenshot` and `benchmark`.
  A `WINGINE_SCRIPT` run keeps its saves in an empty temporary folder and can be handed
  nothing else: no save folder, environment, arguments or Luau. So every script in
  Paradise Lost's `checks/` starts a new career in town and reaches anything deeper by its
  keys alone. The game's F1 debug window can go anywhere, but a script can find that
  window's buttons only by pixel, since no command does what
  [Simulation::click_gui](src/headless.rs) (currently line 587) does: click a widget by its
  label. Rust tests don't need any of this, because they hand a `Simulation` a
  `save_folder` and run Luau in it. Wanted, any one of these would do:
  - a script command that clicks a GUI widget by label, such as `click-gui Go`;
  - a script command naming a save folder to start from, copied into the run's temporary
    folder so the original is never written;
  - a way to run Luau from a script.

  Acceptance: a script can reach a state a new save can't reach by keys alone, without
  naming a pixel.
