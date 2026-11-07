# CLAUDE.md — sketcraft-engine

The engine: `Session`, the command registry, undo/redo, selection, autosave, recovery and crash reports. It is layer L4. It may use `geom`, `sketch`, `kernel`, `render`, `doc`, `io` and `profile`, but never `ui-egui`, `gpu`, `mcp` or any UI crate. Read the root CLAUDE.md and AGENTS.md first.

## Commands

```sh
cargo test -p sketcraft-engine                          # unit tests in src/ and tests/*.rs
cargo test -p sketcraft-engine hostile_params           # tests whose name matches a filter
cargo test -p sketcraft-engine --test atomic_commands   # one integration test file
cargo test -p sketcraft-engine --lib cmd::edit::tests   # one module of unit tests
cargo run -p xtask -- test -- <filter>                  # what the docs call `cargo xtask test`
```

- Features: `profile`, `raytrace-cpu`, `bundled-render-library`, `solver-profiling`, `param-audit`. The `sketch_*_bench` examples need `--features solver-profiling`. `render_resource_workflow` and `wood_texture_workflow` need `--features raytrace-cpu`.
- Contract audit: `SKETCRAFT_PARAM_AUDIT=<dir> cargo test --workspace --features sketcraft-engine/param-audit` writes `<dir>/audit-<pid>.jsonl` (see `src/schema/record.rs`).
- `SKETCRAFT_SCHEMA_RULE_STRICT=1` turns the missing-schema warning in `src/schema/coverage_tests.rs` into a failure.
- `HOLE_MATRIX_BLESS=1 cargo test -p sketcraft-engine hole_matrix` rewrites the hole matrix list.
- A few tests are `#[ignore]` (benchmarks, manual timings). Each one says why.
- Environment variables are read with `sketcraft_geom::env::var`: `SKETCRAFT_<NAME>`, with `SOLVECRAFT_<NAME>` as a fallback.

## Layout

- `src/lib.rs`: `Session`, `Session::new`, `Session::execute` (the one entry point for every command), `EngineError`, `MAX_LINE` (longest JSON line for MCP and the control channel).
- `src/cmd/mod.rs`: `CommandSpec` and its builder (`.at`, `.icon`, `.key`, `.params`, `.enabled`, `.noundo`, `.doc_only`, `.live`, `.contract`, `.output`), the `planned!` macro, `command_specs()` (the registry, in toolbar order), `find_command`, `in_sketch` / `not_in_sketch`.
- `src/cmd/*.rs`: one file per area (sketch, features, components, joints, motion, file, render, view, inspect, measure…). Each exports a `COMMANDS` array. Tests sit next to them as `*_tests.rs`.
- `src/schema/`: typed parameter contracts and reply schemas, one module per command file, plus `types.rs` (shared field types), `check.rs`, `record.rs` (`record_wasm.rs` on wasm), `coverage_tests.rs` and `unconverted.txt`.
- `src/params.rs`: readers for hostile JSON parameters (`bad`, `finite`, vectors, expressions).
- `src/frames.rs`: maps world picks into the frame of the component they belong to, before a command runs.
- `src/crash.rs`, `src/recovery.rs`: crash reports and the command journal; autosave and recovery entries.
- `src/capture.rs`: the hook a host installs to draw headless pictures (the GPU crate is above this one).
- `src/panels/`: the application layout model, independent of `Session` and egui.
- `src/view.rs`, `src/snap.rs`: what the viewport shows, and snap points on hovered edges and faces.
- `src/fuzz.rs`: seeded random modelling sessions (`sketcraft-cli fuzz`). `src/sample.rs`: the sample design script.
- `src/control_auth.rs`, `src/agent_setup.rs`: the control channel token, and `mcp setup` for AI clients.
- Tests: `src/tests.rs` (the large unit module, with `hostile_params_never_panic`), `src/tests/` (area tests, `.sc` fixtures in `src/tests/fixtures/`), `src/*_tests.rs`, and `tests/*.rs` (atomic commands, export, shell and STEP regressions; shared code in `tests/support/`).

## Rules

- Adding or changing a command: follow `skills/sketcraft-commands/SKILL.md`. In short:
  - Add a `CommandSpec` to the right `COMMANDS` array in `src/cmd/`. A new file also needs its `mod` line (in the sorted group) and a `v.extend(...)` line in `command_specs()`.
  - Ids are our own: dotted, lower case, `<area>.<thing>[.<variant>]`.
  - Validate parameters first and return `EngineError::BadParams` (`params::bad`). Never build a broken feature.
  - Give every new command a contract and an output schema in `src/schema/<file>.rs`. Never add a line to `unconverted.txt`; it only shrinks.
  - Add `cmd.<id>` (and `help.<id>` for ribbon commands) through `assets/locales/pending/`. See `skills/sketcraft-ui-text/SKILL.md`.
- Commands never open dialogs. Only `ui.start` in the front end starts interactive tools.
- Commands that change the document are undoable by default. A failing command must leave the document unchanged.
- Non-test code follows `skills/sketcraft-crash-safety/SKILL.md`. The crate root denies `unwrap`, `expect`, `panic!`, `todo!`, `unreachable!` and `unimplemented!`, and forbids `unsafe`.
- Model decisions use exact geometry, not display meshes. `cargo run -p xtask -- mesh-decisions` checks this crate. `xtask/mesh-decisions.txt` only shrinks.
- The crate builds for wasm32. Use `web_time` for clocks (std `Instant` panics in the browser). `slicer_files` is native-only.
- A command that saves something new in a design needs a format file: `skills/sketcraft-file-formats/SKILL.md`.
- Test placement: `skills/sketcraft-tests/SKILL.md`. Measure results (volume, area, counts) against values you can derive by hand.

## Gotchas

- `Session::execute` runs each command under `catch_unwind` and turns a panic into `EngineError::Internal`. This is a last resort. Do not rely on it.
- `find_command` falls back to a case-insensitive match.
- `panels.*` commands always refuse in a bare `Session` ("application layout backend was not injected"). The host provides that backend.
- Docs that comments cite (`docs/book/`, `docs/control-protocol.md`, `docs/crash-reports.md`, `docs/robustness.md`) are not in this tree.
