//! The ZIP container of a 2021+ file: a port of the miniz reader calls `core.cpp` makes
//! (`mz_zip_reader_init_mem`, `mz_zip_reader_locate_file`, `mz_zip_reader_file_stat`,
//! `mz_zip_reader_extract_to_mem`) and of `Zip::validate_entry_size`. It keeps miniz's
//! behaviour where a file could tell the difference: case-insensitive lookups through the same
//! sorted index, the same header checks and the same CRC check. Inflating is `miniz_oxide`.

use std::borrow::Cow;

use miniz_oxide::inflate::stream::{InflateState, inflate};
use miniz_oxide::{DataFormat, MZFlush, MZStatus};

use super::OUT_OF_MEMORY;
use super::bytes::{slice, u16_at, u32_at, u64_at};

const EOCD_SIG: u32 = 0x0605_4b50;
const CDH_SIG: u32 = 0x0201_4b50;
const LOCAL_SIG: u32 = 0x0403_4b50;
const EOCD64_SIG: u32 = 0x0606_4b50;
const EOCD64_LOCATOR_SIG: u32 = 0x0706_4b50;
const EOCD_SIZE: usize = 22;
const CDH_SIZE: usize = 46;
const LOCAL_SIZE: usize = 30;
const EOCD64_SIZE: usize = 56;
const EOCD64_LOCATOR_SIZE: usize = 20;
const MAX_FILENAME: usize = 512;

/// `Zip::kMaxUncompressedEntryBytes` and friends in `core.cpp`.
const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_RATIO: u64 = 1000;
const RATIO_CHECK_BYTES: u64 = 1024 * 1024;

/// `mz_zip_archive_file_stat`, the fields used.
pub(super) struct Stat {
    name: Vec<u8>,
    bit_flag: u16,
    method: u16,
    crc32: u32,
    comp: u64,
    uncomp: u64,
    local: u64,
    is_dir: bool,
}

pub(super) struct Zip<'a> {
    data: &'a [u8],
    cdir: &'a [u8],
    /// Offset of each central directory record in `cdir`.
    offsets: Vec<usize>,
    /// File indices sorted by lower-cased name, as miniz sorts them.
    sorted: Vec<usize>,
    names: Vec<Vec<u8>>,
}

impl<'a> Zip<'a> {
    /// `mz_zip_reader_init_mem` plus the file name list `Zip::Zip` builds. `None` is
    /// "Invalid ZIP container".
    pub(super) fn open(data: &'a [u8]) -> Option<Self> {
        let mut zip = read_central_dir(data)?;
        for i in 0..zip.offsets.len() {
            if let Some(stat) = zip.stat(i)
                && !stat.is_dir
            {
                zip.names.push(stat.name);
            }
        }
        Some(zip)
    }

    /// Every file (not directory) name, in central directory order.
    pub(super) fn names(&self) -> &[Vec<u8>] {
        &self.names
    }

