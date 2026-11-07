# CLAUDE.md — sketcraft-cli

The headless command line. It runs command scripts and designs, measures, exports, draws snapshots, replays oracle recipes and kernel cases, fuzzes, reads crash reports, and serves MCP over stdio. It is an app, so the layer rules do not apply to it (`Exempt` in `xtask/src/layers.rs`). It uses `sketcraft-engine`, `sketcraft-gpu` and `sketcraft-mcp`. See the root CLAUDE.md.

## Commands

```sh
cargo run -p sketcraft-cli -- --help                     # the usage of every subcommand
cargo run -p sketcraft-cli -- run script.json --out part.step
cargo run -p sketcraft-cli -- eval design.sc             # measurements as JSON
cargo run -p sketcraft-cli -- snapshot design.sc --out shot.png
cargo run -p sketcraft-cli -- commands                   # the command registry as JSON
cargo run -p sketcraft-cli -- mcp --headless             # MCP server on stdio
cargo test -p sketcraft-cli                              # tests/*.rs (most run the built binary; see Gotchas)
cargo test -p sketcraft-cli --test design_corpus         # one test file
```

- Subcommands: `run`, `eval` (alias `inspect`), `script`, `bench-open`, `bench-agent`, `bench-assistant`, `snapshot`, `render`, `exec`, `commands`, `recipe`, `oracle`, `kernel-cases`, `fuzz`, `fuzz-one`, `mcp`, `crashes`, `licences` (alias `licenses`), `gpu`, `icon`, `bench-edit`, `save-stress`. The list is the `Command` table in `src/main.rs`.
- Features (default on): `raytrace-cpu`, `profile`. Optional: `bundled-render-library`.
- The action matrix: `cargo run -p xtask -- action-matrix [--bless]` runs the ignored `action_matrix_all` test with the `ci` profile. Its other knobs are `ACTION_MATRIX_*` env vars in `tests/action_matrix.rs`.
- The oracle: `cargo run -p xtask -- oracle` runs `cargo run --release -p sketcraft-cli -- oracle` over `tests/oracle/*`. `tests/oracle_cases.rs` is ignored (cargo-mutants only).
- Snapshot tests skip without a GPU adapter unless `SKETCRAFT_REQUIRE_GPU=1`.

## Layout

- `src/main.rs`: argument parsing, the `Command` table and usage strings, and the subcommands that need little code. Global flags: `--debug`, `--max-file-size SIZE`.
- `src/alias.rs`: the `solvecraft-cli` binary, a deprecated alias that runs `sketcraft-cli`.
- `src/oracle.rs`, `src/recipe.rs`: oracle recipes to command scripts, and the comparison with `expected.json`.
- `src/kernel_cases.rs`, `src/kernel_cases_export.rs`: the kernel rig (`tests/kernel-cases/<area>/<id>/`) and its corpus export.
- `src/fuzz.rs`: seeded fuzzing, one process per seed (`fuzz-one`) with a time limit.
- `src/crashes.rs`: list, show, replay and delete the desktop app's crash reports.
- `src/render.rs`, `src/preview.rs`: render jobs, and snapshot options (`--body`, `--tool`, `--section`).
- `src/bench_agent.rs`, `src/bench_assistant.rs`: benchmarks (the second needs a running app with `--control PORT`).
- `src/profiling.rs`, `src/logger.rs`, `src/spatial.rs`: `--profile`, stderr logging, mesh comparison helpers.
- `tests/`: integration tests that run the binary. `tests/corpus/` holds old design files with `.expected.json` records and `scripts/` (see its README).
- `examples/studio_evidence.rs`: render evidence through the production job interfaces.

## Rules

- stdout is the command's JSON (or MCP's JSON-RPC). Notes, logs and profiles go to stderr.
- The CLI only calls engine commands (`Session::execute`). Add behaviour as a command in `crates/engine`, not here. See `skills/sketcraft-commands/SKILL.md`.
- A new subcommand needs an entry in the `Command` table and its usage string. `tests/help_output.rs` checks the help.
- Crash-safety rules apply (both binaries deny `unwrap` and friends and forbid `unsafe`): `skills/sketcraft-crash-safety/SKILL.md`.
- Oracle expected values never come from SketCraft's own output: `skills/sketcraft-tests/SKILL.md`.

## Gotchas

- `tests/KNOWN_FAILING_TESTS` names these tests `sketcraft-cli::…`, while nextest will report `sketcraft-cli::…`.
- `src/main.rs` includes `assets/icons/app/shared/source/app_icon.rs` with `#[path]`.
- Docs that comments cite (`docs/mcp.md`, `docs/crash-reports.md`, `docs/robustness.md`, `docs/oracle.md`) are not in this tree.
