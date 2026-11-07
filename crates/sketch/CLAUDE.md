# CLAUDE.md — sketcraft-sketch

Sketches (layer L1): points and curves on a plane, geometric constraints and driving dimensions,
our own Levenberg–Marquardt constraint solver with degree-of-freedom analysis, profile (region)
detection, offsets, patterns, text outlines and fonts. Depends only on `sketcraft-geom` among
workspace crates. See the root CLAUDE.md.

## Commands

```sh
cargo test -p sketcraft-sketch                         # unit tests + tests/*.rs
cargo test -p sketcraft-sketch --lib profiles          # one area by name filter
cargo test -p sketcraft-sketch --test cursor_target    # one integration test file
cargo test -p sketcraft-sketch --features solver-profiling   # adds the profiling tests
cargo run --release -p sketcraft-sketch --example solve_bench
cargo run --release -p sketcraft-sketch --features solver-profiling --example solve_profile -- --help
# Ignored measurement tests (print numbers, no assertion on time):
cargo test --release -p sketcraft-sketch --lib rounds_and_tangents_without_the_seed -- --ignored --nocapture
```

## Layout

- `src/lib.rs`: module list and the public re-exports (the crate's API is what is re-exported here).
- `src/model.rs`: `Sketch`, `SPoint`, `Curve`, `Constraint`, `SketchError` and the hard caps
  (`MAX_POINTS`, `MAX_CONSTRAINTS`, `MAX_CURVES`, `MAX_COORD`).
- `src/solver.rs`: `solve*`, `Analysis`, `SolveReport`. It pulls in `cursor.rs` (drag motion) and
  `redundancy.rs` via `#[path]`. `construct.rs` and `loci.rs`: the constructive seed before iterating.
- `src/decompose.rs`: splitting into components, with `Budget`.
- `src/profiles.rs`: closed-profile detection (`find_profiles`, `profile_at_point`, …).
- `src/curves.rs`, `nurbs.rs`, `periodic.rs`, `spline_points.rs`, `equation.rs`: splines, conics,
  equation curves.
- `src/offset.rs`, `offset_topology.rs`, `freeform_offset.rs` + `freeform_offset/`, `thin.rs`:
  offsets and thin walls.
- `src/link.rs` (+ `link_loops.rs`): sketch links to model geometry. `src/pattern.rs`, `shape.rs`,
  `pierce.rs`, `ends.rs`, `wire.rs`, `dims.rs`.
- `src/text.rs`, `src/fonts.rs`: text outlines (harfrust, ttf-parser, unicode-bidi). The default
  font is embedded from `assets/fonts/liberation-sans/`.
- `src/profiling.rs`: only with the `solver-profiling` feature.
- Tests: `src/tests.rs` and `src/*_tests.rs` (some are included from their module with `#[path]`,
  not from `lib.rs`), plus `tests/analysis_levels.rs` and `tests/cursor_target.rs`.
- `examples/solve_bench.rs`, `examples/solve_profile.rs` (needs `solver-profiling`).

## Rules

- May depend on `sketcraft-geom` only (plus `solidcore-record`). Not on the kernel, doc or UI.
- No egui, no `unsafe`. The crate root denies `unwrap`/`expect`/`panic!`.
- Sketch input is hostile: respect the `MAX_*` caps, check finite coordinates within `MAX_COORD`.
  See `skills/solidcraft-crash-safety/SKILL.md`.
- Must build for wasm32 (`cargo xtask wasm`). The `solver-profiling` feature has no clocks on wasm.
- The solver is a clean-room implementation. Do not copy another solver's code.
- Measure before claiming a solver speed-up: `skills/solidcraft-performance/SKILL.md`.

## Gotchas

- `SKETCRAFT_FONT_DIRS` adds font folders to the installed-font lookup. Apps can set a file
  check with `set_font_file_check`.
- Doc comments in examples still say `solidcraft-sketch`; the package is `sketcraft-sketch`.
