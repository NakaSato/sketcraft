//! Pure-Rust SketchUp reader: a port of the subset of OpenSKP (`vendor/skp/native`) that
//! `full_parse` + `build_model` reach, plus the topology adapter that used to live in
//! `src/skp_native.cpp`. Parity with the C++ output is the goal: the same entities, the same
//! IDs, the same errors. Comments name the C++ function each piece mirrors.
//!
//! Input is hostile. Every read is bounds checked, every count read from the file is capped
//! by the bytes left to read, and recursion is bounded ([`MAX_TLV_DEPTH`],
//! `legacy::MAX_OBJECT_DEPTH`, the adapter's depth of 64).

mod bytes;
mod import;
mod legacy;
mod vff;
mod xml;
mod zip;

use std::collections::{BTreeMap, BTreeSet};

pub(crate) use import::import;

/// Nesting limit for VFF records. Real files nest about ten deep; the C++ reader recursed
/// without a limit and would overflow the stack instead.
const MAX_TLV_DEPTH: usize = 256;

/// The message for every allocation that a file's sizes could make fail.
const OUT_OF_MEMORY: &str = "Not enough memory to read this SketchUp file";

/// `CoEdge`: one oriented edge of a face loop.
#[derive(Clone, Copy, Debug)]
struct CoEdge {
    edge: i64,
    orientation: i64,
}

/// `RawFace`, reduced to what the import uses.
#[derive(Debug, Default)]
struct RawFace {
    loops: Vec<Vec<CoEdge>>,
    material: Option<i64>,
    hidden: bool,
    /// The layer ID as written (`RawFace::layer` before `build_model` resolves it).
    layer: Option<u64>,
}

/// `RawInstance`, reduced to what the import uses.
#[derive(Debug, Default)]
struct RawInstance {
    name: Vec<u8>,
    ref_idx: Option<i64>,
    material: Option<i64>,
    /// 12, 13 or 16 numbers, or empty for no transform.
    matrix: Vec<f64>,
    layer: Option<u64>,
    hidden: bool,
}

/// `GeometryBuilder`, reduced to what the import uses. `edge_flags` is separate from `edges`
/// because the C++ keeps them in two maps: a later record with the same ID replaces the end
/// points but keeps the flags unless it has flags of its own.
#[derive(Debug, Default)]
struct Builder {
    vertices: BTreeMap<i64, [f64; 3]>,
    edges: BTreeMap<i64, (Option<i64>, Option<i64>)>,
    edge_flags: BTreeMap<i64, i32>,
    faces: BTreeMap<i64, RawFace>,
    instances: Vec<RawInstance>,
}

/// `RawDefinition`: a component definition, or the model root.
#[derive(Debug, Default)]
struct Definition {
    name: Vec<u8>,
    builder: Builder,
}

/// `RawParsed`, reduced to what `build_model` and the adapter use. Names are bytes: the file
/// does not promise UTF-8, and the C++ compared them as bytes.
#[derive(Debug, Default)]
struct Parsed {
    definitions: BTreeMap<i64, Definition>,
    root: Definition,
    /// Layer names in first-seen order (only these become `SkpModel::layers`).
    layer_order: Vec<Vec<u8>>,
    /// The names in `layer_order` (`layer_colors`' keys in the C++).
    layer_known: BTreeSet<Vec<u8>>,
    layer_hidden: BTreeMap<Vec<u8>, bool>,
    layer_id_to_name: BTreeMap<i64, Vec<u8>>,
    material_id_to_name: BTreeMap<i64, Vec<u8>>,
    /// Every material read, as `[r, g, b]` before the `uint8_t` conversion.
    materials: Vec<[i32; 3]>,
    materials_by_name: BTreeMap<Vec<u8>, usize>,
    materials_by_folder: BTreeMap<Vec<u8>, usize>,
}

impl Parsed {
    /// Adds `name` to the layer list the way both readers do: the first time a name is seen
    /// it keeps its place in `layer_order`.
    fn add_layer(&mut self, name: &[u8], hidden: bool) {
        if self.layer_known.insert(name.to_vec()) {
            self.layer_order.push(name.to_vec());
        }
        self.layer_hidden.insert(name.to_vec(), hidden);
    }

    /// The Layer0 defaults `full_parse` and `parse_legacy` add after reading.
    fn add_default_layer0(&mut self) {
        if self.layer_known.insert(b"Layer0".to_vec()) {
            self.layer_order.push(b"Layer0".to_vec());
        }
        self.layer_hidden.entry(b"Layer0".to_vec()).or_insert(false);
    }
}

/// Reads a SketchUp file into the parsed definitions (`full_parse`).
fn parse(data: &[u8]) -> Result<Parsed, String> {
    if !valid_header(data) {
        return Err("Not a valid SketchUp file (bad header magic) | stage=header".into());
    }
    if is_legacy(data) {
        return legacy::parse(data).map_err(|_| "legacy .skp parse failed | stage=legacy_walk".to_owned());
    }
    vff::parse(data)
}

/// `valid_header`.
fn valid_header(d: &[u8]) -> bool {
    d.starts_with(&[0xff, 0xfe, 0xff, 0x0e])
}

/// `extract_version`: the `{x.y.z}` string after the second header marker.
fn extract_version(d: &[u8]) -> Option<Vec<u8>> {
    let marker = (4..d.len().min(512)).find(|&i| d.get(i..i + 3) == Some(&[0xff, 0xfe, 0xff][..]))?;
    let mut s = Vec::new();
    let mut i = marker + 4;
    while i + 1 < d.len() && i < 512 {
        match d.get(i) {
            Some(&c) if c != 0 => s.push(c),
            _ => {}
        }
        i += 2;
    }
    let a = s.iter().position(|&c| c == b'{')?;
    let b = a + s.get(a..)?.iter().position(|&c| c == b'}')?;
    s.get(a..=b).map(<[u8]>::to_vec)
}

/// `is_legacy`: SketchUp 2020 and older use the MFC archive format.
fn is_legacy(d: &[u8]) -> bool {
    let Some(v) = extract_version(d) else {
        return false;
    };
    let Some(dot) = v.iter().position(|&c| c == b'.') else {
        return false;
    };
    if v.len() < 2 {
        return false;
    }
    v.get(1..dot).and_then(stoi).is_some_and(|major| major <= 20)
}

/// `std::stoi`: optional leading white space and sign, then decimal digits; anything after
/// the digits is ignored. `None` where `stoi` throws (no digits, or out of `int` range).
fn stoi(s: &[u8]) -> Option<i32> {
    let mut i = s.iter().take_while(|c| is_space(**c)).count();
    let negative = match s.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let digits = s.get(i..)?.iter().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let mut value: i64 = 0;
    for &c in s.get(i..i + digits)? {
        // strtol saturates and reports ERANGE, so any overflow is an error.
        value = value.checked_mul(10)?.checked_add(i64::from(c - b'0'))?;
    }
    i32::try_from(if negative { -value } else { value }).ok()
}

/// `isspace` in the C locale.
fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}
