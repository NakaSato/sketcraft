# CLAUDE.md — sketcraft-smlib

A safe wrapper around solidcore, the Rust port of NVIDIA's SMLib B-rep kernel (pinned by git rev in the root `Cargo.toml`). It is layer L1 and the only crate that depends on `solidcore`. Only `crates/kernel` may use it (`INTRA_LAYER_ORDER` in `xtask/src/layers.rs`). Its other dependencies are `geom`, `profile` and `solidcore-record`. `publish = false`. See the root CLAUDE.md.

## Commands

```sh
cargo test -p sketcraft-smlib                            # unit tests in src/ and tests/ssi_corpus.rs
cargo test -p sketcraft-smlib --test ssi_corpus          # the SSI corpus in tests/ssi-corpus
cargo test -p sketcraft-smlib --lib core::boolean_tests  # one test module (ops_tests.rs is `ops::tests`)
SMLIB_OPS_SHOTS=/tmp/shots cargo test -p sketcraft-smlib --lib ops::tests   # also write each result as .smb
```

- The SSI parity test is `#[ignore]`. It needs `SSI_PARITY_RAW`, `SSI_PARITY_SCORES`, `SSI_PARITY_TOL` and `SSI_SM_MESSAGES`; the header of `tests/ssi_corpus.rs` shows the full command.
- The first solidcore build fetches a git dependency.

## Layout

- `src/lib.rs`: `Error` (every way a call can fail or be refused), the public re-exports, and the module list.
- `src/core.rs`, `src/core/`: the solidcore backend. One process-wide kernel, every call under its lock and a panic guard, opt-in upstream fixes. Submodules: boolean, blend, offset, sweep, split, merge, trim, ssi, measure, transform, convert, history, file, dump.
- `src/ops.rs`: booleans, fillet, chamfer, shell, offset faces, thicken. Edges and faces are picked by points, not by internal order.
- `src/sweeps.rs`: extrude (straight or drafted), revolve, sweep, loft, screw sweeps.
- `src/import.rs`, `src/export.rs`, `src/flat.rs`: schema 4 records (`solidcore_record::brep`) to and from SMLib's flat B-rep arrays. The exported arrays are read as hostile input.
- `src/history.rs`: face and edge history through operations (tagged operands).
- `src/sheets.rs`, `src/split.rs`, `src/trim.rs`, `src/ssi.rs`, `src/pcurves.rs`: sheet sewing and offset, split face and body, trims, surface/surface intersection, pcurves.
- `src/measure.rs`: exact mass properties, exact box, point classification. `src/check.rs`: SMLib's validity check.
- `src/stop.rs`: stopping work for a closed design (`Stop::within`). `src/trace.rs`: an operation trace for the kernel rig's corpus export.
- Tests: `*_tests.rs` beside each module, `src/tests.rs`, and `tests/ssi_corpus.rs` with its scorer in `tests/ssi_score/`.

## Rules

- No solidcore type crosses into `crates/kernel`'s public API. Map solidcore errors into `Error`.
- Every call must not change its inputs. It returns new bodies.
- Every call runs under the lock and panic guard in `src/core.rs`. A panic becomes `Error::Internal`.
- solidcore results are hostile too. `export.rs` checks every count and index it reads.
- Refuse an operation solidcore cannot build exactly (`Unsupported`, `NotExact`, `CornerUnsupported`, …). Do not build it another way.
- Tests compare against analytic volumes and topology counts, and run the validity check on every result.
- Crash-safety rules apply (the crate root denies `unwrap` and friends): `skills/solidcraft-crash-safety/SKILL.md`.
- A solidcore rev bump can change test results. Keep `tests/KNOWN_FAILING_TESTS` and `tests/kernel-cases/*/KNOWN_FAILING` in step.

## Gotchas

- SMLib keeps some state for the whole process (for example the fillet solver's latched step). That is why there is one kernel per process.
- Status codes in `Error::Status` are SMLib's (1001 `SM_ERR`, 1007 `SM_ERR_INVALID_INPUT`).
- One test in `src/tests.rs` is ignored because solidcore reads an ASCII file of junk as an empty B-rep.
- Comments still say `solidcraft-smlib` and `solidcraft-cli`. The packages are `sketcraft-smlib` and `sketcraft-cli`.
