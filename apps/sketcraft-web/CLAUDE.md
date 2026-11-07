# CLAUDE.md — sketcraft-web (browser app)

SketCraft in the browser: the same `sketcraft_ui_egui::SketApp` as the desktop app, run by
eframe's web runner (wgpu: WebGPU, WebGL2 fallback) and built for `wasm32-unknown-unknown` by
Trunk (`Trunk.toml`). This crate adds the browser host: files in OPFS and on disk, settings in
browser storage, release channels, install prompts and a JS API over the
control channel. Apps are exempt from `cargo xtask layers`. See the root CLAUDE.md and AGENTS.md.

## Commands

```sh
trunk serve                          # debug build, served on http://127.0.0.1:8781, rebuilt on change
trunk serve --release                # optimized (slow to build; wasm-opt -Oz)
trunk build --release                # static site in dist/web
cargo check -p sketcraft-web --target wasm32-unknown-unknown
cargo test -p sketcraft-web          # native unit tests of the cfg(any(wasm32, test)) modules
cargo xtask web-offline-test         # service worker checks
```

- One-time setup: `rustup target add wasm32-unknown-unknown` and Trunk (`brew install trunk` or
  `cargo install trunk --locked`). Trunk fetches the `wasm-bindgen` and `wasm-opt` that match
  Cargo.lock by itself. Run the commands above from `apps/sketcraft-web`.
- The first build compiles the kernel (`sketcore`) for wasm and takes a long time (tens of
  minutes); later builds reuse it and rebuild only what changed.
- A native `cargo run -p sketcraft-web` only prints that it runs in the browser.
- Compilation check: read `/tmp/sketcraft-web.log` of the running `trunk serve`
  (AGENTS.md, "Compilation checks"). Do not start a new build just for this.

## Layout

- `src/main.rs`: entry point. URL flags `?webgl`, `?webgpu`, `?sample`, `?embed=editor`; the
  settings writer (`prefs_key()`, cross-tab `storage` events); keys left to the browser
  (Ctrl+Tab); the `mod web` with the wasm-only startup.
- `src/files.rs`: designs in the origin private file system, mirrored into the engine's in-memory
  files (`sketcraft_engine::io::vfs`); pickers and drops.
- `src/disk.rs`: File System Access API handles (kept in IndexedDB) so Save writes back to disk.
- `src/keep.rs`: keep unsaved work in the browser (plain Rust, tested natively).
- `src/channel.rs`: stable / rc / edge channels keep separate storage keys.
- `src/api.rs`: promise-based JS access to the control channel. `src/linked.rs`: bounded downloads.
- `src/install.rs`, `src/easyprint.rs`.
- `build.rs`: links wasm with an 8 MiB stack (the kernel recurses; wasm has no guard page).
- `index.html` (the page template: Trunk acts on its `<link data-trunk ...>` tags), `sw.js`
  (service worker), `manifest.webmanifest`, `channel-storage.txt`. Release builds run
  `cargo xtask web-offline` to fill in `sw.js`.

## Rules

- Application code is Rust. The only JavaScript allowed is `sw.js` and the small inline scripts in
  `index.html`; keep them minimal (`skills/sketcraft-performance/SKILL.md`).
- Gate browser code with `cfg(target_arch = "wasm32")`. Logic you want tested natively goes in a
  `cfg(any(target_arch = "wasm32", test))` module, like `keep.rs` and `channel.rs`.
- Every storage key goes through `channel::key(...)` so channels never share designs or settings.
- Preferences must survive an immediate reload: write them in the frame they change, as `main.rs`
  does (AGENTS.md, "Persistent options and settings").
- Downloads and dropped files are hostile input: bound sizes (`skills/sketcraft-crash-safety/SKILL.md`).
- No dependency that cannot build for wasm32. Do not block the page's thread with long work.

## Gotchas

- The repo root `index.html` is a leftover page from the removed KetchUp UI; its canvas id does
  not match `CANVAS_ID`, so nothing builds from it. This folder's `index.html` is the page.
- The local server has no `/api/` or `/auth/` proxy to the account server.
- `packaging/web-channels.sh`, named in `channel-storage.txt`, is not in this tree.
