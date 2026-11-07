# CLAUDE.md — sketcraft-skp

The SketchUp (`.skp`) reader. On desktop it wraps the vendored OpenSKP C++ parser, which turns a file into JSON. The crate decodes that JSON into a `Model` of vertices, edges, planar faces, groups, colours, tags and hidden state, and triangulates faces. `xtask/src/layers.rs` lists it as `Standalone`: layer 0 with no workspace dependencies. `crates/io/src/skp_in.rs` is its only user. See the root CLAUDE.md.

## Commands

```sh
cargo test -p sketcraft-skp                              # unit tests in src/lib.rs (desktop only)
cargo test -p sketcraft-skp imports_fixtures             # one test
SKP_FILES=/path/a.skp:/path/b.skp cargo test -p sketcraft-skp imports_local_files -- --ignored
```

- `imports_local_files` is `#[ignore]`. It reads each path in `SKP_FILES` (colon-separated) and checks that each file has at least one face that triangulates.
- Building needs a C++17 compiler. `build.rs` compiles the parser with `cc` on every desktop build.

## Layout

- `src/lib.rs`: `Model`, `Vertex`, `Edge`, `Face`, `Object`, `Tag`, `import_skp`, `MAX_SKP_BYTES` (1 GiB), the FFI call (`parse_skp`), and the unit tests.
- `src/skp_native.cpp`: the C ABI (`ketchup_skp_parse`, `ketchup_skp_free`) and the topology adapter over OpenSKP.
- `vendor/skp/native/`: the OpenSKP source subset and headers, pinned to one commit, plus `miniz/`. `vendor/skp/README.md` says what is pinned and what is not imported.
- `build.rs`: compiles `src/skp_native.cpp`, the OpenSKP files and miniz into two static libraries. It does nothing on wasm32.
- `tests/fixtures/`: `placed-window.skp`, `modern-triangle.skp`.

## Rules

- `unsafe_code = "forbid"` is a workspace lint, and crates opt in with `[lints] workspace = true`. This crate's `Cargo.toml` does not, so the forbid (and the workspace clippy lints) do not apply here. The crate root also lacks the `#![deny(clippy::unwrap_used, …)]` line other production crates carry. The FFI in `parse_skp` is the only `unsafe`. Keep it there, with a `// SAFETY:` note on each block.
- C++ exceptions must not cross the C ABI. `ketchup_skp_parse` is `noexcept` and catches everything, returning the message with `success = 0`.
- Memory from the parser is freed once with `ketchup_skp_free`. A null return means out of memory.
- A crash inside the C++ code (a segfault, an abort) is not caught by `catch_unwind` or `sketcraft_kernel::guard`. It takes the app down. Treat parser changes as crash-safety work: `skills/solidcraft-crash-safety/SKILL.md`.
- Input is hostile. Keep the size check before parsing, the count caps in `decode_model`, and the loop bound in `any_ancestor` (a group may contain itself).
- wasm32: `import_skp` returns an error ("only available in the desktop app"). Keep that stub so `cargo run -p xtask -- wasm` and the web build pass.
- Do not add workspace crate dependencies; the layer table marks it `Standalone`.
- Positions are metres, Z up. `crates/io/src/skp_in.rs` multiplies them by 1000 into millimetres.
- Vendored code: keep `vendor/skp/LICENSE-openskp` and `native/miniz/LICENSE` with it. Record any change to the pinned sources in `vendor/skp/README.md`. New assets need attribution (`skills/solidcraft-ground-rules/SKILL.md`).

## Gotchas

- The parser writes an empty map as `[]` and a full one as an object. `map_or_empty_array` handles both. It is a visitor, not `#[serde(untagged)]`, because buffering turned integer keys like `"6529"` into strings that no longer parsed.
- `decode_model` renames the last object (the highest id) to the file name. The reader puts the root group last.
- `face_triangles` returns nothing for a degenerate or non-planar face (`PLANE_TOL = 1e-6`).
- `crates/testkit` mutation tests (`crates/io/src/mutate.rs`) do not cover SKP yet.
- The crate is formatted at rustfmt's default width, unlike the wide lines in the rest of the workspace.
