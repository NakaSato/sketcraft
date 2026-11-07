# CLAUDE.md — sketcraft (desktop app)

The desktop binary for Windows, macOS and Linux. It starts eframe with the shared
`sketcraft_ui_egui::SolveApp` and adds what only a native host has: the window and graphics
backend choice, native menus (macOS), the loopback control server, crash capture, 3D mouse
readers, window placement and the settings writer. Apps are exempt from `cargo xtask layers`.
See the root CLAUDE.md and AGENTS.md.

## Commands

```sh
cargo run -p sketcraft                                # needs a C++17 compiler (SKP reader, via engine → io → skp)
cargo run -p sketcraft -- --sample --control 9000     # sample design + control channel on port 9000
cargo run -p sketcraft -- --safe-mode                 # default settings, no recovery offer, software renderer
cargo test -p sketcraft                               # unit tests inside src/*.rs (no tests/ dir)
cargo test -p sketcraft prefs_writer                  # one test module by filter
cargo build -p sketcraft --release --locked
```

For a deploy request, do not build or test locally: follow AGENTS.md ("Release workflow").
Driving the running app: `skills/sketcraft-agent-tools/SKILL.md`.

## Layout

- `src/main.rs`: `fn main`, argument parsing (`--control`, `--sample`, `--crash-log`,
  `--safe-mode`, a file to open), `eframe::run_native`, the `PrefsWriter` that writes changed prefs
  each frame (rate-limited), and the `Services` the UI gets (rfd file pickers, …).
- `src/startup.rs`: getting the window open whatever state settings or the GPU are in (checks
  eframe's `app.ron` with `ron`).
- `src/control_server.rs`: loopback JSON-lines control server. Requests go to `sketcraft_ui_egui::control`.
- `src/nav_bridge.rs`: loopback bridge that lets a device's own driver program move the view.
- `src/space_mouse.rs` + `src/space_mouse/`: 3D mouse readers (`hidraw.rs`, `spacenavd.rs` on Linux,
  `windows_hid.rs` on Windows, hidapi; macOS also uses `sketcraft-3dx-client`).
- `src/device_lost.rs`, `src/window_fit.rs`, `src/come_forward.rs`: GPU loss, keeping the window
  on screen, raising the window on close prompts.
- Windows: `native_crash.rs`, `native_frame.rs`. macOS: `native_crash_macos.rs`, `native_menu.rs`,
  `open_documents.rs`. Linux: `desktop_theme.rs`, `desktop_chooser.rs`.
- `app_icon` is loaded with `#[path]` from `assets/icons/app/shared/source/app_icon.rs`.
- `build.rs`: on Windows only, embeds the `.ico` and a version resource (`embed-resource`).

## Features

`default = ["raytrace-cpu", "bundled-render-library", "profile"]`, each forwarding to
`sketcraft-engine`. `profile` compiles the profiler in; it still records nothing until asked.

## Rules

- Keep the app thin: behaviour belongs in commands (`crates/engine`) and UI in `crates/ui-egui`.
- `unsafe_code` is `deny`, not `forbid`. Only these files may `#[allow(unsafe_code)]`:
  `open_documents.rs`, `native_crash.rs`, `native_crash_macos.rs`, `native_frame.rs`,
  `space_mouse/windows_hid.rs`. Add no new unsafe without a case that has no safe API
  (`skills/sketcraft-crash-safety/SKILL.md`). `Cargo.toml` copies the workspace lints: keep them in sync.
- Gate platform code with `cfg(target_os = ...)`; every platform must still build.
- Preferences must survive an immediate restart: change `Prefs` so `PrefsWriter::store` writes
  them (AGENTS.md, "Persistent options and settings").
- The control channel is local-only with a launch token. Never paste the token anywhere.

## Gotchas

- Env vars use the `SKETCRAFT_` prefix (most are read through `sketcraft_engine::geom::env`, which
  falls back to the legacy `SOLVECRAFT_` prefix):
  `SKETCRAFT_CONTROL_PORT`, `SKETCRAFT_SAFE_MODE`, `SKETCRAFT_SETTINGS_DIR`, `SKETCRAFT_TITLEBAR`.
- Without `--control`, the control channel still opens on the default port unless Preferences turns it off.