    fn header(&self, i: usize) -> Option<&'a [u8]> {
        self.cdir.get(*self.offsets.get(i)?..)
    }

    /// The full name stored in record `i` (not truncated like `Stat::name`).
    fn raw_name(&self, i: usize) -> &'a [u8] {
        self.header(i).and_then(|h| slice(h, CDH_SIZE, usize::from(u16_at(h, 28)?))).unwrap_or(&[])
    }

    /// `mz_zip_reader_file_stat`.
    fn stat(&self, i: usize) -> Option<Stat> {
        let p = self.header(i)?;
        let mut stat = Stat {
            name: Vec::new(),
            bit_flag: u16_at(p, 8)?,
            method: u16_at(p, 10)?,
            crc32: u32_at(p, 16)?,
            comp: u64::from(u32_at(p, 20)?),
            uncomp: u64::from(u32_at(p, 24)?),
            local: u64::from(u32_at(p, 42)?),
            is_dir: false,
        };
        let name_len = usize::from(u16_at(p, 28)?);
        let name = slice(p, CDH_SIZE, name_len.min(MAX_FILENAME - 1))?;
        // miniz copies the name into a C string.
        stat.name = name.iter().copied().take_while(|&c| c != 0).collect();
        stat.is_dir = (name_len > 0 && p.get(CDH_SIZE + name_len - 1) == Some(&b'/')) || u32_at(p, 38)? & 0x10 != 0;
        if stat.comp.max(stat.uncomp).max(stat.local) == u64::from(u32::MAX) {
            let mut remaining = usize::from(u16_at(p, 30)?);
            let mut at = CDH_SIZE + name_len;
            while remaining > 0 {
                if remaining < 4 {
                    return None;
                }
                let id = u16_at(p, at)?;
                let size = usize::from(u16_at(p, at + 2)?);
                if size + 4 > remaining {
                    return None;
                }
                if id == 1 {
                    let mut field = at + 4;
                    let mut left = size;
                    for value in [&mut stat.uncomp, &mut stat.comp, &mut stat.local] {
                        if *value == u64::from(u32::MAX) {
                            if left < 8 {
                                return None;
                            }
                            *value = u64_at(p, field)?;
                            field += 8;
                            left -= 8;
                        }
                    }
                    break;
                }
                at += 4 + size;
                remaining -= 4 + size;
            }
        }
        Some(stat)
    }

    /// `mz_zip_reader_locate_file` without flags: a binary search over the names sorted
    /// case-insensitively.
    fn locate(&self, name: &[u8]) -> Option<usize> {
        let (mut low, mut high) = (0i64, self.sorted.len() as i64 - 1);
        while low <= high {
            let mid = low + ((high - low) >> 1);
            let index = *self.sorted.get(usize::try_from(mid).ok()?)?;
            let candidate = self.raw_name(index);
            let shared = candidate.len().min(name.len());
            let mismatch = (0..shared).find(|&k| lower(candidate[k]) != lower(name[k]));
            let order = match mismatch {
                Some(k) => i64::from(lower(candidate[k])) - i64::from(lower(name[k])),
                None => candidate.len() as i64 - name.len() as i64,
            };
            match order {
                0 => return Some(index),
                o if o < 0 => low = mid + 1,
                _ => high = mid - 1,
            }
        }
        None
    }

    fn find(&self, name: &[u8]) -> Result<Option<(usize, Stat)>, String> {
        let Some(index) = self.locate(name) else {
            return Ok(None);
        };
        let Some(stat) = self.stat(index) else {
            return Ok(None);
        };
        validate(&stat)?;
        Ok(Some((index, stat)))
    }

    /// `Zip::get`: the entry's bytes, `None` when it is missing or does not extract, an error
    /// when its declared size is refused.
    pub(super) fn get(&self, name: &[u8]) -> Result<Option<Cow<'a, [u8]>>, String> {
        let Some((_, stat)) = self.find(name)? else {
            return Ok(None);
        };
        self.extract(&stat)
    }

    /// `Zip::get` where only success matters (texture and style data the import never uses):
    /// the same lookup and size checks, without inflating. A damaged deflate stream or CRC
    /// therefore counts as success here where miniz would fail; that only changes which later
    /// candidate `Zip::get` would also check.
    pub(super) fn probe(&self, name: &[u8]) -> Result<bool, String> {
        let Some((_, stat)) = self.find(name)? else {
            return Ok(false);
        };
        Ok(self.data_start(&stat).is_some_and(|start| stat.is_dir || stat.comp == 0 || stat.method != 0 || fits(self.data, start, stat.uncomp)))
    }

    /// Checks shared by every extraction; the offset of the entry's data on success.
    fn data_start(&self, stat: &Stat) -> Option<u64> {
        if stat.is_dir || stat.comp == 0 {
            return Some(0);
        }
        if stat.bit_flag & (1 | 32 | 64) != 0 || (stat.method != 0 && stat.method != 8) {
            return None;
        }
        let local = slice(self.data, usize::try_from(stat.local).ok()?, LOCAL_SIZE)?;
        if u32_at(local, 0)? != LOCAL_SIG {
            return None;
        }
        let start = stat.local + LOCAL_SIZE as u64 + u64::from(u16_at(local, 26)?) + u64::from(u16_at(local, 28)?);
        fits(self.data, start, stat.comp).then_some(start)
    }

    /// `mz_zip_reader_extract_to_mem` into a buffer of the declared size.
    fn extract(&self, stat: &Stat) -> Result<Option<Cow<'a, [u8]>>, String> {
        let Some(start) = self.data_start(stat) else {
            return Ok(None);
        };
        if stat.is_dir || stat.comp == 0 {
            // miniz leaves the zero-filled buffer as it is.
            let size = usize::try_from(stat.uncomp).map_err(|_| OUT_OF_MEMORY.to_owned())?;
            let mut zeros = Vec::new();
            zeros.try_reserve_exact(size).map_err(|_| OUT_OF_MEMORY.to_owned())?;
            zeros.resize(size, 0);
            return Ok(Some(Cow::Owned(zeros)));
        }
        let start = usize::try_from(start).map_err(|_| OUT_OF_MEMORY.to_owned())?;
        let bytes: Cow<'a, [u8]> = if stat.method == 0 {
            // Stored: miniz copies the declared uncompressed size.
            let Some(bytes) = usize::try_from(stat.uncomp).ok().and_then(|n| slice(self.data, start, n)) else {
                return Ok(None);
            };
            Cow::Borrowed(bytes)
        } else {
            let Some(input) = usize::try_from(stat.comp).ok().and_then(|n| slice(self.data, start, n)) else {
                return Ok(None);
            };
            match inflate_exact(input, stat.uncomp)? {
                Some(bytes) => Cow::Owned(bytes),
                None => return Ok(None),
            }
        };
        Ok((crc32fast::hash(&bytes) == stat.crc32).then_some(bytes))
    }
}

