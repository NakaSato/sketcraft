# CLAUDE.md — sketcraft-io

File formats: design files (JSON and `.sc` containers), drawings, STL, OBJ, 3MF, glTF, PLY/XYZ point clouds, DXF, SVG, URDF, STEP and IGES (through the kernel), FreeCAD and Rhino (through `cadmpeg`), and SketchUp (through `sketcraft-skp`). It is layer L3. It may use `geom`, `kernel`, `doc`, `profile` and `skp`. See the root CLAUDE.md.

## Commands

```sh
cargo test -p sketcraft-io                               # all unit tests (there is no tests/ dir)
cargo test -p sketcraft-io mutate::                      # the hostile-input tests in src/mutate.rs
cargo test -p sketcraft-io --release -- --ignored        # manual IO and save timings
```

- Feature `trust-unknown-source` (tests only): trust a model cache even when the build has no git commit. The engine turns it on in its dev-dependencies.

## Layout

- `src/lib.rs`: `IoError`, `Format`, `read_design` / `write_design`, `step_import_feature`, `mesh_import_feature`, design extensions (`DESIGN_EXT = "sc"`, `LEGACY_DESIGN_EXTS`).
- `src/design.rs`, `src/design/`: versioned design containers, drawings, thumbnails, and the model cache (`native_cache.rs`).
- `src/formats.rs`: the format registry. File pickers, dialog filters and the web `accept` list come from it.
- `src/limits.rs`: the largest STEP and design files read (defaults, a 16 GB ceiling, fixed on wasm).
- `src/vfs.rs`: file access. Disk on desktop; an in-memory store on wasm that the web host fills.
- `src/safe_write.rs`: crash-safe writes (temp file, flush, rename, optional `.bak`).
- `src/design_lock.rs`: `.~<file>.lock` files and file stamps for designs open on two computers (desktop only).
- Mesh readers and writers: `threemf.rs`, `obj.rs`, `gltf_in.rs`, `ply.rs`, `ply_complex.rs`, `point_cloud.rs`, `skp_in.rs`.
- 2D: `dxf.rs`, `dxf_out.rs`, `svg.rs`, `sketch2d.rs`, `paper.rs` and `paper/` (sheets as SVG).
- `src/native_cad.rs`, `src/native_cad/`: FreeCAD and Rhino files to STEP, with fixtures.
- `src/urdf.rs`, `src/catalog.rs` (material libraries), `src/legacy_dir.rs` (older app data folders).
- `src/mutate.rs` (test only): runs each reader over mutated files from `sketcraft-testkit`.
- `build.rs`: sets `SKETCRAFT_SOURCE_COMMIT` and `SKETCRAFT_EVALUATION_SOURCE`, which key the model cache.

## Rules

- Every byte read from a file is hostile. Check sizes against `limits`, cap counts and nesting, and return `IoError`, never panic. A new reader gets a case in `src/mutate.rs`. See `skills/solidcraft-crash-safety/SKILL.md`.
- Write files through `safe_write` (`write_atomic`).
- Read and write through `vfs` so the browser build works. Native-only code needs `#[cfg(not(target_arch = "wasm32"))]`.
- A change to what a design saves goes in a pending format file in `crates/doc`, not here: `skills/solidcraft-file-formats/SKILL.md`.
- Keep older designs opening. Extensions in `LEGACY_DESIGN_EXTS` must keep working.
- A new format gets a row in `src/formats.rs`, so the UI and hosts pick it up.
- Imports become a new component named after the file (handled by the engine's `file.*` commands).
- Reference values in tests never come from SketCraft's own output.

## Gotchas

- `build.rs` hashes `.cargo/config.toml` (in `EVALUATION_FILES`). This tree has no such file. So the evaluation source is `unknown`, and non-test builds never trust a saved model cache.
- `build.rs` also needs git. A source tarball gets `unknown` too.
- SKP import works only on desktop. On wasm, `sketcraft_skp::import_skp` returns an error.
- `skp_in.rs` groups faces by their parent group name and skips hidden faces. It returns an error when a file has no visible faces.
- The `lib.rs` doc comment mentions `.solidcraft` files, but `LEGACY_DESIGN_EXTS` is `["sketcraft", "solvecraft"]`.
