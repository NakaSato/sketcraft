# CLAUDE.md — sketcraft-space-mouse

3D mouse input that does not depend on the device API or the UI: a HID report descriptor reader,
report decoding into six axes and buttons, calibration, and measured default button jobs. Layer
L0 with no workspace dependencies. The apps open the device (hidapi, hidraw, spacenavd, the
browser Gamepad API) and feed it bytes. See the root CLAUDE.md.

## Commands

```sh
cargo test -p sketcraft-space-mouse
cargo test -p sketcraft-space-mouse --features serde   # the serde paths (ui-egui uses this feature)
cargo check -p sketcraft-space-mouse --target wasm32-unknown-unknown
```

Tests are unit tests inside each module. There is no `tests/` directory.

## Layout

- `src/lib.rs`: re-exports only.
- `src/descriptor.rs`: `Descriptor::parse` reads a HID report descriptor (from the public USB HID
  spec); `Layout::decode` turns an input report into axes and buttons. `FULL_SCALE`, `MAX_BUTTONS`.
- `src/calibration.rs`: the calibration `Wizard`/`Recording` and `Recording::solve` → `Calibration`
  that maps raw axes onto 3Dconnexion's axes. Largest module.
- `src/buttons.rs` (public): default button jobs for measured devices (`MEASURED`).

## Rules

- No workspace dependencies. `serde` is optional (feature `serde`), used by `crates/ui-egui` to save
  calibrations in preferences. Keep the crate building without it.
- Malformed descriptors and reports are hostile input: never panic, return less information.
  The crate root denies `unwrap`/`expect`/`panic!`; `unsafe` is forbidden.
- Nothing here touches the camera, commands or UI. Apply axes in `crates/ui-egui/src/space_mouse*.rs`
  and read devices in `apps/sketcraft/src/space_mouse*` or `apps/sketcraft-web/src/space_mouse.rs`.
- Button defaults come only from our own measurements. Do not guess a device's buttons.
- Saved calibrations live in prefs: keep deserialization compatible with older saved data
  (see AGENTS.md, "Persistent options and settings").