/// Whether `n` bytes from `start` lie inside `data`.
fn fits(data: &[u8], start: u64, n: u64) -> bool {
    start.checked_add(n).is_some_and(|end| end <= data.len() as u64)
}

/// `MZ_TOLOWER`.
fn lower(c: u8) -> u8 {
    c.to_ascii_lowercase()
}

/// `Zip::validate_entry_size`: refuse declared sizes no real file needs.
fn validate(stat: &Stat) -> Result<(), String> {
    let declared = stat.uncomp;
    if declared == 0 {
        return Ok(());
    }
    let name = String::from_utf8_lossy(&stat.name);
    if usize::try_from(declared).is_err() {
        return Err(format!("ZIP entry '{name}' declares {declared} bytes, exceeding addressable memory limits | stage=zip_extract"));
    }
    if declared > MAX_ENTRY_BYTES {
        return Err(format!(
            "ZIP entry '{name}' declares {declared} bytes uncompressed, exceeding the {MAX_ENTRY_BYTES}-byte safety ceiling | stage=zip_extract"
        ));
    }
    if declared >= RATIO_CHECK_BYTES {
        let compressed = stat.comp;
        if compressed == 0 || declared / compressed > MAX_RATIO {
            return Err(format!(
                "ZIP entry '{name}' declares an implausible compression ratio ({declared} bytes from {compressed} bytes compressed) - likely a decompression bomb | stage=zip_extract"
            ));
        }
    }
    Ok(())
}

/// Raw deflate into exactly `size` bytes, as tinfl does with a non-wrapping output buffer:
/// more output than declared, less, or a broken stream fails. The buffer is reserved up front
/// but filled as the data inflates, so a size the stream does not back costs no memory.
fn inflate_exact(mut input: &[u8], size: u64) -> Result<Option<Vec<u8>>, String> {
    let size = usize::try_from(size).map_err(|_| OUT_OF_MEMORY.to_owned())?;
    let mut out = Vec::new();
    out.try_reserve_exact(size).map_err(|_| OUT_OF_MEMORY.to_owned())?;
    let mut state = InflateState::new_boxed(DataFormat::Raw);
    let mut chunk = vec![0u8; 256 * 1024];
    loop {
        let result = inflate(&mut state, input, &mut chunk, MZFlush::None);
        input = input.get(result.bytes_consumed..).unwrap_or(&[]);
        let written = chunk.get(..result.bytes_written).unwrap_or(&[]);
        if written.len() > size - out.len() {
            return Ok(None);
        }
        out.extend_from_slice(written);
        match result.status {
            Ok(MZStatus::StreamEnd) => break,
            Ok(_) if result.bytes_consumed > 0 || result.bytes_written > 0 => {}
            _ => return Ok(None),
        }
    }
    Ok((out.len() == size).then_some(out))
}

