# CLAUDE.md — sketcraft-web (browser app)

SketCraft in the browser: the same `sketcraft_ui_egui::SolveApp` as the desktop app, run by
eframe's web runner (wgpu: WebGPU, WebGL2 fallback) and built with Trunk for
`wasm32-unknown-unknown`. This crate adds the browser host: files in OPFS and on disk, settings in
browser storage, release channels, install prompts, the Gamepad 3D mouse and a JS API over the
control channel. Apps are exempt from `cargo xtask layers`. See the root CLAUDE.md and AGENTS.md.

## Commands

```sh
trunk serve                          # from the repo root: http://127.0.0.1:8080, proxies /api/ and /auth/ to localhost:8081
trunk build --release --locked       # from the repo root: static site in dist/ (CI uses this, see .github/workflows/pages.yml)
cargo check -p sketcraft-web --target wasm32-unknown-unknown
cargo test -p sketcraft-web          # native unit tests of the cfg(any(wasm32, test)) modules
cargo xtask web-offline-test         # service worker checks
```

- Needs the `wasm32-unknown-unknown` target and `trunk`.
- A native `cargo run -p sketcraft-web` only prints that it runs in the browser.
- Compilation check: read the Trunk log `/tmp/ketchup-trunk.log` of the running `trunk serve`
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
- `src/install.rs`, `src/easyprint.rs`, `src/space_mouse.rs` (Gamepad API).
- `build.rs`: links wasm with an 8 MiB stack (the kernel recurses; wasm has no guard page).
- `index.html`, `Trunk.toml`, `sw.js` (service worker), `manifest.webmanifest`,
  `channel-storage.txt`: this folder's own Trunk setup (port 8781, output `dist/web`, runs
  `cargo xtask web-offline` after a build to fill in `sw.js`).

## Rules

- Application code is Rust. The only JavaScript allowed is `sw.js` and the small inline scripts in
  `index.html`; keep them minimal (`skills/solidcraft-performance/SKILL.md`).
- Gate browser code with `cfg(target_arch = "wasm32")`. Logic you want tested natively goes in a
  `cfg(any(target_arch = "wasm32", test))` module, like `keep.rs` and `channel.rs`.
- Every storage key goes through `channel::key(...)` so channels never share designs or settings.
- Preferences must survive an immediate reload: write them in the frame they change, as `main.rs`
  does (AGENTS.md, "Persistent options and settings").
- Downloads and dropped files are hostile input: bound sizes (`skills/solidcraft-crash-safety/SKILL.md`).
- No dependency that cannot build for wasm32. Do not block the page's thread with long work.

## Gotchas

- There are two Trunk setups. The repo root `Trunk.toml` + `index.html` (port 8080, `/api/` and
  `/auth/` proxies, output `dist/`) is what `trunk serve`, the root CLAUDE.md and CI use. The ones in
  this folder (port 8781, `dist/web`, offline hook) are separate.
- `install.rs` reads `window.sketcraftInstall`, which only this folder's `index.html` sets. The
  root `index.html` has no `beforeinstallprompt` script, so the root build gets no install offer.
- Some messages still say `apps/solidcraft-web`; the folder is `apps/sketcraft-web`.
- `packaging/web-channels.sh`, named in `channel-storage.txt`, is not in this tree.
