# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Read `AGENTS.md` too. It holds the project's binding rules: persisted UI state, the release/deploy workflow, compilation checks against the Trunk log, modal button order, inspector tables, English-only UI text and the shared PDF header. These are not repeated here.

## What this repository is

KetchUp is a SketchUp-style 3D edge/face editor in Rust. The same editor runs natively (Windows/macOS/Linux) and in the browser (WebAssembly via Trunk). Crates use the `sketcraft-*` prefix; the product name shown to users is KetchUp.

## Commands

```sh
cargo run -p sketcraft-ui              # desktop app (binary `sketcraft`; needs a C++17 compiler for the native SKP reader)
trunk serve                            # browser app at http://127.0.0.1:8080 (needs wasm32-unknown-unknown + trunk)
cargo build -p sketcraft-ui --release --locked
trunk build --release --locked         # static site in dist/

cargo check --workspace --locked
cargo check --workspace --target wasm32-unknown-unknown --locked
cargo test --workspace --locked
cargo test -p sketcraft-core <name>    # one test in the core
cargo test -p sketcraft-ui <name>      # one test in the UI
python3 scripts/run_feature_audit.py   # sequential audit incl. ignored GPU/workload tests
```

GPU comparison and workload benchmarks are `#[ignore]` tests in `sketcraft-ui`. See the README for the exact invocations. Release scripts need Python 3.11+ (`tomllib`).

For deploy requests, follow `AGENTS.md`: no local builds or tests. CI (`.github/workflows/build.yml`, `pages.yml`) builds and publishes on a `v*` tag that matches `[workspace.package].version`, which must be a stable `X.Y.Z`. Release notes go in `assets/release-notes.json`.

## Architecture

The workspace has two crates.

- `crates/sketcraft-core`: the model, with no GUI or GPU dependencies. `Document`/`Model` holds double-precision vertices and edges, and planar faces are discovered and triangulated from the edge graph. It also has objects (groups/components), tags, inference/snapping, picking, measurements, camera, display settings, units and bounded undo history. `skp.rs` plus `skp_native.cpp` (compiled by `build.rs` with `cc`) read SketchUp `.skp` files natively. Model-specific state such as tag visibility and entity assignments is stored in the document.
- `crates/sketcraft-ui`: the egui/eframe app. It is a library (`sketcraft_ui::KetchupApp`) plus the `sketcraft` binary (`src/main.rs`, a native and wasm entry point). `app.rs` holds the app state and gesture routing, and each tool has its own module. `sidebar.rs` holds the inspector and shared preferences (`InspectorPreferences`, section persistence, `persist_inspector_changes`), and `modal.rs`, `inspector_table.rs` and `pdf_header.rs` are the shared shells `AGENTS.md` requires. Viewports use a wgpu/WGSL edge renderer (`edges.wgsl`) with CPU projection. Rendering is event-driven, not a continuous loop. Browser-only helpers are the JS files in `src/` (SKP worker, project save, cursor). Desktop updates use Velopack (`updates.rs`), and the version is pinned to match `VPK_VERSION` in CI and `scripts/desktop_release.py`.
- `index.html` hosts the wasm app (canvas `ketchup-canvas`, browser shortcut guards that must stay in sync with `KetchupApp::keyboard`). `Trunk.toml` proxies `/api/` and `/auth/` to the account server on `localhost:8081`. The account server URL comes from `KETCHUP_SERVER_URL` or `.env`, read in `crates/sketcraft-ui/build.rs`, or from `--server-url` at runtime.
