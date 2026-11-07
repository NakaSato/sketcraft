# CLAUDE.md — sketcraft-profile

The profiler: nested, timed spans per thread (kernel calls, features, commands, imports), reported
as an aggregate table and a Chrome trace. It is "Standalone" in `cargo xtask layers`: no workspace
dependencies, usable from every layer. See the root CLAUDE.md.

## Commands

```sh
cargo test -p sketcraft-profile                 # tests compile the profiler in (cfg test)
cargo test -p sketcraft-profile nothing_is_recorded_while_off
cargo check -p sketcraft-profile --features enabled
cargo check -p sketcraft-profile --target wasm32-unknown-unknown
```

## Layout

- `src/lib.rs`: the public API. `span`, `scope`, `record`, `intern`, `enabled`/`set_enabled`,
  `reset`, `report` (→ `Report` of `Row`s), `mark`, `write_chrome_trace`. Limits: `MAX_EVENTS`,
  `MAX_NAMES`, `OTHER`.
- `mod imp` inside `lib.rs`: the recording code, compiled only with the `enabled` feature (or in
  tests). Without it every call is an empty inline function.
- `src/tests.rs`: unit tests. The profiler is global, so tests take the `serial()` lock.

## Features

- `enabled`: compiles the profiler in. Engine, kernel and smlib turn it on through their own
  features; the apps enable it by default (`profile` feature). Even when compiled in, it records
  nothing until `set_enabled(true)` (`SKETCRAFT_PROFILE=1`, the CLI's `--profile`, the preference,
  the `diagnostics.profile` command).

## Rules

- Dependencies: `serde_json` and `web-time` only. No workspace crates.
- Use `web_time::Instant`, never `std::time::Instant`: std's panics on wasm32.
- Keep the off path cheap: one relaxed atomic load per span.
- The crate root denies `unwrap`, `expect`, `panic!` and friends.
  See `skills/solidcraft-crash-safety/SKILL.md` and `skills/solidcraft-performance/SKILL.md`.

## Gotchas

- `COMPILED` is true in tests even without the feature (`cfg!(any(feature = "enabled", test))`).
- A span's total counts only its outermost occurrence on a thread's stack (no double counting of
  nested commands or booleans).
- `cargo xtask wasm` only checks layered crates, not Standalone ones like this. It still builds for
  wasm as a dependency of `doc` and others.
