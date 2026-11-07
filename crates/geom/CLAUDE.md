# CLAUDE.md — sketcraft-geom

The geometry foundation (layer L0): vectors, planes, boxes, triangle meshes and their measures,
and freeform surface maths (B-splines, NURBS, fills, curve networks, T-splines). Units are
millimetres. It knows nothing about sketches, documents or the kernel; every other crate builds on it.
See the root CLAUDE.md and AGENTS.md for workspace rules.

## Commands

```sh
cargo test -p sketcraft-geom                    # all unit tests
cargo test -p sketcraft-geom --lib fill_iso     # tests whose name contains "fill_iso"
cargo check -p sketcraft-geom --target wasm32-unknown-unknown
# Ignored timing run (not a check):
SKETCRAFT_BENCH_STL=part.stl cargo test --release -p sketcraft-geom --lib simplify_timing -- --ignored --nocapture
```

There is no `tests/` directory. Tests are unit tests in `src/tests.rs` and `src/<module>_tests.rs`
files next to their module.

## Layout

- `src/lib.rs`: module list, re-exports, `prelude` (geometry traits on `solidcore_record` types),
  `EPS`, `LINEAR_TOL`, `finite_or`.
- `vec.rs`, `unit_vec3.rs`, `plane.rs`, `aabb2.rs`: vector, plane and 2D box helpers as traits
  (`Vec3Geom`, `PlaneGeom`, …) on the record's plain data.
- `profile.rs`, `arcfit.rs`, `chain.rs`: planar profiles (`Seg2`/`Loop2`/`Region2`), arc fitting,
  typed rings and paths.
- `mesh.rs`, `bvh.rs`, `mesh_distance.rs`, `compare.rs`, `simplify.rs`, `inertia.rs`, `ball.rs`:
  meshes, measures, spatial index, distances, display LOD.
- `bspline.rs`, `spline_space.rs`, `fill*.rs`, `network.rs`, `linkage.rs`, `blend.rs`,
  `catmull_rom.rs`, `tspline/`: freeform curves and surfaces.
- `brep.rs` + `brep/`: evaluation and checks on the portable B-rep record (`solidcore_record::brep`).
- `tess.rs` + `tess/`: display tessellation plans. `hlr/`: hidden-line removal. `section.rs`.
- `drawing_sheet.rs`, `drawing_view.rs`, `title_block.rs`, `cell.rs`, `paper.rs`: drawing data in
  sheet millimetres (no drawing or file writing here).
- `env.rs`: every `SKETCRAFT_<NAME>` environment variable is read through `env::var`/`var_os`.
- `stderr.rs`: `note!`, an `eprintln!` that never panics on a closed stderr.
- `srgb.rs`: the one sRGB transfer function for Rust code.

## Rules

- Dependencies: only `solidcore-record`, `serde`, `thiserror`. No workspace crate. `cargo xtask layers`
  enforces this.
- No egui/wgpu/winit/rfd, no `unsafe` (`#![forbid(unsafe_code)]`).
- The crate root denies `unwrap`, `expect`, `panic!`, `todo!`, `unreachable!`. Input is hostile:
  check finiteness, cap sizes and loop counts. See `skills/sketcraft-crash-safety/SKILL.md`.
- Must build for `wasm32-unknown-unknown` (checked by `cargo xtask wasm`).
- New `mod` lines go into the sorted list in `lib.rs`.

## Gotchas

- Read environment variables through `env::var`, not `std::env`: it falls back to the legacy
  `SOLVECRAFT_` prefix.
