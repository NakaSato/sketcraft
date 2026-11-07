//! Differential tests: the Rust reader against the vendored C++ reader it replaces, which
//! `build.rs` still compiles on desktop. The C++ result travels as JSON (`%.17g` numbers, read
//! back with serde_json's `float_roundtrip`), so it compares exactly.
//!
//! The C++ code is reached only from here; once parity is settled it can be removed together
//! with `build.rs`, `src/skp_native.cpp` and `vendor/skp/native`.

#![allow(unsafe_code, clippy::panic, clippy::unwrap_used, clippy::expect_used)]

use std::time::Instant;

use crate::{MAX_SKP_BYTES, Model, import_skp};

fn parse_skp(bytes: &[u8]) -> Result<String, String> {
    use std::ffi::{CStr, c_char};
    unsafe extern "C" {
        fn ketchup_skp_parse(bytes: *const u8, len: usize, success: *mut i32) -> *mut c_char;
        fn ketchup_skp_free(value: *mut c_char);
    }
    let mut success = 0;
    // SAFETY: `bytes` is valid for `bytes.len()` reads. The reader returns a newly
    // allocated, null-terminated string, or null when out of memory.
    let value = unsafe { ketchup_skp_parse(bytes.as_ptr(), bytes.len(), &mut success) };
    if value.is_null() {
        return Err("Not enough memory to read this SketchUp file".into());
    }
    // SAFETY: `value` is non-null and null-terminated.
    let text = unsafe { CStr::from_ptr(value) }.to_string_lossy().into_owned();
    // SAFETY: `value` came from `ketchup_skp_parse` and is freed once.
    unsafe { ketchup_skp_free(value) };
    if success == 1 { Ok(text) } else { Err(text) }
}

fn decode_model(json: &str, name: &str) -> Result<Model, String> {
    let mut model: Model = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if model.vertices.len() > 100_000_000 || model.faces.len() > 100_000_000 || model.objects.len() > 100_000_000 {
        return Err("Model is too large".into());
    }
    // The reader puts the file's root group last; name it after the file.
    if let Some(root) = model.objects.values_mut().next_back() {
        root.name = name.to_owned();
    }
    Ok(model)
}

/// `import_skp` as it was: the C++ reader, then the JSON decode.
fn cpp_import(bytes: &[u8], name: &str) -> Result<Model, String> {
    if bytes.len() > MAX_SKP_BYTES {
        return Err("SketchUp import is limited to 1 GiB per file".into());
    }
    let json = parse_skp(bytes).map_err(|e| format!("Could not read SketchUp file: {e}"))?;
    decode_model(&json, name)
}

/// Relative tolerance for positions. Apple clang contracts `a * b + c` into fused
/// multiply-adds by default (`-ffp-contract=on`) and Rust never does, so a placement transform
/// can round differently in the last bit or two. Everything else must match exactly.
const POSITION_TOLERANCE: f64 = 1e-12;

/// Panics with the first difference between the two readers' results.
fn assert_same(label: &str, rust: &Result<Model, String>, cpp: &Result<Model, String>) {
    match (rust, cpp) {
        (Ok(a), Ok(b)) => {
            if a == b {
                return;
            }
            assert_eq!(a.vertices.len(), b.vertices.len(), "{label}: vertex count");
            let (mut inexact, mut worst) = (0usize, 0f64);
            for (va, vb) in a.vertices.values().zip(b.vertices.values()) {
                assert_eq!(va.id, vb.id, "{label}: vertex IDs");
                if va.position == vb.position {
                    continue;
                }
                inexact += 1;
                for (x, y) in va.position.to_array().into_iter().zip(vb.position.to_array()) {
                    let error = (x - y).abs() / y.abs().max(1.0);
                    worst = worst.max(error);
                    assert!(error <= POSITION_TOLERANCE, "{label}: vertex {} at {} vs {}", va.id, va.position, vb.position);
                }
            }
            println!("{label}: {inexact} of {} positions differ in rounding (worst relative {worst:e})", a.vertices.len());
            assert_eq!(a.edges, b.edges, "{label}: edges");
            assert_eq!(a.faces, b.faces, "{label}: faces");
            assert_eq!(a.face_colors, b.face_colors, "{label}: face colors");
            assert_eq!(a.objects, b.objects, "{label}: objects");
            assert_eq!(a.hidden, b.hidden, "{label}: hidden");
            assert_eq!((&a.tags, &a.entity_tags), (&b.tags, &b.entity_tags), "{label}: tags");
        }
        (Err(a), Err(b)) => assert_eq!(a, b, "{label}: error"),
        (a, b) => panic!("{label}: Rust {:?} but C++ {:?}", a.as_ref().map(|_| "Ok"), b.as_ref().map(|_| "Ok")),
    }
}

