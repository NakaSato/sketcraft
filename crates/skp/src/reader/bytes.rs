//! Bounds-checked little-endian reads (`read_u16`, `read_u32`, `read_f64`, `parse_varint` in
//! `tlv.cpp`). `None` is where the C++ throws `std::out_of_range`.

pub(super) fn slice(d: &[u8], at: usize, n: usize) -> Option<&[u8]> {
    d.get(at..at.checked_add(n)?)
}

fn array<const N: usize>(d: &[u8], at: usize) -> Option<[u8; N]> {
    slice(d, at, N)?.try_into().ok()
}

pub(super) fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    array(d, at).map(u16::from_le_bytes)
}

pub(super) fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    array(d, at).map(u32::from_le_bytes)
}

pub(super) fn u64_at(d: &[u8], at: usize) -> Option<u64> {
    array(d, at).map(u64::from_le_bytes)
}

pub(super) fn f64_at(d: &[u8], at: usize) -> Option<f64> {
    array(d, at).map(f64::from_le_bytes)
}

/// `parse_varint`: `n` little-endian bytes. Fails past the buffer or for more than 8 bytes.
pub(super) fn varint(d: &[u8], at: usize, n: usize) -> Option<u64> {
    let bytes = slice(d, at, n)?;
    if n > 8 {
        return None;
    }
    Some(bytes.iter().rev().fold(0, |v, &b| (v << 8) | u64::from(b)))
}

/// `parse_varint(p, 0, p.size())`.
pub(super) fn varint_all(d: &[u8]) -> Option<u64> {
    varint(d, 0, d.len())
}
