# SketCraft

[![Try SketCraft in your browser](https://img.shields.io/badge/Try_SketCraft_in_your_browser-d63b32?style=for-the-badge)](https://nakasato.github.io/sketcraft/play/)
[![Download the latest release](https://img.shields.io/badge/Download-555555?style=for-the-badge)](https://github.com/NakaSato/sketcraft/releases/latest)

SketCraft is a SketchUp-style 3D modeler written in Rust. The same editor runs as a native
Windows, macOS or Linux application and in a desktop browser through WebAssembly.

It is built on a parametric CAD workspace, so SketchUp's way of working sits on top of a real
solid-modeling core:

- **SketchUp first.** The app opens on the **SketchUp** ribbon tab, which holds SketchUp's Large
  Tool Set: Select, Eraser and Paint; Line, Arc, Rectangle, Circle and Polygon; Push/Pull,
  Offset, Move, Scale and Follow Me; Tape Measure, Dimensions, Text and Section Plane;
  Components and Tags; Orbit, Pan, Zoom and Zoom Extents. Drawing tools pick their plane with
  the first click. The model screen has a sky and ground, long red, green and blue axes, and a
  perspective camera. Preferences › Shortcuts › **Use SketchUp Keys** gives each tool its
  SketchUp key.
- **CAD when you need it.** The Model, Insert, Inspect, Manage and View tabs, plus Surface,
  Assemble, Sheet Metal and Weldments (hidden at first; show them from the ribbon's **+** or
  Settings › Layout › Toolbar tabs): constrained sketches, a feature timeline, assemblies and
  joints, analysis tools and a Render workspace.
- **Files.** Designs are saved as `.sc`, and older `.sketcraft` and `.solvecraft`
  files still open. Import and export cover STEP, IGES, STL, 3MF, OBJ, glTF, DXF and SVG, plus
  SketchUp `.skp`, Rhino `.3dm`, FreeCAD, point clouds (`.ply`, `.xyz`, `.pts`) and URDF,
  depending on the format's direction.
- **Automation.** Every action is a command that the toolbar, the command palette, the CLI
  (`sketcraft-cli`), a local control channel and an MCP server all run the same way.

For how to use the app, see the [user documentation](docs/README.md). This README covers
building, testing and releasing it.

## Prerequisites

- Rust 1.95 or newer, installed with rustup.
- A C++17 compiler for the native SketchUp reader: GCC or Clang on Linux, Xcode Clang on
  macOS, MSVC on Windows. Browser builds do not need it.
- A GPU and driver supported by wgpu. The browser uses WebGPU and falls back to WebGL2.
- On Linux, development libraries for X11/Wayland and xkbcommon. On Debian/Ubuntu:
  `sudo apt install build-essential pkg-config libx11-dev libxcb1-dev libxkbcommon-dev libwayland-dev libegl1-mesa-dev`.
- On Windows, the MSVC Rust toolchain and the Visual Studio C++ build tools.
- On macOS, the Xcode Command Line Tools.

## Run on desktop

```sh
cargo run -p sketcraft
```

The development profile is optimized (with debug info, assertions and overflow checks kept),
so the debug build is usable for modeling. A first build of the whole workspace takes a while.

On Linux, register the launcher and icons once after building:

```sh
python3 scripts/install_linux_desktop.py                         # points at target/debug/sketcraft
python3 scripts/install_linux_desktop.py target/release/sketcraft
```

It installs into `$XDG_DATA_HOME` (or `~/.local/share`) without root. Rerun it if you move the
executable.

## Run in a browser

Install the WebAssembly target and Trunk once:

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
```

Then, from the repository root:

```sh
trunk serve
```

Open <http://127.0.0.1:8080>. Work is saved in the browser for that origin, so keep the same
address and port to get it back. Trunk generates the JavaScript that loads the WebAssembly
program; no Node.js or backend is needed.

## Build for distribution

Desktop, on the OS you are targeting:

```sh
cargo build -p sketcraft --release --locked
```

The executable is `target/release/sketcraft` (`sketcraft.exe` on Windows). This makes an
executable, not an installer or a signed bundle.

Browser:

```sh
trunk build --release --locked --public-url ./
```

Serve `dist/` from any static web server, over HTTPS for WebGPU (localhost also works).
`--public-url ./` lets it be served from a subdirectory.

## Test and check

```sh
cargo xtask test                       # every test binary, `ci` profile
cargo xtask test -- <filter>           # the filter is passed to each test binary
cargo test -p sketcraft-ui-egui --lib keymap   # one crate, one area
cargo xtask ci                         # before committing: fmt, lints, clippy -D warnings, tests, wasm
cargo xtask layers                     # no crate may depend on a higher layer
cargo xtask known-failing              # tests/KNOWN_FAILING_TESTS against the actual failures
```

A failing test ships with its fix or gets a line in `tests/KNOWN_FAILING_TESTS`.
`SKETCRAFT_TEST_JOBS=N` limits how many test commands run at once. UI scenario tests are JSON
files in `crates/ui-egui/tests/scenarios/`; see [`crates/ui-egui/CLAUDE.md`](crates/ui-egui/CLAUDE.md).

Browser shortcut handling can be tested without building the app:

```sh
node --test scripts/test_web_shortcuts.cjs
```

## Repository layout

```
L0 geom → L1 sketch, kernel, render → L2 doc → L3 io → L4 engine → L5 ui-egui → apps
```

- `crates/geom`, `crates/sketch`, `crates/kernel`, `crates/render`: geometry, the sketch
  solver, the B-rep kernel wrapper (around the external `solidcore` kernel) and scene data.
- `crates/doc`: the design document. `crates/io`: file formats.
  `crates/skp`: the SketchUp reader (vendored OpenSKP C++ parser, desktop only).
- `crates/engine`: `Session` and the command registry. Every user action is a `CommandSpec` in
  `crates/engine/src/cmd/`, run through `Session::execute`.
- `crates/ui-egui`: the egui front end: ribbon, viewport, dialogs, preferences. The SketchUp
  tab lives in `workspace.rs`, its drawing tools in `sketchup_draw.rs` and its model screen in
  `sketchup_screen.rs`. `crates/gpu`: the wgpu pipelines.
- `apps/sketcraft` (desktop), `apps/sketcraft-web` (browser, built by Trunk from `index.html`),
  `apps/sketcraft-cli` (headless CLI and MCP server). `xtask`: repository tooling.
- `vendor/`: patched `egui-wgpu` and `winit`, outside the workspace.
- `skills/*/SKILL.md`: short contributor guides (commands, tests, UI, file formats,
  preferences, performance).

Project rules (persisted settings, modal button order, inspector tables, English-only UI
text, the release workflow) are in [`AGENTS.md`](AGENTS.md).

## Releases

[Build desktop](.github/workflows/build.yml) runs when a `vX.Y.Z` tag matching
`[workspace.package].version` in `Cargo.toml` is pushed. It builds with stable Rust and the
committed `Cargo.lock`, runs the update tests and packages the app with Velopack for Windows
x64, macOS Apple Silicon and Linux x64 (AppImage). When all three succeed it publishes a GitHub
Release with the installers, portable packages, update feeds and `SHA256SUMS`. A published
version cannot be overwritten.

To release:

1. Add the version's user-facing notes to `assets/release-notes.json` (newest first). They
   become the GitHub Release text and **Help › Release notes**.
2. Bump the workspace version and the lockfile, then check:

   ```sh
   python3 scripts/desktop_release.py version --tag vX.Y.Z
   python3 scripts/desktop_release.py notes --tag vX.Y.Z
   ```

3. Refresh the screenshot from an existing desktop executable (it never builds one):
   `python3 -B scripts/capture_readme_desktop.py`, or pass `--binary /path/to/sketcraft`.
4. Commit and push, then push the tag. CI does the builds and tests; a deploy does not run them
   locally.

`VPK_VERSION` in the workflow, the Velopack Rust dependency and the version in
`scripts/desktop_release.py` are pinned together (1.2.161); update them together. Only the
packaging machine needs .NET 8.

### Signing

Without signing secrets CI produces unsigned development packages. Set the repository variable
`KETCHUP_REQUIRE_SIGNING=true` to make packaging fail when Windows or macOS signing is missing.
See Velopack's [signing guide](https://docs.velopack.io/packaging/signing).

| Secret | Platform | Value |
|---|---|---|
| `AZURE_CREDENTIALS` | Windows | Azure service-principal JSON for `azure/login` |
| `WINDOWS_AZURE_SIGNING` | Windows | JSON with `Endpoint`, `CodeSigningAccountName`, `CertificateProfileName` |
| `MACOS_CERTIFICATES_P12` | macOS | Base64 `.p12` with the Developer ID Application and Installer certificates and keys |
| `MACOS_CERTIFICATE_PASSWORD` | macOS | The `.p12` password |
| `MACOS_SIGN_APP_IDENTITY` | macOS | Developer ID Application identity |
| `MACOS_SIGN_INSTALL_IDENTITY` | macOS | Developer ID Installer identity |
| `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD` | macOS | Notarization account, team and app-specific password |

The Azure account needs the **Artifact Signing Certificate Profile Signer** role. Partly
configured signing fails rather than silently producing unsigned files.

### Website

[Deploy GitHub Pages](.github/workflows/pages.yml) builds the browser editor into
`dist/site/play/` and the landing page from [`site/`](site/index.html) into `dist/site/`. It runs
after a release workflow succeeds, or by hand from the Actions tab. Set **Settings › Pages ›
Source** to **GitHub Actions** once. The site is at <https://nakasato.github.io/sketcraft/>, the
editor at <https://nakasato.github.io/sketcraft/play/>. Browser work is saved locally; GitHub Pages
is not model storage.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE). See
[`NOTICE`](NOTICE) and [`ATTRIBUTION.md`](ATTRIBUTION.md) for third-party components.
