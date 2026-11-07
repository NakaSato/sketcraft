# CLAUDE.md — sketcraft-kernel

The B-rep kernel boundary. Every solid operation goes through this crate: extrude, revolve, sweep and loft, booleans, fillets and chamfers, shells, surfaces, tessellation, measurement, and STEP and IGES import and export. It is layer L1. It may use `geom`, `profile`, `smlib` and `step`. See the root CLAUDE.md.

The backend is sketcore, reached through `sketcraft-smlib`. This crate depends only on `sketcore-record`, not on `sketcore` itself. The `lib.rs` doc comment still mentions `truck`; that is out of date.

## Commands

```sh
cargo test -p sketcraft-kernel                           # unit tests in src/ and tests/*.rs
cargo test -p sketcraft-kernel --test boolean_props      # proptest invariants of booleans
PROPTEST_CASES=2000 cargo test -p sketcraft-kernel --test boolean_props
cargo test --release -p sketcraft-kernel perf_measure_5000_faces -- --ignored
NATIVE_BREP_BLESS=1 cargo test -p sketcraft-kernel box_and_fillet_have_stable_wire_records
                                                         # rewrites src/native/fixtures/box-fillet-v5.json
```

- Other ignored tests take env vars: `SKETCRAFT_STEP_BENCH_COUNT`, `SKETCRAFT_STEP_BENCH_KIND`, `SKETCRAFT_STEP_BENCH_FIXTURE` (`src/step_in/tests.rs`), `THREAD_STEP` (`src/thread_display_tests.rs`) and `SKETCRAFT_TESS_SVG` (`src/tess/tests.rs`).
- `examples/step_board_fixture.rs` writes a large board-like STEP assembly (`SKETCRAFT_STEP_PCB_COMPONENTS`, `SKETCRAFT_STEP_PCB_PINS`).
- Geometry regressions also run through the kernel rig: `cargo run -p xtask -- kernel-cases` (cases in `tests/kernel-cases/`).

## Layout

- `src/lib.rs`: the public API (all `pub use`), `KernelError`, `guard`, `guarded`, `crash_test`.
- `src/body.rs`: `Body`, face and edge info, display mesh settings.
- `src/build.rs`: primitives, extrude, revolve, loft, sweep. `src/prism*.rs`: planar-region extrusions.
- `src/ops.rs`: booleans, transforms, `split_by_plane`, `apart`. `src/smlib_boolean.rs`, `src/smlib_sweeps.rs`, `src/blend_smlib.rs`: calls into smlib.
- `src/blend*.rs`, `src/corner_g2.rs`: fillets and chamfers. `src/polyhedron.rs`: shell, draft, offset faces.
- `src/surfaces.rs`, `src/surface_*.rs`, `src/fill.rs`, `src/boundary*.rs`, `src/fair.rs`: surface tools.
- `src/tess/`: tessellation (CDT, grids, seams, sag). `src/meshbody.rs`, `src/mesh_convert.rs`: mesh bodies.
- `src/step_in/`: our own STEP reader (product structure, units, colours). `src/step.rs`, `src/step_out.rs`: export. `src/iges_in*`, `src/iges_out.rs`: IGES.
- `src/native.rs`, `src/native/`: the native B-rep encoding (`SCBREP05`) and its fixture.
- `src/measure.rs`, `src/validity.rs`, `src/validation.rs`: mass properties and validity checks.
- `src/par.rs`: parallel map on the worker pool (serial on wasm), `KERNEL_STACK` (8 MiB). `src/watch.rs`: progress and cancellation.
- Tests: many `#[cfg(test)]` modules in `src/` (`*_tests.rs`, `issue_<n>_tests.rs`, `src/tests.rs`) and `tests/*.rs`.

## Rules

- No `sketcore` type in the public API. Callers see this crate's types, `sketcore-record` records and a few smlib re-exports (`Stop`, `hold_a_kernel`, `smlib_trace`).
- Run third-party kernel code under `guard`. It turns a panic into `KernelError::Internal`. `par.rs` already wraps each item.
- Kernel threads need `KERNEL_STACK`. Use `kernel_thread` or the helpers in `par.rs`, not a plain `std::thread::spawn`.
- STEP and IGES files are hostile. Cap sizes, counts and nesting. Turn what cannot be read into a warning when the rest of the file is still usable. See `skills/sketcraft-crash-safety/SKILL.md`.
- Model decisions use exact geometry. `cargo run -p xtask -- mesh-decisions` checks this crate against `xtask/mesh-decisions.txt`, which only shrinks.
- `rayon` is desktop-only. Code must still build for wasm32.
- Put a regression test in its area's existing test file. Prefer exact, hand-derived volumes and areas. See `skills/sketcraft-tests/SKILL.md`.
- Never change this crate to depend on `doc`, `io`, `engine` or anything higher.

## Gotchas

- `KernelError::TooLarge` reads "not supported yet". The UI shows it like an unfinished feature.
- `src/step_in/p21.rs` is a separate Part 21 parser from the one in `crates/step`.
