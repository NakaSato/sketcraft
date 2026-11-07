# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Read `AGENTS.md` too. It holds the project's binding rules: persisted UI state, the release/deploy workflow, compilation checks against the Trunk log, modal button order, inspector tables, English-only UI text and the shared PDF header. These are not repeated here.

## What this repository is

SketCraft is a SketchUp-style 3D edge/face editor in Rust that runs natively (Windows/macOS/Linux) and in the browser (WebAssembly via Trunk). It is built on a fork of an upstream parametric CAD workspace. Crates, binaries and the contributor skills in `skills/` use the `sketcraft-*` prefix. The older name SolveCraft remains only where the app still reads it: legacy file extensions, format tags, app ids, settings keys and data folders. Some docs those skills mention (`docs/book/`, `ROADMAP.md`) are not in this tree.

Shared UI building blocks live in `crates/ui-egui/src/ui_kit/`: sections, tables, modals, footers and disclosures. Preferences are in `crates/ui-egui/src/prefs.rs`. Use these before writing new widgets.

## Commands

```sh
cargo run -p sketcraft                 # desktop app (needs a C++17 compiler for the native SKP reader)
trunk serve                            # browser app at http://127.0.0.1:8080 (needs wasm32-unknown-unknown + trunk)
cargo build -p sketcraft --release --locked
trunk build --release --locked         # static site in dist/

cargo xtask test                       # every test binary, `ci` profile (much faster to build than --release)
cargo xtask test -- <filter>           # filter is passed to each test binary
cargo test -p sketcraft-skp <name>     # one test in one crate
cargo xtask ci                         # pre-commit gate: fmt, lints, clippy -D warnings, tests, wasm
cargo xtask layers                     # fails if a lower crate depends on a higher one
cargo xtask known-failing              # checks tests/KNOWN_FAILING_TESTS against actual failures
cargo xtask oracle                     # reference tests in tests/oracle/<case>/{recipe,expected}.json
```

`SKETCRAFT_TEST_JOBS=N` limits how many test commands run at once. A failing test must ship with its fix or get a line in `tests/KNOWN_FAILING_TESTS`.

For deploy requests, follow `AGENTS.md`: no local builds or tests. CI (`.github/workflows/build.yml`, `pages.yml`) builds and publishes on a `v*` tag that matches `[workspace.package].version`. Release notes go in `assets/release-notes.json`, checked with `python3 scripts/desktop_release.py version|notes --tag vX.Y.Z`.

## Architecture

The workspace has `crates/*`, `apps/*` and `xtask`. `vendor/` holds patched `egui-wgpu` and `winit` and is excluded from the workspace. `unsafe_code` is forbidden workspace-wide.

`cargo xtask layers` enforces this layering:

```
L0 geom → L1 sketch, kernel, render → L2 doc → L3 io → L4 engine → L5 ui-egui → apps
```

- Only `ui-egui` and the apps may use egui, eframe, winit, wgpu or rfd. `kernel` wraps the external `solidcore` B-rep kernel (pinned git rev). `step`, `smlib` and `profile` support it.
- `engine` owns `Session` and the command registry. Every user-visible action is a `CommandSpec` in `crates/engine/src/cmd/`, run through `Session::execute(id, params)`. Toolbar, dialogs, CLI, control channel and MCP (`crates/mcp`) all go through this path. Commands that change the document are undoable by default. Programmatic calls never open dialogs; only `ui.start` begins interactive tools. Details: `skills/sketcraft-commands/SKILL.md`.
- `ui-egui` is a thin front end. Panels read engine state and act through `app.run(id, params)` / `app.start(id)`. UI scenario tests are JSON files in `crates/ui-egui/tests/scenarios/`.
- `sketcraft-skp` (`crates/skp`) is the SketchUp reader: the vendored OpenSKP C++ parser (`vendor/skp/native` and `src/skp_native.cpp`, compiled by `build.rs` with `cc`, desktop only) and a small `Model` of faces, groups and hidden state that triangulates faces. `crates/io/src/skp_in.rs` turns it into mesh bodies for `.skp` import.
- Apps: `apps/sketcraft` is the desktop binary (native menus, control server, crash handling, Velopack updates). `apps/sketcraft-web` is the wasm entry used by `index.html`/Trunk; it reaches the account server under `/api/` and `/auth/` (see `.env.example`); `trunk serve` has no proxy for them, since there is no `Trunk.toml`. `apps/sketcraft-cli` is a headless CLI and MCP server.
- `xtask` holds the repo tooling: tests, ci, layers, i18n, icons, features, licences, skills, oracle, kernel cases.

## Where to look

`skills/*/SKILL.md` are short contributor guides, chosen by task: `sketcraft-tests` (test placement and running), `sketcraft-ui` (dialogs, panels, icons), `sketcraft-ui-text` (labels and i18n keys like `cmd.<id>` / `help.<id>`), `sketcraft-file-formats` (saved data and import/export), `sketcraft-preferences`, `sketcraft-crash-safety` (hostile input), `sketcraft-performance` (window thread, wasm). User docs are in `docs/`.
