//! The `solvecraft-cli` compatibility alias forwards to `sketcraft-cli`.
use std::process::Command;

#[test]
fn legacy_alias_forwards_with_a_deprecation_note() {
    let out = Command::new(env!("CARGO_BIN_EXE_solvecraft-cli")).arg("--version").output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), format!("sketcraft-cli {}", env!("CARGO_PKG_VERSION")));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("deprecated") && stderr.contains("sketcraft-cli"), "{stderr}");
}

#[test]
fn legacy_alias_passes_the_exit_status_through() {
    let alias = Command::new(env!("CARGO_BIN_EXE_solvecraft-cli")).arg("no-such-command").output().unwrap();
    let real = Command::new(env!("CARGO_BIN_EXE_sketcraft-cli")).arg("no-such-command").output().unwrap();
    assert_eq!(alias.status.code(), real.status.code());
    assert_eq!(alias.stdout, real.stdout);
}
