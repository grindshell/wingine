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
