# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Read `AGENTS.md` too. It holds the project's binding rules: persisted UI state, the release/deploy workflow, compilation checks against the Trunk log, modal button order, inspector tables, English-only UI text and the shared PDF header. These are not repeated here.

## What this repository is

KetchUp is a SketchUp-style 3D edge/face editor in Rust that runs natively (Windows/macOS/Linux) and in the browser (WebAssembly via Trunk). It is built on a fork of SolidCraft, a parametric CAD workspace. Crate and binary names still use the `sketcraft-*` prefix, and the contributor skills in `skills/` still say "SolidCraft". Some docs those skills mention (`docs/book/`, `ROADMAP.md`) are not in this tree.

Shared UI building blocks live in `crates/ui-egui/src/ui_kit/`: sections, tables, modals, footers and disclosures. Preferences are in `crates/ui-egui/src/prefs.rs`. Use these before writing new widgets.

## Commands

```sh
cargo run -p sketcraft                 # desktop app (needs a C++17 compiler for the native SKP reader)
trunk serve                            # browser app at http://127.0.0.1:8080 (needs wasm32-unknown-unknown + trunk)
cargo build -p sketcraft --release --locked
trunk build --release --locked         # static site in dist/

cargo xtask test                       # every test binary, `ci` profile (much faster to build than --release)
cargo xtask test -- <filter>           # filter is passed to each test binary
cargo test -p sketcraft-core <name>    # one test in one crate
cargo xtask ci                         # pre-commit gate: fmt, lints, clippy -D warnings, tests, wasm
cargo xtask layers                     # fails if a lower crate depends on a higher one
cargo xtask known-failing              # checks tests/KNOWN_FAILING_TESTS against actual failures
cargo xtask oracle                     # reference tests in tests/oracle/<case>/{recipe,expected}.json
```

`SOLIDCRAFT_TEST_JOBS=N` limits how many test commands run at once. A failing test must ship with its fix or get a line in `tests/KNOWN_FAILING_TESTS`.

For deploy requests, follow `AGENTS.md`: no local builds or tests. CI (`.github/workflows/build.yml`, `pages.yml`) builds and publishes on a `v*` tag that matches `[workspace.package].version`. Release notes go in `assets/release-notes.json`, checked with `python3 scripts/desktop_release.py version|notes --tag vX.Y.Z`.

## Architecture

The workspace has `crates/*`, `apps/*` and `xtask`. `vendor/` holds patched `egui-wgpu` and `winit` and is excluded from the workspace. `unsafe_code` is forbidden workspace-wide.

`cargo xtask layers` enforces this layering:

```
L0 geom → L1 sketch, kernel, render → L2 doc → L3 io → L4 engine → L5 ui-egui → apps
```

- Only `ui-egui` and the apps may use egui, eframe, winit, wgpu or rfd. `kernel` wraps the external `solidcore` B-rep kernel (pinned git rev). `step`, `smlib` and `profile` support it.
- `engine` owns `Session` and the command registry. Every user-visible action is a `CommandSpec` in `crates/engine/src/cmd/`, run through `Session::execute(id, params)`. Toolbar, dialogs, CLI, control channel and MCP (`crates/mcp`) all go through this path. Commands that change the document are undoable by default. Programmatic calls never open dialogs; only `ui.start` begins interactive tools. Details: `skills/solidcraft-commands/SKILL.md`.
- `ui-egui` is a thin front end. Panels read engine state and act through `app.run(id, params)` / `app.start(id)`. UI scenario tests are JSON files in `crates/ui-egui/tests/scenarios/`.
- `sketcraft-core` is the SketchUp-style model: `Document`/`Model` with vertices, edges, faces, objects and components, tags, inference/snapping, measurements, camera and display settings. It contains a C++ SKP reader (`skp_native.cpp`, compiled by `build.rs` with `cc`). `crates/io/src/skp_in.rs` uses it for `.skp` import. Model-specific state (tags, entity assignments) is stored in the document.
- Apps: `apps/sketcraft` is the desktop binary (native menus, control server, crash handling, Velopack updates). `apps/sketcraft-web` is the wasm entry used by `index.html`/Trunk; it reaches the account server through the `/api/` and `/auth/` proxies to `localhost:8081` (see `Trunk.toml`, `.env.example`). `apps/sketcraft-cli` is a headless CLI and MCP server.
- `xtask` holds the repo tooling: tests, ci, layers, i18n, icons, features, licences, skills, oracle, kernel cases.

## Where to look

`skills/*/SKILL.md` are short contributor guides, chosen by task: `solidcraft-tests` (test placement and running), `solidcraft-ui` (dialogs, panels, icons), `solidcraft-ui-text` (labels and i18n keys like `cmd.<id>` / `help.<id>`), `solidcraft-file-formats` (saved data and import/export), `solidcraft-preferences`, `solidcraft-crash-safety` (hostile input), `solidcraft-performance` (window thread, wasm). User docs are in `docs/`.
