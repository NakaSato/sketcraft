# CLAUDE.md — sketcraft-gpu

GPU rendering with wgpu: the viewport's render pipelines, offscreen captures (headless pictures), display tessellation plans evaluated in a compute shader, and the native progressive path tracer. It is layer L5, so it may use `wgpu`. Within L5, `ui-egui` and `mcp` may use it (`INTRA_LAYER_ORDER` in `xtask/src/layers.rs`). Its normal dependencies are `geom`, `render` and `sketcore-record`. See the root CLAUDE.md.

## Commands

```sh
cargo test -p sketcraft-gpu                              # unit tests and tests/*.rs
cargo test -p sketcraft-gpu --test capture               # headless picture checks
SKETCRAFT_REQUIRE_GPU=1 cargo test -p sketcraft-gpu      # fail, not skip, without a GPU adapter
SKETCRAFT_GPU=software cargo test -p sketcraft-gpu --test depth
cargo test -p sketcraft-gpu --test ground_ao_measurement -- --ignored   # manual timing, writes to SKETCRAFT_BENCH_OUTPUT_DIR
cargo run -p sketcraft-gpu --example glass_comparison
```

- `SKETCRAFT_GPU` picks the headless adapter: `auto`, `hardware`, `software` or `cpu`. `WGPU_BACKEND` narrows the backends. `WGPU_DX12_COMPILER` sets the D3D12 shader compiler.
- Capture tests skip with a printed reason when no adapter is found, unless `SKETCRAFT_REQUIRE_GPU=1`.

## Layout

- `src/lib.rs`: `TessGpu` (plan evaluation; `new` with the app's device, `headless` on native), `GpuMesh`, depth format helpers, error scopes.
- `src/tess.wgsl`: the tessellation compute shader, the GPU twin of `TessPlan::evaluate` in `geom`.
- `src/view.rs`, `src/view/`: viewport pipelines (shading, wide lines, section cap, AO, OIT, clouds, highlights, MSAA check).
- `src/capture.rs`: offscreen pictures through the viewport pipelines, with the CPU rasterizer as the last tier. The CLI, MCP screenshots and thumbnails draw through it.
- `src/select.rs` (native only): adapter classification and the `SKETCRAFT_GPU` policy.
- `src/concurrent.rs`: pipelines built on several threads (D3D12 compiles shaders at pipeline creation).
- `src/path/` (native only): the progressive path tracer, its WGSL shaders, denoise, photons, volumes and textures.
- `src/pbr.rs`: the opaque surface shading and studio reference, appended to the viewport shader. `src/tests.rs`: unit tests, including a check that the HLSL from naga suits FXC.
- `tests/`: `capture.rs`, `depth.rs`, `thin_lines.rs`, `ground_ao_measurement.rs`.

## Rules

- No egui, eframe or winit here. The app hands this crate a device and a render pass.
- Native backends (`metal`, `vulkan`, `dx12`, `gles`) and `pollster` are desktop-only dependencies. `path` and `select` are `#[cfg(not(target_arch = "wasm32"))]`. Keep the crate building for wasm32.
- In the browser an error scope's answer comes later and is not waited for. Do not make correctness depend on it.
- The engine cannot depend on this crate. Pictures reach it through the renderer the host installs (`crates/engine/src/capture.rs`).
- GPU output differs between drivers. Tests count pixels of a kind; they do not compare bytes.
- Every picture names the renderer that drew it (`gpu`, `gpu-software` or `cpu`). Keep that when adding a path.
- Crash-safety rules apply (the crate root denies `unwrap` and friends): `skills/sketcraft-crash-safety/SKILL.md`. Window-thread and wasm cost: `skills/sketcraft-performance/SKILL.md`.

## Gotchas

- Dev-dependencies pull in the real engine (with `bundled-render-library`), kernel and io, so the tests build most of the workspace.
- `robust` is listed twice in `Cargo.toml`, once as a plain version and once from the workspace for native targets.
- macOS uses Metal only and has no software adapter, so `SKETCRAFT_GPU=software` fails there.
