# CLAUDE.md — sketcraft-skp

The SketchUp (`.skp`) reader. `import_skp` reads a file into a `Model` of vertices, edges, planar faces, groups, colours, tags and hidden state, and the model triangulates faces. The reader is pure Rust (`src/reader/`), a port of the vendored OpenSKP C++ reader, and works on every target, the browser included. `xtask/src/layers.rs` lists the crate as `Standalone`: layer 0 with no workspace dependencies. `crates/io/src/skp_in.rs` is its only user. See the root CLAUDE.md.

## Commands

```sh
cargo test -p sketcraft-skp                              # unit tests and the C++ parity tests (desktop)
cargo test -p sketcraft-skp imports_fixtures             # one test
cargo test --release -p sketcraft-skp large_example_matches_cpp -- --ignored --nocapture   # the 72 MB example, with timings
SKP_FILES=/path/a.skp:/path/b.skp cargo test -p sketcraft-skp local_files_match_cpp -- --ignored
SKP_FILES=/path/a.skp:/path/b.skp cargo test -p sketcraft-skp imports_local_files -- --ignored
cargo check -p sketcraft-skp --target wasm32-unknown-unknown
```

- `imports_local_files` checks that each file in `SKP_FILES` (colon-separated) has a face that triangulates; `local_files_match_cpp` checks that both readers give the same result.
- `large_example_matches_cpp` reads `examples/Ban Bueng - EV Roofs Joined.skp`. Debug builds are slow on it; use `--release`. `large_example_one_reader` runs one reader (`SKP_READER=rust|cpp`) for measuring peak memory with `/usr/bin/time -l`.
- Desktop builds still need a C++17 compiler: `build.rs` compiles the C++ reader as the test oracle.

## Layout

- `src/lib.rs`: `Model`, `Vertex`, `Edge`, `Face`, `Object`, `Tag`, `import_skp`, `MAX_SKP_BYTES` (1 GiB), and the unit tests (including truncated, random and flipped input).
- `src/reader/`: the reader. `mod.rs` holds the parsed structures and the header and version checks; `vff.rs` reads 2021+ files (a ZIP whose `model.dat` is a tag-length-value tree); `legacy.rs` reads SketchUp 3 to 2020 files (an MFC object archive); `zip.rs` ports the miniz calls the C++ made, with `miniz_oxide` inflating; `xml.rs` replaces the `std::regex` searches over `material.xml`; `import.rs` is the topology adapter that was in `skp_native.cpp`; `bytes.rs` has the bounds-checked reads.
- `src/parity.rs` (desktop tests only): the FFI call into the C++ reader (`parse_skp`), the old JSON decode (`decode_model`), and the tests that compare both readers: the fixtures, every prefix and every single-byte change of the fixtures, the large example and `SKP_FILES`.
- `src/skp_native.cpp`, `vendor/skp/native/`, `build.rs`: the C++ reader and its build, kept only as the oracle for `src/parity.rs`. Nothing outside the tests calls it. `build.rs` does nothing on wasm32.
- `tests/fixtures/`: `placed-window.skp` (SketchUp 2017, legacy format), `modern-triangle.skp` (2026, VFF).

## Rules

- Parity with the C++ output is the contract while the oracle exists: same entities, same IDs, same error messages. Run the parity tests after any reader change. Comments in `src/reader/` name the C++ function each part mirrors; where the C++ behaviour looks odd, it is kept on purpose and the comment says so.
- Input is hostile (`skills/sketcraft-crash-safety/SKILL.md`). The reader must not panic on any input: bounds-checked reads (`bytes.rs`), counts capped by the bytes left, no allocation sized by an untrusted number except through `try_reserve`, and bounded recursion (`MAX_TLV_DEPTH`, `legacy::MAX_OBJECT_DEPTH`, the adapter's 64 levels and cycle check). Keep the size check in `import_skp` and the loop bound in `any_ancestor` (a group may contain itself).
- The reader holds the input, the inflated `model.dat` (borrowed when stored) and the parsed definitions; VFF records are read in place, not copied. Keep it that way: the browser build has a 32-bit address space.
- The crate root denies `unsafe_code` and the panicking clippy lints. The only `unsafe` is the FFI in `src/parity.rs`, with a `// SAFETY:` note on each block. The crate does not opt in to the workspace lints because their `unsafe_code = "forbid"` cannot be relaxed for that module.
- Do not add workspace crate dependencies; the layer table marks it `Standalone`. External dependencies must build for `wasm32-unknown-unknown`.
- Positions are metres, Z up. `crates/io/src/skp_in.rs` multiplies them by 1000 into millimetres.
- Vendored code: keep `vendor/skp/LICENSE-openskp` and `native/miniz/LICENSE` with it while it is vendored; the Rust port is derived from OpenSKP (MIT), so keep its licence notice when the C++ goes. Record any change to the pinned sources in `vendor/skp/README.md`.

## Gotchas

- Names in the parsed structures are bytes, not strings: the file does not promise UTF-8 and the C++ compared bytes. They become `String` (lossy) only in `Object::name`, which is what the old JSON path produced.
- The import renames the root group (the highest ID, allocated last) to the file name, as `decode_model` did.
- The ZIP lookup is case-insensitive through miniz's sorted index, and the entry size limits (16 GiB, compression ratio 1000) apply to every entry the C++ opened, including textures and styles the import does not use (`Zip::probe`).
- Legacy errors all read "legacy .skp parse failed"; the C++ discarded the detail too.
- The parity tests compare positions with a 1e-12 relative tolerance and everything else exactly: Apple clang contracts `a * b + c` into fused multiply-adds by default, Rust does not, so a placement transform can round differently in the last bit. Building the oracle with `CXXFLAGS=-ffp-contract=off` makes them match bit for bit.
- `large_example_matches_cpp` takes about 15 minutes and many GB: the C++ path builds a JSON document of the whole expanded model (26 M vertices). The Rust reader alone takes about 7 s in release.
- The adapter collects its output as ID-ordered lists and builds each map in one go (`Output::into_model`); inserting one by one leaves B-tree nodes half full. A large expanded model is still big (about 4 GB for the example above): every component placement is copied, as the `Model` API requires.
- `face_triangles` returns nothing for a degenerate or non-planar face (`PLANE_TOL = 1e-6`).
- `map_or_empty_array` lets `Model` deserialize the old JSON, where an empty map was written as `[]`. It is a visitor, not `#[serde(untagged)]`, because buffering turned integer keys like `"6529"` into strings that no longer parsed.
- `crates/testkit` mutation tests (`crates/io/src/mutate.rs`) do not cover SKP yet.
- The crate is formatted with the workspace `rustfmt.toml` (150 columns), like the rest of the workspace.
