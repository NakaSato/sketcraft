# CLAUDE.md — sketcraft-step

STEP (ISO 10303) as plain data, with no modelling backend: the Part 21 exchange structure, Part 42 geometry and topology, and the units of a representation context. It reads solids into the kernel-neutral B-rep record (schema 4) and writes records back. It is layer L1. Its dependencies are `geom`, `profile` and `solidcore-record`. `crates/kernel` uses it (`INTRA_LAYER_ORDER` in `xtask/src/layers.rs`). See the root CLAUDE.md.

## Commands

```sh
cargo test -p sketcraft-step                             # all unit tests (no tests/ dir)
cargo test -p sketcraft-step --lib topology::tests       # one test module (src/topology_tests.rs)
SKETCRAFT_STEP_CORPUS=/path/to/models cargo test -p sketcraft-step -- --ignored --nocapture
```

- The two `#[ignore]` tests (`corpus_solids_read_into_schema_4` in `src/topology_tests.rs` and one in `src/geom_tests.rs`) read every `.step`/`.stp` file in `SKETCRAFT_STEP_CORPUS`.

## Layout

- `src/lib.rs`: the module list.
- `src/p21.rs`: the Part 21 parser (HEADER and DATA, simple and complex instances).
- `src/read.rs`: geometry entities into the curves and surfaces of `src/geom.rs`. `src/eval.rs`: points on STEP curves and surfaces in STEP's own parameterisation.
- `src/geom.rs`: curves and surfaces exactly as the file defines them, in millimetres and radians. Nothing is approximated or converted on the way in.
- `src/topology.rs`, `src/topology/`: solids, shells and faces into schema 4. `seams.rs` adds seams on periodic faces, `frames.rs` turns round-face frames, `swept.rs` maps swept, offset and open-conic geometry.
- `src/units.rs`: length and angle units, and the distance uncertainty. Missing or bad units read as mm and radians.
- `src/write.rs`, `src/write/topology.rs`: write Part 21 and schema 4 solids back. Identical geometry is shared; topology is distinct.
- Tests: `src/geom_tests.rs` (entity read, write and round trips) and `src/topology_tests.rs` (small solids written entity by entity). Expected values are analytic.

## Rules

- Written from the public ISO 10303-21, -41, -42 and -43 specifications. Do not copy third-party STEP code. See `skills/solidcraft-ground-rules/SKILL.md`.
- Input is hostile. Cap sizes, entity counts and nesting depth, check every reference, and return an error message, never a panic. See `skills/solidcraft-crash-safety/SKILL.md`.
- Keep this crate backend-free. Do not depend on `kernel`, `smlib`, `doc` or anything higher.
- Keep STEP's own parameterisation, so 2D curves in a surface's parameter space keep their meaning.
- A new entity gets read, write and read → write → read cases in `src/geom_tests.rs` or `src/topology_tests.rs`, with analytic expected values.
- The crate root denies `unwrap`, `expect`, `panic!` and friends, and forbids `unsafe`.

## Gotchas

- `crates/kernel/src/step_in/` has its own `p21.rs` and walks the product structure. This crate reads geometry and topology only.
- Doc comments cite `docs/native-brep.md`, which is not in this tree.