/// Compares both readers on `bytes`; true when they read a model.
fn compare(label: &str, bytes: &[u8]) -> bool {
    let rust = import_skp(bytes, "File");
    assert_same(label, &rust, &cpp_import(bytes, "File"));
    rust.is_ok()
}

const WINDOW: &[u8] = include_bytes!("../tests/fixtures/placed-window.skp");
const TRIANGLE: &[u8] = include_bytes!("../tests/fixtures/modern-triangle.skp");

#[test]
fn fixtures_match_cpp() {
    assert!(compare("placed-window", WINDOW));
    assert!(compare("modern-triangle", TRIANGLE));
}

/// Every prefix of both fixtures gives the same result (model or error message) as the C++.
#[test]
fn truncated_fixtures_match_cpp() {
    for (name, fixture) in [("placed-window", WINDOW), ("modern-triangle", TRIANGLE)] {
        let read = (0..fixture.len()).filter(|&len| compare(&format!("{name}[..{len}]"), &fixture[..len])).count();
        println!("{name}: {read} of {} prefixes read", fixture.len());
    }
}

/// Every single-byte change to both fixtures gives the same result as the C++.
#[test]
fn flipped_fixtures_match_cpp() {
    for (name, fixture) in [("placed-window", WINDOW), ("modern-triangle", TRIANGLE)] {
        let mut read = 0;
        for at in 0..fixture.len() {
            for mask in [0x01, 0x10, 0x80, 0xff] {
                let mut bytes = fixture.to_vec();
                bytes[at] ^= mask;
                read += usize::from(compare(&format!("{name} byte {at} ^ {mask:#x}"), &bytes));
            }
        }
        println!("{name}: {read} of {} changed files read", fixture.len() * 4);
    }
}

fn large_example() -> Vec<u8> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/Ban Bueng - EV Roofs Joined.skp");
    std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The 72 MB example. Run in release mode for meaningful timings:
/// cargo test --release -p sketcraft-skp large_example_matches_cpp -- --ignored --nocapture
#[test]
#[ignore]
fn large_example_matches_cpp() {
    let bytes = large_example();
    let start = Instant::now();
    let rust = import_skp(&bytes, "File");
    let rust_time = start.elapsed();
    let start = Instant::now();
    let cpp = cpp_import(&bytes, "File");
    let cpp_time = start.elapsed();
    if let Ok(model) = &rust {
        println!(
            "{} vertices, {} edges, {} faces, {} colours, {} groups, {} hidden",
            model.vertices.len(),
            model.edges.len(),
            model.faces.len(),
            model.face_colors.len(),
            model.objects.len(),
            model.hidden.len()
        );
    }
    println!("Rust {rust_time:?}, C++ {cpp_time:?}, identical: {}", rust == cpp);
    assert_same("large example", &rust, &cpp);
}

/// One reader on the large example, for measuring peak memory of each separately:
/// SKP_READER=rust|cpp /usr/bin/time -l <test binary> large_example_one_reader --ignored
#[test]
#[ignore]
fn large_example_one_reader() {
    let bytes = large_example();
    let start = Instant::now();
    let model = match std::env::var("SKP_READER").as_deref() {
        Ok("cpp") => cpp_import(&bytes, "File"),
        _ => import_skp(&bytes, "File"),
    }
    .unwrap();
    println!(
        "{} vertices, {} edges, {} faces ({} hole rings, {} ring vertices), {} colours, {} groups, {} hidden in {:?}",
        model.vertices.len(),
        model.edges.len(),
        model.faces.len(),
        model.faces.values().map(|f| f.holes.len()).sum::<usize>(),
        model.faces.values().map(|f| f.vertices.len() + f.holes.iter().map(Vec::len).sum::<usize>()).sum::<usize>(),
        model.face_colors.len(),
        model.objects.len(),
        model.hidden.len(),
        start.elapsed()
    );
}

/// Optional real files: SKP_FILES=a.skp:b.skp cargo test -p sketcraft-skp local_files_match_cpp -- --ignored
#[test]
#[ignore]
fn local_files_match_cpp() {
    for path in std::env::var("SKP_FILES").unwrap_or_default().split(':').filter(|p| !p.is_empty()) {
        assert!(compare(path, &std::fs::read(path).unwrap()), "{path}");
    }
}