/// `mz_zip_reader_locate_header_sig`: the end-of-central-directory record, searched backwards
/// from the end in 4 KiB windows for at most about 64 KiB.
fn find_eocd(data: &[u8]) -> Option<usize> {
    const WINDOW: i64 = 4096;
    let size = data.len() as i64;
    let record = EOCD_SIZE as i64;
    if size < record {
        return None;
    }
    let mut cur = (size - WINDOW).max(0);
    loop {
        let n = WINDOW.min(size - cur);
        let mut i = n - 4;
        while i >= 0 {
            let at = usize::try_from(cur + i).ok()?;
            if u32_at(data, at) == Some(EOCD_SIG) && size - (cur + i) >= record {
                return Some(at);
            }
            i -= 1;
        }
        if cur == 0 || size - cur >= i64::from(u16::MAX) + record {
            return None;
        }
        cur = (cur - (WINDOW - 3)).max(0);
    }
}

/// Reads `n` bytes at a 64-bit offset (`mz_zip_mem_read_func` must return all of them).
fn read(data: &[u8], at: u64, n: usize) -> Option<&[u8]> {
    slice(data, usize::try_from(at).ok()?, n)
}

/// `mz_zip_reader_read_central_dir`, then `mz_zip_reader_sort_central_dir_offsets_by_filename`.
fn read_central_dir(data: &[u8]) -> Option<Zip<'_>> {
    let size = data.len() as u64;
    if data.len() < EOCD_SIZE {
        return None;
    }
    let eocd_ofs = find_eocd(data)?;
    let eocd = slice(data, eocd_ofs, EOCD_SIZE)?;
    if u32_at(eocd, 0)? != EOCD_SIG {
        return None;
    }
    let mut zip64 = false;
    let mut locator: &[u8] = &[];
    if eocd_ofs >= EOCD64_LOCATOR_SIZE + EOCD64_SIZE {
        locator = slice(data, eocd_ofs - EOCD64_LOCATOR_SIZE, EOCD64_LOCATOR_SIZE)?;
        zip64 = u32_at(locator, 0)? == EOCD64_LOCATOR_SIG;
    }
    let mut eocd64: &[u8] = &[];
    if zip64 {
        let valid = |at: u64| read(data, at, EOCD64_SIZE).filter(|r| u32_at(r, 0) == Some(EOCD64_SIG));
        let at = (eocd_ofs - EOCD64_LOCATOR_SIZE - EOCD64_SIZE) as u64;
        eocd64 = match valid(at) {
            Some(record) => record,
            None => {
                let at = u64_at(locator, 8)?;
                if at > size - EOCD64_SIZE as u64 {
                    return None;
                }
                valid(at)?
            }
        };
    }
    let mut total = u64::from(u16_at(eocd, 10)?);
    let mut on_disk = u64::from(u16_at(eocd, 8)?);
    let mut this_disk = u32::from(u16_at(eocd, 4)?);
    let mut cdir_disk = u32::from(u16_at(eocd, 6)?);
    let mut cdir_size = u64::from(u32_at(eocd, 12)?);
    let mut cdir_ofs = u64::from(u32_at(eocd, 16)?);
    if zip64 {
        let disks = u32_at(locator, 16)?;
        let total64 = u64_at(eocd64, 32)?;
        let on_disk64 = u64_at(eocd64, 24)?;
        let record_size = u64_at(eocd64, 4)?;
        let cdir_size64 = u64_at(eocd64, 40)?;
        if record_size < (EOCD64_SIZE - 12) as u64 || disks != 1 {
            return None;
        }
        let max = u64::from(u32::MAX);
        if total64 > max || on_disk64 > max || cdir_size64 > max {
            return None;
        }
        total = total64;
        on_disk = on_disk64;
        cdir_size = cdir_size64;
        this_disk = u32_at(eocd64, 16)?;
        cdir_disk = u32_at(eocd64, 20)?;
        cdir_ofs = u64_at(eocd64, 48)?;
    }
    if total != on_disk {
        return None;
    }
    if (this_disk | cdir_disk) != 0 && (this_disk != 1 || cdir_disk != 1) {
        return None;
    }
    if cdir_size < total * CDH_SIZE as u64 {
        return None;
    }
    if cdir_size > size || cdir_ofs > size - cdir_size {
        return None;
    }
    if (eocd_ofs as u64) < cdir_ofs + cdir_size {
        return None;
    }
    if zip64 && (eocd_ofs as u64) - (cdir_ofs + cdir_size) < (EOCD64_SIZE + EOCD64_LOCATOR_SIZE) as u64 {
        return None;
    }
    let cdir = read(data, cdir_ofs, usize::try_from(cdir_size).ok()?)?;
    let mut offsets = Vec::new();
    let mut has_extended_info = false;
    let mut p = 0usize;
    for _ in 0..total {
        let n = cdir.len() - p;
        let h = cdir.get(p..)?;
        if n < CDH_SIZE || u32_at(h, 0)? != CDH_SIG {
            return None;
        }
        offsets.push(p);
        let comp = u32_at(h, 20)?;
        let decomp = u32_at(h, 24)?;
        let local = u32_at(h, 42)?;
        let name_len = usize::from(u16_at(h, 28)?);
        let extra_len = usize::from(u16_at(h, 30)?);
        if !has_extended_info && extra_len > 0 && comp.max(decomp).max(local) == u32::MAX {
            // miniz reads an extra field that runs past this record from the start of the
            // central directory, not from the record. Kept as it is.
            let extra = if CDH_SIZE + name_len + extra_len > n {
                read(data, cdir_ofs + (CDH_SIZE + name_len) as u64, extra_len)?
            } else {
                slice(h, CDH_SIZE + name_len, extra_len)?
            };
            let mut remaining = extra_len;
            let mut at = 0usize;
            while remaining > 0 {
                if remaining < 4 {
                    return None;
                }
                let id = u16_at(extra, at)?;
                let field = usize::from(u16_at(extra, at + 2)?);
                if field + 4 > remaining {
                    return None;
                }
                if id == 1 {
                    has_extended_info = true;
                    break;
                }
                at += 4 + field;
                remaining -= 4 + field;
            }
        }
        if comp != u32::MAX && decomp != u32::MAX {
            // miniz reads the method as 32 bits, so the file time takes part too.
            let stored = u32_at(h, 10)? == 0;
            if (stored && decomp != comp) || (decomp != 0 && comp == 0) {
                return None;
            }
        }
        let disk = u32::from(u16_at(h, 34)?);
        if disk == u32::from(u16::MAX) || (disk != this_disk && disk != 1) {
            return None;
        }
        if comp != u32::MAX && u64::from(local) + LOCAL_SIZE as u64 + u64::from(comp) > size {
            return None;
        }
        if u16_at(h, 8)? & 0x2000 != 0 {
            return None;
        }
        let record = CDH_SIZE + name_len + extra_len + usize::from(u16_at(h, 32)?);
        if record > n {
            return None;
        }
        p += record;
    }
    let mut zip = Zip { data, cdir, sorted: (0..offsets.len()).collect(), offsets, names: Vec::new() };
    zip.sort();
    Some(zip)
}

