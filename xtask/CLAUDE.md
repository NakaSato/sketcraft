# CLAUDE.md — xtask

Repository tooling: the test runner, the CI gate, the layering check, and checks for i18n, icons, licences, skills, assets, mesh decisions, the oracle, the kernel rig and the action matrix. It is a binary (`publish = false`) and is exempt from the layering rules. It depends only on external crates. See the root CLAUDE.md.

## Commands

This tree has no `.cargo/config.toml`, so the `cargo xtask` alias that the docs use is not defined. Run the binary directly:

```sh
cargo run -p xtask -- help                               # the full usage
cargo run -p xtask -- layers                             # crate layering
cargo run -p xtask -- test -- <filter>                   # build tests with the ci profile, run every binary
cargo run -p xtask -- ci                                 # fmt, checks, clippy -D warnings, tests, wasm
cargo run -p xtask -- known-failing --junit <dir>        # grade a nextest junit report
cargo test -p xtask                                      # xtask's own unit tests
```

- `test`: `SKETCRAFT_TEST_JOBS=N` bounds concurrent test binaries and side jobs. `RUST_TEST_THREADS` and `ACTION_MATRIX_JOBS` still win when set.
- `ci`: `SKETCRAFT_TEST_RUNNER=libtest` (the default) or `nextest` picks the test runner.
- `icons` and `ci` run Python 3.11+ scripts. `ci` also runs `web-offline-test` when Chrome is installed (`CHROME` names the binary).

## Layout

- `src/main.rs`: `USAGE` (the help text), the `match` that dispatches subcommands, and `oracle`, `action-matrix`, `wasm` and `ci` themselves.
- `src/test.rs`, `src/test_nextest.rs`: `test`, and the optional nextest runner for `ci`.
- `src/layers.rs`: the layer `TABLE`, `INTRA_LAYER_ORDER`, `UI_CRATES` and the rule engine.
- `src/known_failing.rs`: `known-failing` against `tests/KNOWN_FAILING_TESTS`.
- `src/kernel_cases.rs`, `src/kernel_ladder.rs`, `src/kernel_corpus.rs`: the kernel rig (`tests/kernel-cases/`), its ladder and its corpus export.
- `src/mesh_decisions.rs`: checks `crates/doc`, `crates/engine` and `crates/kernel` against `mesh-decisions.txt`.
- `src/skills.rs`: checks AGENTS.md and `skills/*/SKILL.md`.
- `src/i18n.rs`, `src/pending.rs`, `src/lists.rs`: message catalogs, folding pending files, sorted lists.
- `src/licences.rs`, `src/assets.rs`, `src/render_assets.rs`: `THIRD-PARTY-LICENSES.txt`, asset attribution, render manifests.
- `src/icons.rs`, `src/app_icons.rs`, `src/ui_containers.rs`: icon standards, app icon exports, the UI container rule.
- `src/features.rs`, `src/book.rs`, `src/params.rs`: `docs/features.md`, the book, and the parser for command parameter docs.
- `src/external_corpus.rs`, `src/web_offline.rs`, `src/web_offline_test.rs`: the nightly STEP corpus, the web service worker, the offline browser test.
- `mesh-decisions.txt`: functions that still decide geometry with a mesh. It only shrinks.

## Rules

- A new subcommand needs a `USAGE` entry, a `match` arm in `main` and its own module (in the sorted `mod` list). `skills` fails when a doc names a `cargo xtask <name>` that does not exist.
- A new workspace crate needs a row in `layers::TABLE`. Keep the comment that says why it sits there.
- Never add a line to `mesh-decisions.txt` or `crates/engine/src/schema/unconverted.txt`. Fixes remove lines.
- Keep rule logic apart from `cargo metadata` so it can be unit-tested, as `layers.rs` does (`from_metadata` builds the model; `check` works on it).
- For deploy requests, follow AGENTS.md: no local builds or tests.

## Gotchas

- The root CLAUDE.md and `skills/solidcraft-tests` say `SOLIDCRAFT_TEST_JOBS`. The code reads `SKETCRAFT_TEST_JOBS`.
- `skills` (and so `ci`) expects a router table of `skills/<name>/SKILL.md` rows in AGENTS.md, under 5000 characters. The current AGENTS.md has no such table and is longer, so this check fails.
- The nextest runner uses the `local-ci` profile from `.config/nextest.toml`. That file is not in this tree.
- Usage text and some messages still say `solidcraft-cli` and `solidcraft-*`. The packages are `sketcraft-*`. `layers::short_name` strips the `sketcraft-` prefix.
- `wasm` checks only crates in a numbered layer, plus `sketcraft-web`. Standalone crates such as `skp` are checked through the crates that use them.
- The tree has no `rustfmt.toml`, but most code uses lines far wider than rustfmt's default of 100. Check before running `cargo fmt` or the `ci` fmt step.
