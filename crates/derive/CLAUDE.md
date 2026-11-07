# CLAUDE.md — sketcraft-derive

A proc-macro crate with one derive, `#[derive(Walk)]`. It generates the walks over a feature's
fields (expressions, references, geometry) from `#[walk(...)]` role attributes. It is "Standalone"
in `cargo xtask layers`: layer 0 with no workspace dependencies. See the root CLAUDE.md.

## Commands

```sh
cargo check -p sketcraft-derive
cargo test -p sketcraft-doc feature_walk     # the real tests: golden records of every walk
FEATURE_WALK_BLESS=1 cargo test -p sketcraft-doc feature_walk   # re-record after an intended change
```

This crate has no tests of its own. Its output is tested through `sketcraft-doc`
(`crates/doc/src/feature_walk_tests.rs`, goldens in `crates/doc/src/feature_walk_tests/*.txt`).

## Layout

- `src/lib.rs`: the whole crate. `derive_walk` is the entry point. `roles()` parses `#[walk(...)]`
  on a field. The expansion functions emit `walk_exprs`, `walk_refs` and `walk_geometry`, each in a
  `&` and a `&mut` form.
- The module doc at the top of `src/lib.rs` is the reference for the attributes: `length`/`angle`/
  `unitless = label`, `pinned`, `pick_if`, `placement`, `profiles_of` (+ `when`), `after`,
  `picks_faces`.

## Rules

- Dependencies: `proc-macro2`, `quote`, `syn` only. No workspace crates.
- The generated code names `crate::walk`. So the derive only works inside `sketcraft-doc`
  (`crates/doc/src/walk.rs` holds the traits it calls).
- Report bad attributes with `syn::Error` (`to_compile_error`), never panic. The crate root denies
  `unwrap`, `expect` and `panic!`.
- A change to what a walk visits changes the goldens. Re-record them with `FEATURE_WALK_BLESS=1`
  and say why in the change.

## Gotchas

- Fields are visited in declaration order. Order matters: the order of expression inputs gives
  the `dN` parameter names. Use `after = field` instead of reordering struct fields.
- An `Expr` or bare `Vec3` field without a role does not compile on purpose. Give it a role.
- Doc comments still say `solidcraft-doc`; the package is `sketcraft-doc`.
