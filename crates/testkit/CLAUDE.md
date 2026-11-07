# CLAUDE.md — sketcraft-testkit

Test tooling: deterministic mutations of valid files, and the no-panic check run over them. It has no dependencies. `xtask/src/layers.rs` classes it as `Testkit`: it may depend on anything up to L5, and other crates may use it only as a dev-dependency. See the root CLAUDE.md.

## Commands

```sh
cargo test -p sketcraft-testkit                          # its own unit test
cargo test -p sketcraft-io mutate::                      # its main user: crates/io/src/mutate.rs
```

## Layout

- `src/lib.rs` is the whole crate:
  - `mutations(good, text, seed)`: truncations, flipped, zeroed and saturated bytes, dropped, repeated and swapped runs, appended garbage. With `text`, also dropped and repeated lines and numbers replaced by liars (`nan`, `inf`, `1e400`, …). The same input and seed give the same cases.
  - `survives(what, inputs, cap, read)`: runs `read` on each input and fails the test on a panic (even one the reader caught), on a case slower than `BUDGET` (2 s), or on a count above `cap`.
  - A private xorshift64 `Rng`, so it needs no fuzzing crate and builds for wasm.

## Rules

- Add it to `[dev-dependencies]` only. `cargo run -p xtask -- layers` fails otherwise.
- Keep it dependency-free, deterministic and stable-Rust only. A failing case must be reproducible from its seed.
- Changing `mutations` changes every user's cases. Keep it deterministic and bounded (`MAX_TRUNCATIONS` keeps large files at about 1000 truncations).
- A new reader of untrusted bytes in any crate should get a `survives(mutations(...))` test. See `skills/sketcraft-crash-safety/SKILL.md`.
- `survives` uses `panic!` on purpose (it carries `#[allow(clippy::panic)]`). Elsewhere the crate root denies `unwrap`, `expect` and `panic!`.

## Gotchas

- A panic the reader catches itself (for example inside `sketcraft_kernel::guard`) still fails `survives`. The browser build aborts on any panic, so a caught panic on desktop is a crash on the web.
- STEP, IGES and FreeCAD/Rhino readers live in the kernel and have their own tests. SKP (`crates/skp`) has no mutation test yet.
