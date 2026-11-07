# CLAUDE.md — sketcraft-render

View math and device-independent rendering (layer L1): the orbit camera, projection and picking
rays, the description of a headless picture (`Scene`), a small CPU rasterizer for snapshots
without a GPU, and product render jobs (film, lighting, textures, an optional CPU ray tracer).
Depends only on `sketcraft-geom` among workspace crates. The GPU crate (`crates/gpu`) draws the
same `Scene` with the viewport's pipelines. See the root CLAUDE.md.

## Commands

```sh
cargo test -p sketcraft-render                          # default features
cargo test -p sketcraft-render --features raytrace-cpu  # also the CPU ray tracer (src/raytrace/)
cargo test -p sketcraft-render --lib camera             # filter by name
cargo check -p sketcraft-render --target wasm32-unknown-unknown
cargo run --locked --profile ci -p sketcraft-render --example validate_starter
cargo run -p sketcraft-render --features raytrace-cpu --example cpu_resource_smoke -- NEW_DIR [4k|8k|dense|bodies]
# Ignored manual measurement:
cargo test --release -p sketcraft-render --lib sampler_cost_against_the_reference -- --ignored --nocapture
```

## Layout

- `src/camera.rs`: `Camera`, `Mat4`, `Quat`, `StandardView`, `CameraAnim`.
- `src/scene.rs`: `Scene`, `SceneMesh`, `SceneLine`, colours, section cap/hatch constants.
- `src/raster.rs`: CPU rasterizer (`render`, `render_png`, `render_transparent`). `src/gif.rs`.
- `src/curve_display.rs`: pixel-budgeted curve sampling for display.
- `src/analysis.rs`: surface analysis colouring.
- `src/backend.rs` (+ `backend/tests.rs`): render capability metadata and request resolution.
- `src/job/`: product-image jobs (`job/mod.rs`, `host.rs`, `output.rs`, `image.rs`, `png.rs`,
  `linear_exr*.rs`, `textures.rs`, …). Hosts own execution; documents never hold these values.
- `src/raytrace/` (feature `raytrace-cpu`): CPU path tracing.
- `src/film*.rs`, `src/lighting/`, `src/texture_*` (+ folders), `src/surface*.rs`,
  `src/stage.rs`, `src/studio.rs`, `src/mapping.rs`, materials (`glass_model.rs`, `clear_coat.rs`,
  `transmission.rs`, `subsurface.rs`, `anisotropic_surface.rs`).
- Tests: `src/*_tests.rs` and `tests.rs` files inside the module folders. No `tests/` directory.

## Features

- `raytrace-cpu` (off by default): the CPU ray tracer, pulls in `robust`. Enabled through
  `sketcraft-engine/raytrace-cpu`; the desktop app and CLI turn it on by default.

## Rules

- May depend on `sketcraft-geom` only (plus `sketcore-record`). No egui, wgpu or window types:
  device resources belong in `crates/gpu`.
- No `unsafe`. The crate root denies `unwrap`/`expect`/`panic!`. Image and texture input is hostile:
  bound sizes and memory. See `skills/sketcraft-crash-safety/SKILL.md`.
- Must build for wasm32. Use `web_time`, not `std::time::Instant`.
- `job/textures.rs` embeds starter textures from `assets/render/starter/` with `include_bytes!`;
  moving those files breaks the build.

