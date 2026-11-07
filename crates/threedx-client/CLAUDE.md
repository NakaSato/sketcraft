# CLAUDE.md — sketcraft-3dx-client

A client of the 3Dconnexion driver (3DxWare) on macOS. The driver holds the HID device while it
runs, so the app talks to `3DconnexionClient.framework` instead, loaded with `dlopen` at run time.
It is "Standalone" in `cargo xtask layers` (no workspace dependencies). Only `apps/sketcraft`
uses it, on macOS. See the root CLAUDE.md.

The package is `sketcraft-3dx-client`; the folder is `crates/threedx-client`.

## Commands

```sh
cargo check -p sketcraft-3dx-client
cargo test -p sketcraft-3dx-client        # the unit tests live in src/macos.rs: macOS only
cargo run -p sketcraft-3dx-client --example dump   # prints driver events for 60 s
```

The driver sends nothing to a program outside an app bundle, so `dump` shows only `DeviceAdded`.
Use the app's `.app` bundle to see the cap move.

## Layout

- `src/lib.rs`: the platform-neutral API: `start(name, sink, connected)`, `stop()`, `Event`
  (`DeviceAdded`, `DeviceRemoved`, `Axes`, `Buttons`) and `Error`. Off macOS, `start` returns
  `Error::Unsupported`.
- `src/macos.rs` (`cfg(target_os = "macos")`): `dlopen`/`dlsym`, the C declarations, the two driver
  callbacks, the client thread. Reads the driver's packed state struct by byte offsets.
- `examples/dump.rs`: manual check.

## Rules

- `unsafe_code` is `deny` here, not `forbid`. Only `src/macos.rs` allows it (`#![allow(unsafe_code)]`)
  because loading a framework has no safe API. Keep unsafe there, and small.
  `Cargo.toml` copies the workspace lints by hand: keep them in sync with `[workspace.lints]`.
- Never link against, bundle or redistribute the framework. The declarations are our own.
- The app must run without the driver: return `Error::NotInstalled` / `MissingSymbol`, never panic.
  Callbacks run under `catch_unwind`. See `skills/solidcraft-crash-safety/SKILL.md`.
- Call `stop()` when the app ends. The driver keeps a dead client registered otherwise, and a later
  client gets nothing until the device is replugged.

## Gotchas

- `start` returns once the client thread runs. The registration outcome arrives later through the
  `connected` callback, on the client's thread (registration needs the main run loop).
- Only one client per process (`Error::AlreadyStarted`).
- The doc comment in `examples/dump.rs` says `solidcraft-3dx-client`; use `sketcraft-3dx-client`.