impl Zip<'_> {
    /// `mz_zip_reader_filename_less`.
    fn name_less(&self, l: usize, r: usize) -> bool {
        let (a, b) = (self.raw_name(l), self.raw_name(r));
        match a.iter().zip(b).find(|(x, y)| lower(**x) != lower(**y)) {
            Some((x, y)) => lower(*x) < lower(*y),
            None => a.len() < b.len(),
        }
    }

    /// `mz_zip_reader_sort_central_dir_offsets_by_filename`: miniz's heap sort, kept exactly so
    /// that names equal but for case resolve to the same entry.
    fn sort(&mut self) {
        let size = self.sorted.len();
        if size <= 1 {
            return;
        }
        let mut indices = std::mem::take(&mut self.sorted);
        let sift = |indices: &mut [usize], mut root: usize, end: usize| {
            loop {
                let mut child = root * 2 + 1;
                if child >= end {
                    break;
                }
                if child + 1 < end && self.name_less(indices[child], indices[child + 1]) {
                    child += 1;
                }
                if !self.name_less(indices[root], indices[child]) {
                    break;
                }
                indices.swap(root, child);
                root = child;
            }
        };
        let mut start = (size - 2) >> 1;
        loop {
            sift(&mut indices, start, size);
            if start == 0 {
                break;
            }
            start -= 1;
        }
        let mut end = size - 1;
        while end > 0 {
            indices.swap(end, 0);
            sift(&mut indices, 0, end);
            end -= 1;
        }
        self.sorted = indices;
    }
}
