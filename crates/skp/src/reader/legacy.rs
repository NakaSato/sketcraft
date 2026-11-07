//! SketchUp 3 to 2020 files: an MFC object archive (`parse_legacy` in `legacy.cpp`).
//!
//! The archive is a stream of objects. An object is written in full the first time anything
//! points at it and referenced by slot number afterwards, so reading is one recursive walk that
//! numbers slots exactly as the writer did. This is a line-by-line port of `Archive` and the walk
//! around it, including its recovery heuristics (the SketchUp 2020 filler, burned slot numbers
//! after annotations, the slot-base probes), because any difference in how bytes are consumed
//! changes every slot number after it.
//!
//! What the import never reads is consumed but not kept: attribute dictionaries, textures,
//! cameras, thumbnails, scenes, text, dimensions and construction geometry. Scenes
//! (`scan_pages_for_layers`) are skipped entirely; the C++ ran them last and caught their errors.
//!
//! Every error here becomes the same message, as in the C++ ("legacy .skp parse failed"), so
//! errors carry no detail. Recursion is limited to [`MAX_OBJECT_DEPTH`] nested objects; the C++
//! had no limit and overflowed the stack instead.

use std::collections::HashMap;
use std::rc::Rc;

use super::bytes::{f64_at, slice, u16_at, u32_at};
use super::{Builder, CoEdge, Definition, Parsed, RawFace, RawInstance};

/// Any failure; see the module comment.
pub(super) struct Fail;

type Res<T> = Result<T, Fail>;

/// SketchUp 3 lacks fields every later version writes (`kFirstV4`).
const FIRST_V4: i32 = 4;
/// Textures became `CDib` objects in SketchUp 6 (`kFirstDibObject`).
const FIRST_DIB_OBJECT: i32 = 6;
/// Instances carry a name from SketchUp 5 (`kFirstNamedInstance`).
const FIRST_NAMED_INSTANCE: i32 = 5;
/// `kMaxV20FillerPad`.
const MAX_V20_FILLER_PAD: usize = 29;
/// Nested objects (a definition placed in a definition is about two levels). The import
/// accepts 64 levels of components, well inside this.
const MAX_OBJECT_DEPTH: u32 = 256;
/// The string record marker.
const MARKER: [u8; 3] = [255, 254, 255];

type Name = Rc<[u8]>;

/// The display block every drawable entity carries (`Archive::draw`).
#[derive(Clone, Copy, Default)]
struct Draw {
    mat: i32,
    hidden: i32,
    soft: i32,
    smooth: i32,
    layer: i32,
}

/// A decoded object (`V` in `internal.hpp`), keeping only what the import reads. The accessors
/// return the C++ field defaults for kinds that never set a field, because the C++ reads fields
/// off whatever object a reference names.
enum V {
    Other,
    Drawn(Draw),
    SectionPlane {
        draw: Draw,
        name: Vec<u8>,
    },
    Dict(Vec<u8>),
    Vertex([f64; 3]),
    Edge {
        draw: Draw,
        v1: u64,
        v2: u64,
    },
    EdgeUse {
        edge: u64,
        sense: bool,
    },
    /// The (edge, sense) of each use.
    Loop(Vec<(u64, bool)>),
    /// Loops; `None` stands for a reference to something that is not a loop (no uses).
    Face {
        draw: Draw,
        loops: Vec<Option<Rc<V>>>,
    },
    Definition {
        name: Vec<u8>,
        ents: Vec<(u64, Option<Rc<V>>)>,
    },
    Instance {
        draw: Draw,
        def: u64,
        xf: Vec<f64>,
        name: Vec<u8>,
    },
    Image {
        draw: Draw,
        def: u64,
        xf: Vec<f64>,
    },
    Layer {
        name: Vec<u8>,
        hidden: i32,
        rgb: [i32; 3],
    },
    Material {
        name: Vec<u8>,
        rgb: [i32; 3],
    },
}

impl V {
    fn draw(&self) -> Draw {
        match self {
            V::Drawn(draw)
            | V::SectionPlane { draw, .. }
            | V::Edge { draw, .. }
            | V::Face { draw, .. }
            | V::Instance { draw, .. }
            | V::Image { draw, .. } => *draw,
            _ => Draw::default(),
        }
    }

    fn name(&self) -> &[u8] {
        match self {
            V::SectionPlane { name, .. }
            | V::Dict(name)
            | V::Definition { name, .. }
            | V::Instance { name, .. }
            | V::Layer { name, .. }
            | V::Material { name, .. } => name,
            _ => &[],
        }
    }

    fn hidden(&self) -> i32 {
        match self {
            V::Layer { hidden, .. } => *hidden,
            _ => self.draw().hidden,
        }
    }

    fn rgb(&self) -> [i32; 3] {
        match self {
            V::Layer { rgb, .. } | V::Material { rgb, .. } => *rgb,
            _ => [128; 3],
        }
    }

    fn ends(&self) -> (u64, u64) {
        match self {
            V::Edge { v1, v2, .. } => (*v1, *v2),
            _ => (0, 0),
        }
    }

    fn uses(&self) -> &[(u64, bool)] {
        match self {
            V::Loop(uses) => uses,
            _ => &[],
        }
    }

    fn ents(&self) -> &[(u64, Option<Rc<V>>)] {
        match self {
            V::Definition { ents, .. } => ents,
            _ => &[],
        }
    }

    /// Whether an entity list keeps this object for `fill`. Dropping the rest keeps chains of
    /// shared objects shallow.
    fn fills(&self) -> bool {
        matches!(self, V::Edge { .. } | V::Face { .. } | V::Instance { .. } | V::Image { .. })
    }
}

/// `Archive::object`'s result: slot, class name, value.
struct Obj {
    slot: u64,
    name: Name,
    v: Option<Rc<V>>,
}

/// A slot-table entry (`LegacySlotEntry`): a class declaration or an object.
struct Entry {
    cls: bool,
    name: Name,
    v: Option<Rc<V>>,
}

impl Default for Entry {
    fn default() -> Self {
        Self { cls: false, name: Rc::from(&[][..]), v: None }
    }
}

fn is_marker(d: &[u8], at: usize) -> bool {
    slice(d, at, 3) == Some(&MARKER[..])
}

/// `is_class_ref`: a reference to class `slot` in either encoding.
fn is_class_ref(d: &[u8], p: usize, slot: u64) -> bool {
    if slot <= 0x7fff {
        return u16_at(d, p).is_some_and(|t| u64::from(t) == (0x8000 | slot));
    }
    u16_at(d, p) == Some(0x7fff) && u32_at(d, p + 2).is_some_and(|t| u64::from(t) == u64::from(0x8000_0000 | slot as u32))
}

fn named(d: &[u8], at: usize, name: &[u8]) -> bool {
    slice(d, at, name.len()) == Some(name)
}

struct Archive<'a> {
    d: &'a [u8],
    p: usize,
    ver: i32,
    pid: bool,
    next: u64,
    base: u64,
    current_loop: u64,
    slots: HashMap<u64, Entry>,
    class_slot: HashMap<Name, u64>,
    /// Burned slot numbers: (file band start, width). See `register_burn`.
    burns: Vec<(u64, u64)>,
    cum_delta: u64,
    annot_watermark: Option<u64>,
    burn_stack: Vec<i32>,
    cline_tail: Option<usize>,
    depth: u32,
    empty: Name,
    premodel: Name,
    reserved: Name,
}

impl<'a> Archive<'a> {
    fn new(d: &'a [u8], ver: i32) -> Self {
        Self {
            d,
            p: 0,
            ver,
            pid: ver >= 17,
            next: 0,
            base: 0,
            current_loop: 0,
            slots: HashMap::new(),
            class_slot: HashMap::new(),
            burns: Vec::new(),
            cum_delta: 0,
            annot_watermark: None,
            burn_stack: Vec::new(),
            cline_tail: None,
            depth: 0,
            empty: Rc::from(&[][..]),
            premodel: Rc::from(&b"premodel"[..]),
            reserved: Rc::from(&b"reserved"[..]),
        }
    }

    // ── the byte reader (`R`) ──

    fn need(&self, n: usize) -> Res<()> {
        if self.p > self.d.len() || n > self.d.len() - self.p { Err(Fail) } else { Ok(()) }
    }

    fn raw(&mut self, n: usize) -> Res<&'a [u8]> {
        self.need(n)?;
        let bytes = slice(self.d, self.p, n).ok_or(Fail)?;
        self.p += n;
        Ok(bytes)
    }

    fn skip(&mut self, n: usize) -> Res<()> {
        self.raw(n).map(|_| ())
    }

    fn u8(&mut self) -> Res<u8> {
        self.raw(1)?.first().copied().ok_or(Fail)
    }

    fn u16(&mut self) -> Res<u16> {
        let v = u16_at(self.d, self.p).ok_or(Fail)?;
        self.p += 2;
        Ok(v)
    }

    fn u32(&mut self) -> Res<u32> {
        let v = u32_at(self.d, self.p).ok_or(Fail)?;
        self.p += 4;
        Ok(v)
    }

    fn f64(&mut self) -> Res<f64> {
        let v = f64_at(self.d, self.p).ok_or(Fail)?;
        self.p += 8;
        Ok(v)
    }

    fn f64s(&mut self, n: usize) -> Res<Vec<f64>> {
        (0..n).map(|_| self.f64()).collect()
    }

    fn peek16(&self, at: usize) -> Res<u16> {
        u16_at(self.d, at).ok_or(Fail)
    }

    /// A null object tag at `p`.
    fn null_ahead(&self) -> bool {
        u16_at(self.d, self.p) == Some(0)
    }

    fn marker(&self) -> bool {
        is_marker(self.d, self.p)
    }

    /// The length in UTF-16 units of the string record at `p`, which is consumed up to its
    /// characters.
    fn string_len(&mut self) -> Res<usize> {
        if !self.marker() {
            return Err(Fail);
        }
        self.p += 3;
        let n = self.u8()?;
        let mut z = u32::from(n);
        if n == 255 {
            z = u32::from(self.u16()?);
            if z == 65535 {
                z = self.u32()?;
            }
        }
        let z = usize::try_from(z).map_err(|_| Fail)?;
        self.need(z.checked_mul(2).ok_or(Fail)?)?;
        Ok(z)
    }

    /// `R::utf16`: each code unit to one to three bytes (surrogates are not paired).
    fn utf16(&mut self) -> Res<Vec<u8>> {
        let z = self.string_len()?;
        let units = self.raw(z * 2)?;
        let mut s = Vec::with_capacity(z);
        for unit in units.as_chunks::<2>().0 {
            let c = u16::from_le_bytes(*unit);
            if c < 0x80 {
                s.push(c as u8);
            } else if c < 0x800 {
                s.extend([0xc0 | (c >> 6) as u8, 0x80 | (c & 63) as u8]);
            } else {
                s.extend([0xe0 | (c >> 12) as u8, 0x80 | ((c >> 6) & 63) as u8, 0x80 | (c & 63) as u8]);
            }
        }
        Ok(s)
    }

    /// A string that is not kept; returns its length.
    fn skip_utf16(&mut self) -> Res<usize> {
        let z = self.string_len()?;
        self.skip(z * 2)?;
        Ok(z)
    }

    // ── the object graph ──

    fn alloc(&mut self, entry: Entry) -> u64 {
        let slot = self.next;
        self.next = self.next.wrapping_add(1);
        self.slots.insert(slot, entry);
        slot
    }

    fn null(&self) -> Obj {
        Obj { slot: 0, name: self.empty.clone(), v: None }
    }

    /// `Archive::object`: null, a back-reference, a reference to a known class (a new object of
    /// it follows), or a class declaration (likewise).
    fn object(&mut self, expect: Option<&'static [u8]>) -> Res<Obj> {
        let tag = self.u16()?;
        if tag == 0 {
            return Ok(self.null());
        }
        if tag == 0x7fff {
            let big = self.u32()?;
            if big & 0x8000_0000 != 0 {
                return self.new_class_ref(u64::from(big & 0x7fff_ffff), expect);
            }
            return self.back(u64::from(big));
        }
        if tag == 0xffff {
            let _schema = self.u16()?;
            let n = self.u16()?;
            if n > 40 {
                return Err(Fail);
            }
            let name: Name = Rc::from(self.raw(usize::from(n))?);
            let slot = self.alloc(Entry { cls: true, name: name.clone(), v: None });
            self.class_slot.insert(name.clone(), slot);
            return self.new_obj(name);
        }
        if tag & 0x8000 != 0 {
            return self.new_class_ref(u64::from(tag & 0x7fff), expect);
        }
        self.back(u64::from(tag))
    }

    fn new_class_ref(&mut self, slot: u64, expect: Option<&'static [u8]>) -> Res<Obj> {
        if let std::collections::hash_map::Entry::Vacant(vacant) = self.slots.entry(slot) {
            let expect: Name = Rc::from(expect.ok_or(Fail)?);
            vacant.insert(Entry { cls: true, name: expect.clone(), v: None });
            self.class_slot.insert(expect, slot);
        }
        let entry = self.slots.get(&slot).ok_or(Fail)?;
        if !entry.cls {
            return Err(Fail);
        }
        let name = entry.name.clone();
        self.new_obj(name)
    }

    /// `translate_ref`: a file slot number to the walker's numbering through the burned bands;
    /// `None` for a number inside a band.
    fn translate_ref(&self, slot: u64) -> Option<u64> {
        let mut offset = 0u64;
        for &(start, width) in &self.burns {
            if slot < start {
                break;
            }
            if slot < start.wrapping_add(width) {
                return None;
            }
            offset = offset.wrapping_add(width);
        }
        Some(slot.wrapping_sub(offset))
    }

    fn back(&self, mut slot: u64) -> Res<Obj> {
        if let Some(&(first, _)) = self.burns.first()
            && slot >= first
        {
            match self.translate_ref(slot) {
                Some(walker) => slot = walker,
                None => return Ok(Obj { slot, name: self.reserved.clone(), v: None }),
            }
        }
        match self.slots.get(&slot) {
            None if slot < self.base => Ok(Obj { slot, name: self.premodel.clone(), v: None }),
            None => Err(Fail),
            Some(entry) if entry.cls => Err(Fail),
            Some(entry) => Ok(Obj { slot, name: entry.name.clone(), v: entry.v.clone() }),
        }
    }

    fn register_burn(&mut self, delta: u64) {
        let watermark = self.annot_watermark.take().unwrap_or(0);
        self.burns.push((watermark.wrapping_add(self.cum_delta), delta));
        self.cum_delta = self.cum_delta.wrapping_add(delta);
        if let Some(credit) = self.burn_stack.last_mut() {
            *credit += 1;
        }
    }

    /// `entity_ref`: a reference that may point forward; never a new object.
    fn entity_ref(&mut self) -> Res<()> {
        let tag = self.u16()?;
        if tag == 0x7fff {
            if self.u32()? & 0x8000_0000 != 0 {
                return Err(Fail);
            }
        } else if tag == 0xffff || tag & 0x8000 != 0 {
            return Err(Fail);
        }
        Ok(())
    }

    /// `legacy_connection_paths`: the instance paths after a dimension's connection point.
    fn connection_paths(&mut self) -> Res<()> {
        let d = self.d;
        let mut q = self.p;
        plain_entity_ref(d, &mut q)?;
        for _ in 0..2 {
            let count = u32_at(d, q).ok_or(Fail)?;
            q += 4;
            if count > 1000 {
                return Err(Fail);
            }
            for _ in 0..count {
                let tag = u16_at(d, q).ok_or(Fail)?;
                let mut is_new = tag == 0xffff || (tag != 0x7fff && tag & 0x8000 != 0);
                if tag == 0x7fff {
                    is_new = u32_at(d, q + 2).ok_or(Fail)? & 0x8000_0000 != 0;
                }
                if is_new {
                    self.p = q;
                    self.object(None)?;
                    q = self.p;
                } else {
                    plain_entity_ref(d, &mut q)?;
                }
            }
        }
        self.p = q;
        Ok(())
    }

    /// `strict_next_tag`: an unambiguous start of an object read at `at`.
    fn strict_next_tag(&self, at: usize, allow_null: bool) -> bool {
        let Some(t) = u16_at(self.d, at) else {
            return false;
        };
        match t {
            0 => allow_null,
            0x7fff | 0xffff => true,
            t if t & 0x8000 != 0 => self.slots.get(&u64::from(t & 0x7fff)).is_some_and(|e| e.cls),
            _ => false,
        }
    }

    /// `plausible_list_tag`.
    fn plausible_list_tag(&self, at: usize) -> bool {
        let Some(t) = u16_at(self.d, at) else {
            return false;
        };
        match t {
            0 | 0x7fff | 0xffff => true,
            t if t & 0x8000 != 0 => self.slots.get(&u64::from(t & 0x7fff)).is_some_and(|e| e.cls),
            t => u64::from(t) < self.next,
        }
    }

    /// `retry_count_after_v20_filler`: SketchUp 2020 sometimes writes an empty string and zero
    /// padding before a count. Finds the count behind it and moves past it.
    fn retry_count_after_v20_filler(&mut self, count_pos: usize, limit: u32) -> Option<u32> {
        let d = self.d;
        let window = count_pos..count_pos.saturating_add(12);
        let marker_at = window.take_while(|&i| i.saturating_add(4) <= d.len()).find(|&i| is_marker(d, i))?;
        if d.get(marker_at + 3) != Some(&0) {
            return None;
        }
        for pad in (1..=MAX_V20_FILLER_PAD).step_by(4) {
            let at = marker_at + 4 + pad;
            let count = u32_at(d, at)?;
            if count > 0 && count <= limit && self.plausible_list_tag(at + 4) {
                self.p = at + 4;
                return Some(count);
            }
        }
        None
    }

    fn class(&self, name: &[u8]) -> Option<u64> {
        self.class_slot.get(name).copied()
    }

    /// `clayer_record_at`.
    fn clayer_record_at(&self, at: usize, unmatched_class_ref: bool) -> bool {
        let Some(tag) = u16_at(self.d, at) else {
            return false;
        };
        if tag == 0xffff && u16_at(self.d, at + 4) == Some(6) && named(self.d, at + 6, b"CLayer") {
            return true;
        }
        if self.class(b"CLayer").is_some_and(|cs| is_class_ref(self.d, at, cs)) {
            return true;
        }
        unmatched_class_ref && tag & 0x8000 != 0 && tag != 0xffff
    }

    /// `skip_clayer_colour_ext`.
    fn skip_clayer_colour_ext(&mut self) {
        const PAD: usize = 12;
        const EXT: usize = 16;
        if self.p.saturating_add(EXT + 2) > self.d.len() {
            return;
        }
        if slice(self.d, self.p, PAD).is_some_and(|pad| pad.iter().all(|&b| b == 0)) && self.clayer_record_at(self.p + EXT, true) {
            self.p += EXT;
        }
    }

    /// `skip_clayer_parent_ref`.
    fn skip_clayer_parent_ref(&mut self, own: u64) {
        let (Some(a), Some(b)) = (u16_at(self.d, self.p), u16_at(self.d, self.p.saturating_add(2))) else {
            return;
        };
        let plain = |t: u16| t != 0 && t & 0x8000 == 0 && t != 0x7fff;
        if !plain(a) || u64::from(a) == own || !plain(b) {
            return;
        }
        let is_layer = |t: u16| self.slots.get(&u64::from(t)).is_some_and(|e| !e.cls && &*e.name == b"CLayer");
        if is_layer(a) && is_layer(b) {
            self.p += 2;
        }
    }

    /// `collect_trailing_layers`: layer records past the declared count, skipping nulls.
    fn collect_trailing_layers(&mut self, unmatched_class_ref: bool) -> Res<Vec<(u64, Rc<V>)>> {
        let mut layers = Vec::new();
        while let Some(tag) = u16_at(self.d, self.p) {
            if tag == 0 {
                self.p += 2;
                continue;
            }
            if !self.clayer_record_at(self.p, unmatched_class_ref) {
                break;
            }
            let q = self.object(Some(b"CLayer"))?;
            if let Some(v) = q.v {
                layers.push((q.slot, v));
            }
        }
        Ok(layers)
    }

    fn new_obj(&mut self, name: Name) -> Res<Obj> {
        if self.depth >= MAX_OBJECT_DEPTH {
            return Err(Fail);
        }
        let slot = self.alloc(Entry { cls: false, name: name.clone(), v: None });
        self.depth += 1;
        let v = self.read(&name, slot);
        self.depth -= 1;
        let v = Rc::new(v?);
        self.slots.entry(slot).or_default().v = Some(v.clone());
        if &*name == b"CDimensionLinear" || &*name == b"CText" {
            self.annot_watermark = Some(self.next);
        }
        Ok(Obj { slot, name, v: Some(v) })
    }

    /// `preamble`: the attribute-container reference and, from SketchUp 2017, the
    /// persistent-ID mask. Returns the container's slot and the reference's class name.
    fn preamble(&mut self) -> Res<(Option<u64>, Name)> {
        let attrs = self.object(Some(b"CAttributeContainer"))?;
        if self.pid {
            let mask = self.u8()?;
            for i in 0..8 {
                if mask & (1 << i) != 0 {
                    self.u8()?;
                }
            }
        }
        Ok(((!attrs.name.is_empty()).then_some(attrs.slot), attrs.name))
    }

    /// `early_preamble`: absent in SketchUp 3 for some entity types.
    fn early_preamble(&mut self) -> Res<()> {
        if self.ver >= FIRST_V4 {
            self.preamble()?;
        }
        Ok(())
    }

    /// `Archive::draw`.
    fn draw(&mut self) -> Res<Draw> {
        let v3 = self.ver < FIRST_V4;
        let b = self.raw(if v3 {
            5
        } else if self.ver < 5 {
            7
        } else {
            8
        })?;
        let mut draw = Draw::default();
        if !v3 {
            let byte = |i: usize| b.get(i).map_or(0, |&v| i32::from(v));
            draw.mat = byte(0) | byte(1) << 8;
            draw.hidden = byte(2);
            draw.soft = byte(5);
            draw.smooth = byte(6);
        }
        let tag = self.peek16(self.p)?;
        if self.class(b"CLayer").is_some_and(|cs| u64::from(tag) == (0x8000 | cs)) {
            self.object(Some(b"CLayer"))?;
        } else if tag == 0x7fff {
            self.u16()?;
            if self.u32()? & 0x8000_0000 != 0 {
                return Err(Fail);
            }
        } else {
            draw.layer = i32::from(self.u16()?);
        }
        Ok(draw)
    }

    /// `texture_block`: the texture of a material or a colour-by-layer layer. Returns its
    /// average colour.
    fn texture_block(&mut self) -> Res<[i32; 3]> {
        if self.ver < FIRST_DIB_OBJECT {
            self.u32()?;
            let z = self.u32()?;
            if u64::from(z) > self.d.len() as u64 {
                return Err(Fail);
            }
            self.skip(z as usize)?;
        } else {
            self.p = self.p.saturating_sub(1);
            self.preamble()?;
            self.object(Some(b"CDib"))?;
        }
        let begin = self.p;
        let limit = self.d.len().min(self.p.saturating_add(28));
        let mut marker = begin;
        while marker + 3 <= limit && !is_marker(self.d, marker) {
            marker += 1;
        }
        match marker - begin {
            20 => {
                self.u32()?;
            }
            16 => {}
            _ => return Err(Fail),
        }
        self.f64()?;
        self.f64()?;
        self.skip_utf16()?;
        let c = self.raw(9)?;
        let rgb = [0, 1, 2].map(|i| c.get(i).map_or(0, |&v| i32::from(v)));
        self.skip_utf16()?;
        self.skip(8)?;
        self.f64()?;
        self.u8()?;
        Ok(rgb)
    }

    /// `Archive::read`: one object's body, by class name.
    fn read(&mut self, n: &[u8], own: u64) -> Res<V> {
        match n {
            b"CVertex" => {
                self.early_preamble()?;
                Ok(V::Vertex([self.f64()?, self.f64()?, self.f64()?]))
            }
            b"CEdge" => {
                self.preamble()?;
                let draw = self.draw()?;
                let v1 = self.object(Some(b"CVertex"))?.slot;
                let v2 = self.object(Some(b"CVertex"))?.slot;
                self.object(None)?;
                Ok(V::Edge { draw, v1, v2 })
            }
            b"CCurve" => {
                self.preamble()?;
                self.u8()?;
                self.u32()?;
                Ok(V::Other)
            }
            b"CArcCurve" => {
                self.preamble()?;
                self.skip(5)?;
                self.f64s(14)?;
                Ok(V::Other)
            }
            b"CEdgeUse" => self.read_edge_use(),
            b"CLoop" => self.read_loop(own),
            b"CFace" => self.read_face(),
            b"CAttributeContainer" => {
                self.preamble()?;
                while self.p.saturating_add(2) <= self.d.len() && !self.null_ahead() {
                    self.object(Some(b"CAttributeNamed"))?;
                }
                self.u16()?;
                Ok(V::Other)
            }
            b"CAttributeNamed" => {
                self.preamble()?;
                self.skip(4)?;
                let name = self.utf16()?;
                while self.skip_utf16()? != 0 {
                    let t = self.u8()?;
                    self.typed(t)?;
                }
                // The trailing u32 is a SketchUp 7 addition.
                if self.ver >= 7 {
                    self.u32()?;
                }
                Ok(V::Dict(name))
            }
            b"CLayer" => self.read_layer(own),
            b"CMaterial" => {
                self.early_preamble()?;
                let name = self.utf16()?;
                let rgb = if self.u16()? == 0 {
                    let c = self.raw(4)?;
                    let rgb = [0, 1, 2].map(|i| c.get(i).map_or(0, |&v| i32::from(v)));
                    self.skip_utf16()?;
                    self.skip(8)?;
                    self.f64()?;
                    self.u8()?;
                    rgb
                } else {
                    self.texture_block()?
                };
                Ok(V::Material { name, rgb })
            }
            b"CDib" => {
                self.u32()?;
                let z = self.u32()?;
                if u64::from(z) > self.d.len() as u64 {
                    return Err(Fail);
                }
                self.skip(z as usize)?;
                Ok(V::Other)
            }
            b"CFaceTextureCoords" => {
                self.preamble()?;
                self.u32()?;
                self.f64s(24)?;
                for _ in 0..2 {
                    let z = usize::try_from(self.u32()?).map_err(|_| Fail)?;
                    self.skip(z.checked_mul(32).ok_or(Fail)?)?;
                }
                self.u32()?;
                self.u32()?;
                Ok(V::Other)
            }
            b"CCamera" => {
                self.skip(137)?;
                self.u16()?;
                self.skip_utf16()?;
                self.skip(if self.ver >= 6 { 33 } else { 10 })?;
                Ok(V::Other)
            }
            b"CThumbnail" => {
                self.preamble()?;
                if self.ver < FIRST_V4 {
                    self.skip(129)?;
                } else {
                    self.object(Some(b"CCamera"))?;
                    if self.ver >= 6 {
                        self.object(Some(b"CDib"))?;
                    }
                }
                Ok(V::Other)
            }
            b"CImage" => {
                self.preamble()?;
                let draw = self.draw()?;
                let def = self.object(None)?.slot;
                let xf = self.f64s(12)?;
                self.f64()?;
                self.skip_utf16()?;
                self.skip(16)?;
                Ok(V::Image { draw, def, xf })
            }
            b"CRelationship" => {
                self.preamble()?;
                self.entity_ref()?;
                self.entity_ref()?;
                Ok(V::Other)
            }
            b"CConstructionLine" => self.read_construction_line(),
            b"CConstructionPoint" => {
                self.preamble()?;
                let draw = self.draw()?;
                self.f64s(3)?;
                self.f64s(3)?;
                self.u8()?;
                Ok(V::Drawn(draw))
            }
            b"CSectionPlane" => {
                self.preamble()?;
                let draw = self.draw()?;
                let first = f64_at(self.d, self.p).ok_or(Fail)?;
                if first.abs() > 1.0001 {
                    self.object(None)?;
                }
                self.f64s(4)?;
                let mut name = Vec::new();
                if self.marker() {
                    name = self.utf16()?;
                    self.skip_utf16()?;
                }
                Ok(V::SectionPlane { draw, name })
            }
            b"CSkFont" => {
                self.object(Some(b"CAttributeContainer"))?;
                if self.pid {
                    self.u8()?;
                }
                self.skip_utf16()?;
                self.skip(15)?;
                Ok(V::Other)
            }
            b"CDimensionLinear" => {
                self.preamble()?;
                let draw = self.draw()?;
                self.skip_utf16()?;
                let v6 = self.ver >= 6;
                if v6 {
                    self.object(Some(b"CSkFont"))?;
                }
                self.skip(if v6 { 37 } else { 32 })?;
                self.entity_ref()?;
                self.connection_paths()?;
                self.skip(32)?;
                self.entity_ref()?;
                self.connection_paths()?;
                self.skip(if v6 { 72 } else { 68 })?;
                Ok(V::Drawn(draw))
            }
            b"CText" => self.read_text(),
            b"CComponentDefinition" => self.read_definition(),
            b"CComponentInstance" | b"CGroup" => {
                self.preamble()?;
                let draw = self.draw()?;
                let def = self.object(Some(b"CComponentDefinition"))?.slot;
                let xf = self.f64s(13)?;
                let mut name = Vec::new();
                if self.ver >= FIRST_NAMED_INSTANCE || self.marker() {
                    name = self.utf16()?;
                }
                if self.ver >= 14 {
                    self.skip(16)?;
                }
                Ok(V::Instance { draw, def, xf, name })
            }
            _ => Err(Fail),
        }
    }

    fn read_edge_use(&mut self) -> Res<V> {
        self.early_preamble()?;
        let edge = self.object(Some(b"CEdge"))?.slot;
        let sense = self.u8()? != 0;
        // The parent-loop reference keeps the walk aligned; one that runs ahead after an
        // annotation means slot numbers were burned (`register_burn`).
        let p0 = self.p;
        let tag = self.u16()?;
        let parent = if tag == 0x7fff {
            let big = self.u32()?;
            if big & 0x8000_0000 != 0 {
                return Err(Fail);
            }
            Some(u64::from(big))
        } else if tag == 0xffff || tag & 0x8000 != 0 {
            return Err(Fail);
        } else {
            (tag != 0).then_some(u64::from(tag))
        };
        let expected = self.current_loop.wrapping_add(self.cum_delta);
        if parent != Some(expected) {
            let delta = parent.map_or(0, |p| (p as i64).wrapping_sub(expected as i64));
            if delta > 0 && delta <= 4096 && self.annot_watermark.is_some() {
                self.register_burn(delta as u64);
            } else {
                self.p = p0;
                return Err(Fail);
            }
        }
        Ok(V::EdgeUse { edge, sense })
    }

    fn read_loop(&mut self, own: u64) -> Res<V> {
        let old = self.current_loop;
        self.current_loop = own;
        self.early_preamble()?;
        self.skip(2)?;
        let mut uses = Vec::new();
        while self.p.saturating_add(2) <= self.d.len() && !self.null_ahead() {
            if let Some(v) = self.object(Some(b"CEdgeUse"))?.v {
                uses.push(match *v {
                    V::EdgeUse { edge, sense } => (edge, sense),
                    _ => (0, false),
                });
            }
        }
        self.u16()?;
        self.current_loop = old;
        Ok(V::Loop(uses))
    }

    fn read_face(&mut self) -> Res<V> {
        let (attrs, lead) = self.preamble()?;
        let mut draw = self.draw()?;
        // SketchUp 3: the pointer that opens a face is its front material.
        if self.ver < FIRST_V4
            && let Some(attrs) = attrs
            && &*lead == b"CMaterial"
        {
            draw.mat = attrs as i32;
        }
        self.f64s(4)?;
        let count = self.u32()?;
        if count > 10000 {
            return Err(Fail);
        }
        let mut loops = Vec::new();
        for _ in 0..count {
            if let Some(v) = self.object(Some(b"CLoop"))?.v {
                loops.push(matches!(*v, V::Loop(_)).then_some(v));
            }
        }
        self.u16()?;
        Ok(V::Face { draw, loops })
    }

    fn read_layer(&mut self, own: u64) -> Res<V> {
        self.early_preamble()?;
        let name = self.utf16()?;
        let mut hidden = None;
        let mut mid = 0;
        while mid < 8 && !self.marker() {
            let b = self.u8()?;
            hidden.get_or_insert(i32::from(b));
            mid += 1;
        }
        self.skip_utf16()?;
        let flags = self.u16()?;
        let rgb = if flags & 0x00ff != 0 {
            // Colour by layer with a textured material.
            let rgb = self.texture_block()?;
            self.skip(4)?;
            rgb
        } else {
            let c = self.raw(4)?;
            let rgb = [0, 1, 2].map(|i| c.get(i).map_or(0, |&v| i32::from(v)));
            self.skip_utf16()?;
            self.skip(if self.ver < FIRST_V4 { 17 } else { 21 })?;
            self.skip_clayer_colour_ext();
            self.skip_clayer_parent_ref(own);
            rgb
        };
        Ok(V::Layer { name, hidden: hidden.unwrap_or(0), rgb })
    }

    fn read_construction_line(&mut self) -> Res<V> {
        self.preamble()?;
        let draw = self.draw()?;
        self.f64s(8)?;
        // The trailing block's length depends on the writing build: calibrate on the first
        // guide line of the file, preferring lengths that land on an unambiguous tag.
        let tail = match self.cline_tail {
            Some(tail) => tail,
            None => {
                let preferred = if self.ver == 17 { 7 } else { 4 };
                let order = std::iter::once(preferred).chain([0, 4, 7].into_iter().filter(|&c| c != preferred));
                let strong = order.clone().find(|&c| self.strict_next_tag(self.p.saturating_add(c), false));
                let tail = strong.or_else(|| [4, 0, 7].into_iter().find(|&c| self.strict_next_tag(self.p.saturating_add(c), true))).unwrap_or(4);
                self.cline_tail = Some(tail);
                tail
            }
        };
        self.skip(tail)?;
        Ok(V::Drawn(draw))
    }

    fn read_text(&mut self) -> Res<V> {
        self.preamble()?;
        let draw = self.draw()?;
        self.object(Some(b"CSkFont"))?;
        let d = self.d;
        let end = d.len().min(self.p.saturating_add(512));
        let pattern =
            |q: usize| slice(d, q, 14).is_some_and(|b| b[..4] == [1, 0, 0, 0] && b[6..10] == [3, 0, 0, 0] && b[10] == 1 && b[11..14] == MARKER);
        let found = (self.p..end.saturating_sub(13)).find(|&q| pattern(q)).ok_or(Fail)? + 11;
        self.skip(found - self.p)?;
        self.skip_utf16()?;
        self.skip(5)?;
        // Leader attachments, only in the escaped 6-byte form.
        while slice(d, self.p, 2) == Some(&[255, 127][..]) {
            let Some(val) = u32_at(d, self.p + 2) else {
                break;
            };
            if val & 0x8000_0000 != 0 {
                break;
            }
            self.skip(6)?;
        }
        Ok(V::Drawn(draw))
    }

    fn read_definition(&mut self) -> Res<V> {
        let ver = self.ver;
        self.preamble()?;
        self.skip(if ver >= 17 {
            22
        } else if ver >= 5 {
            20
        } else if ver >= FIRST_V4 {
            19
        } else {
            13
        })?;
        let layer_count = self.u32()?;
        if layer_count > 10000 {
            return Err(Fail);
        }
        // The count is of real layers; SketchUp 2020 puts nulls between them.
        let mut got = 0;
        while got < layer_count {
            if self.null_ahead() {
                self.p += 2;
                continue;
            }
            self.object(Some(b"CLayer"))?;
            got += 1;
        }
        if ver == 19 && layer_count > 0 && self.null_ahead() {
            self.p += 2;
        }
        if self.u16()? == 0x7fff {
            self.u32()?;
        }
        let mut count = None;
        if ver >= 20 {
            count = self.retry_count_after_v20_filler(self.p, 5_000_000);
        }
        let mut count = match count {
            Some(count) => count,
            None => {
                self.u32()?;
                self.u32()?
            }
        };
        if (count > 5_000_000 || count == 0)
            && let Some(retry) = self.retry_count_after_v20_filler(self.p.saturating_sub(4), 5_000_000)
        {
            count = retry;
        }
        if count > 5_000_000 {
            return Err(Fail);
        }
        let ents = self.entity_list(count, false)?;
        let mut relationships = self.u32()?;
        if relationships > 100_000
            && let Some(retry) = self.retry_count_after_v20_filler(self.p.saturating_sub(4), 100_000)
        {
            relationships = retry;
        }
        if relationships > 100_000 {
            return Err(Fail);
        }
        for _ in 0..relationships {
            self.object(Some(b"CRelationship"))?;
        }
        self.u16()?;
        // Anchor the GUID on the name string that follows it (SketchUp 2020 can add 2 bytes).
        if !is_marker(self.d, self.p.saturating_add(16))
            && let Some(at) = (1..=4).map(|skip| self.p + skip).find(|&at| is_marker(self.d, at + 16))
        {
            self.p = at;
        }
        self.skip(16)?;
        let name = self.utf16()?;
        self.skip_utf16()?;
        self.skip_utf16()?;
        self.u32()?;
        let thumbnail = self.class(b"CThumbnail");
        let d = self.d;
        let tpos = (0..96)
            .map(|off| self.p + off)
            .take_while(|&p| p + 26 <= d.len())
            .find(|&p| {
                (slice(d, p, 2) == Some(&[255, 255][..]) && slice(d, p + 4, 2) == Some(&[10, 0][..]) && named(d, p + 6, b"CThumbnail"))
                    || thumbnail.is_some_and(|cs| is_class_ref(d, p, cs))
            })
            .ok_or(Fail)?;
        self.skip(tpos - self.p)?;
        self.object(Some(b"CThumbnail"))?;
        Ok(V::Definition { name, ents })
    }

    /// `typed`: one attribute value, consumed.
    fn typed(&mut self, t: u8) -> Res<()> {
        match t {
            0 => Ok(()),
            4 | 9 => self.skip(4),
            6 | 12 => self.skip(8),
            7 => self.skip(1),
            10 => self.skip_utf16().map(|_| ()),
            17 | 18 => self.f64s(3).map(|_| ()),
            11 => {
                let n = self.u32()?;
                if n > 100_000 || self.depth >= MAX_OBJECT_DEPTH {
                    return Err(Fail);
                }
                self.depth += 1;
                let mut result = Ok(());
                for _ in 0..n {
                    result = self.u8().and_then(|t| self.typed(t));
                    if result.is_err() {
                        break;
                    }
                }
                self.depth -= 1;
                result
            }
            _ => Err(Fail),
        }
    }

    /// `entity_list`: `n` entities, or fewer where the C++ stops early (an over-declared root
    /// count, or a list whose count includes burned phantom entities).
    fn entity_list(&mut self, n: u32, root: bool) -> Res<Vec<(u64, Option<Rc<V>>)>> {
        self.burn_stack.push(0);
        let result = self.entity_list_inner(n, root);
        self.burn_stack.pop();
        result
    }

    fn entity_list_inner(&mut self, n: u32, root: bool) -> Res<Vec<(u64, Option<Rc<V>>)>> {
        let mut list = Vec::new();
        while list.len() < n as usize {
            let save = self.p;
            let credit = !root && self.burn_stack.last().is_some_and(|&c| c > 0);
            if credit && u32_at(self.d, save) == Some(0) && is_marker(self.d, save + 22) {
                break;
            }
            match self.object(None) {
                Ok(o) => list.push((o.slot, o.v.filter(|v| v.fills()))),
                Err(_) if root || credit => {
                    self.p = save;
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(list)
    }

    /// `definition_starts_at`.
    fn definition_starts_at(&self, at: usize) -> bool {
        (slice(self.d, at, 2) == Some(&[0xff, 0xff][..]) && named(self.d, at + 6, b"CComponentDefinition"))
            || self.class(b"CComponentDefinition").is_some_and(|cs| is_class_ref(self.d, at, cs))
    }
}

/// `read_plain_entity_ref` on a separate cursor.
fn plain_entity_ref(d: &[u8], q: &mut usize) -> Res<()> {
    let tag = u16_at(d, *q).ok_or(Fail)?;
    *q += 2;
    if tag == 0x7fff {
        let big = u32_at(d, *q).ok_or(Fail)?;
        *q += 4;
        if big & 0x8000_0000 != 0 {
            return Err(Fail);
        }
    } else if tag != 0 && (tag == 0xffff || tag & 0x8000 != 0) {
        return Err(Fail);
    }
    Ok(())
}

const PROBE_BASE: u64 = 1 << 20;

/// `bootstrap_two_materials`: with two materials, the second one's class reference names the
/// absolute slot of `CMaterial`.
fn bootstrap_two_materials(d: &[u8], ver: i32, start: usize) -> Res<u64> {
    let mut boot = Archive::new(d, ver);
    boot.next = PROBE_BASE;
    boot.base = PROBE_BASE;
    boot.p = start;
    boot.object(Some(b"CMaterial"))?;
    let tag = u16_at(d, boot.p).ok_or(Fail)?;
    if tag == 0xffff || tag & 0x8000 == 0 {
        return Err(Fail);
    }
    Ok(u64::from(tag & 0x7fff))
}

/// `probe_layer_anchor_bases`: slot-base candidates from the definition-list anchor, which
/// refers back to a layer by absolute slot.
fn probe_layer_anchor_bases(d: &[u8], ver: i32, start: usize, mat_count: u32) -> Res<Vec<u64>> {
    let mut boot = Archive::new(d, ver);
    boot.next = PROBE_BASE;
    boot.base = PROBE_BASE;
    boot.p = start;
    for _ in 0..mat_count {
        boot.object(Some(b"CMaterial"))?;
    }
    boot.u32()?;
    if ver >= 17 {
        boot.u8()?;
    }
    let mut layer_count = 0;
    if ver >= FIRST_V4 {
        layer_count = boot.u32()?;
        if !(1..=100_000).contains(&layer_count) {
            return Err(Fail);
        }
    }
    let mut layer_slots = Vec::new();
    for _ in 0..layer_count {
        layer_slots.push(boot.object(Some(b"CLayer"))?.slot);
    }
    layer_slots.extend(boot.collect_trailing_layers(true)?.into_iter().map(|(slot, _)| slot));
    let anchor = boot.object(None)?;
    if &*anchor.name != b"premodel" {
        return Err(Fail);
    }
    let s = anchor.slot as i64;
    Ok(layer_slots
        .into_iter()
        .map(|rel| s.wrapping_sub((rel as i64).wrapping_sub(PROBE_BASE as i64)))
        .filter(|&c| c > 0 && (c as u64) < PROBE_BASE)
        .map(|c| c as u64)
        .collect())
}

/// What `walk_model` returns besides the archive.
struct Walked {
    root: Vec<(u64, Option<Rc<V>>)>,
    layers: Vec<(u64, Rc<V>)>,
    materials: Vec<(u64, Option<Rc<V>>)>,
}

/// `walk_model_on`: materials, layers, definitions, then the root entities.
fn walk_model(ar: &mut Archive<'_>, start: usize, mat_count: u32, base: u64) -> Res<Walked> {
    let ver = ar.ver;
    ar.next = base;
    ar.base = base;
    ar.p = start;
    let mut materials = Vec::new();
    for _ in 0..mat_count {
        let q = ar.object(Some(b"CMaterial"))?;
        materials.push((q.slot, q.v));
    }
    ar.u32()?;
    if ver >= 17 {
        ar.u8()?;
    }
    // SketchUp 3 writes no layer count.
    let layer_count = if ver < FIRST_V4 { 1 << 30 } else { ar.u32()? };
    if layer_count > 100_000 && ver >= FIRST_V4 {
        return Err(Fail);
    }
    let mut layers = Vec::new();
    while layers.len() < layer_count as usize {
        let Some(tag) = u16_at(ar.d, ar.p) else {
            break;
        };
        if tag == 0 {
            ar.p += 2;
            continue;
        }
        if tag != 0xffff && tag & 0x8000 == 0 {
            break;
        }
        let q = ar.object(Some(b"CLayer"))?;
        if let Some(v) = q.v {
            layers.push((q.slot, v));
        }
    }
    layers.extend(ar.collect_trailing_layers(false)?);
    if &*ar.object(None)?.name != b"CLayer" {
        return Err(Fail);
    }
    let mut definitions = ar.u32()?;
    if definitions > 1_000_000
        && let Some(retry) = ar.retry_count_after_v20_filler(ar.p.saturating_sub(4), 1_000_000)
    {
        definitions = retry;
        if !ar.definition_starts_at(ar.p) {
            // A model with no definitions: the count found was the root entity count.
            definitions = 0;
            ar.p = ar.p.saturating_sub(4);
        }
    }
    if definitions > 1_000_000 {
        return Err(Fail);
    }
    for _ in 0..definitions {
        ar.object(Some(b"CComponentDefinition"))?;
    }
    let class = ar.class(b"CComponentDefinition");
    while let Some(t) = u16_at(ar.d, ar.p) {
        let mut yes = class.is_some_and(|cs| u64::from(t) == (0x8000 | cs));
        if !yes && t == 0xffff {
            yes = named(ar.d, ar.p + 6, b"CComponentDefinition");
        }
        if !yes {
            break;
        }
        ar.object(None)?;
    }
    let mut root_count = ar.u32()?;
    if root_count > 5_000_000
        && let Some(retry) = ar.retry_count_after_v20_filler(ar.p.saturating_sub(4), 5_000_000)
    {
        root_count = retry;
    }
    if root_count > 5_000_000 {
        return Err(Fail);
    }
    let root = ar.entity_list(root_count, true)?;
    Ok(Walked { root, layers, materials })
}

/// `add_edge`.
fn add_edge(b: &mut Builder, slot: u64, e: &V, slots: &HashMap<u64, Entry>) {
    let key = slot as i64;
    if b.edges.contains_key(&key) {
        return;
    }
    let (v1, v2) = e.ends();
    for id in [v1, v2] {
        if let Some(V::Vertex(xyz)) = slots.get(&id).and_then(|s| s.v.as_deref()) {
            b.vertices.insert(id as i64, *xyz);
        }
    }
    b.edges.insert(key, (Some(v1 as i64), Some(v2 as i64)));
    let draw = e.draw();
    let flags = (if draw.soft != 0 { 8 } else { 0 }) | (if draw.smooth != 0 { 16 } else { 0 }) | i32::from(e.hidden() != 0);
    if flags != 0 {
        b.edge_flags.insert(key, flags);
    }
}

/// `fill`: one entity list into a geometry builder.
fn fill(b: &mut Builder, ents: &[(u64, Option<Rc<V>>)], slots: &HashMap<u64, Entry>) {
    for (slot, v) in ents {
        let Some(v) = v else {
            continue;
        };
        let nonzero = |n: i32| (n != 0).then_some(n);
        match &**v {
            V::Edge { .. } => add_edge(b, *slot, v, slots),
            V::Face { draw, loops } => {
                let mut f = RawFace { material: nonzero(draw.mat).map(i64::from), hidden: draw.hidden != 0, ..RawFace::default() };
                for lp in loops {
                    let mut co = Vec::new();
                    for &(edge, sense) in lp.as_deref().map_or(&[][..], V::uses) {
                        if let Some(ev) = slots.get(&edge).and_then(|s| s.v.as_deref()) {
                            add_edge(b, edge, ev, slots);
                            co.push(CoEdge { edge: edge as i64, orientation: if sense { -1 } else { 1 } });
                        }
                    }
                    f.loops.push(co);
                }
                b.faces.insert(*slot as i64, f);
            }
            V::Instance { draw, def, xf, .. } | V::Image { draw, def, xf } => b.instances.push(RawInstance {
                name: v.name().to_vec(),
                ref_idx: Some(*def as i64),
                material: nonzero(draw.mat).map(i64::from),
                matrix: xf.clone(),
                layer: nonzero(draw.layer).map(|l| l as u64),
                hidden: draw.hidden != 0,
            }),
            _ => {}
        }
    }
}

/// `\{(\d+)\.` in the first 96 bytes, zero bytes dropped: the major version.
fn version(d: &[u8]) -> Res<i32> {
    let ascii: Vec<u8> = d.iter().take(96).copied().filter(|&c| c != 0).collect();
    for (i, _) in ascii.iter().enumerate().filter(|(_, c)| **c == b'{') {
        let digits = ascii.get(i + 1..).map_or(0, |r| r.iter().take_while(|c| c.is_ascii_digit()).count());
        if digits > 0 && ascii.get(i + 1 + digits) == Some(&b'.') {
            return ascii.get(i + 1..i + 1 + digits).and_then(super::stoi).ok_or(Fail);
        }
    }
    Err(Fail)
}

/// Offset of the first class declaration of `name` (`0xffff`, schema, length, name).
fn find_class_record(d: &[u8], name: &[u8]) -> Option<usize> {
    let len = u16::try_from(name.len()).ok()?;
    let end = d.len().checked_sub(6 + name.len())?;
    (0..end).find(|&i| slice(d, i, 2) == Some(&[255, 255][..]) && u16_at(d, i + 4) == Some(len) && named(d, i + 6, name))
}

/// `parse_legacy`.
pub(super) fn parse(d: &[u8]) -> Res<Parsed> {
    let ver = version(d)?;
    // The material manager: its count sits right before the first CMaterial declaration.
    // Without materials, start at the layer list before the first CLayer declaration.
    let (start, mat_count) = match find_class_record(d, b"CMaterial") {
        Some(mh) if mh >= 4 => {
            let count = u32_at(d, mh - 4).ok_or(Fail)?;
            if count > 100_000 {
                return Err(Fail);
            }
            (mh, count)
        }
        _ => {
            let lh = find_class_record(d, b"CLayer").ok_or(Fail)?;
            (lh.wrapping_sub(if ver >= 17 { 9 } else { 8 }), 0)
        }
    };
    let bases = if mat_count >= 2 { vec![bootstrap_two_materials(d, ver, start)?] } else { probe_layer_anchor_bases(d, ver, start, mat_count)? };
    // `walk_with_bases`: the first base that walks the whole file.
    let (ar, walked) = bases
        .into_iter()
        .find_map(|base| {
            let mut ar = Archive::new(d, ver);
            walk_model(&mut ar, start, mat_count, base).ok().map(|walked| (ar, walked))
        })
        .ok_or(Fail)?;
    let mut p = Parsed::default();
    // A null material (the C++ dereferenced it) is skipped.
    for (slot, v) in walked.materials {
        let Some(v) = v else {
            continue;
        };
        let index = p.materials.len();
        p.materials.push(v.rgb());
        p.materials_by_name.insert(v.name().to_vec(), index);
        p.material_id_to_name.insert(slot as i64, v.name().to_vec());
    }
    for (slot, v) in &walked.layers {
        p.layer_id_to_name.insert(*slot as i64, v.name().to_vec());
        p.add_layer(v.name(), v.hidden() != 0);
    }
    p.add_default_layer0();
    for (slot, entry) in &ar.slots {
        if let Some(v) = entry.v.as_deref()
            && !entry.cls
            && &*entry.name == b"CComponentDefinition"
        {
            let mut definition = Definition { name: v.name().to_vec(), builder: Builder::default() };
            fill(&mut definition.builder, v.ents(), &ar.slots);
            p.definitions.insert(*slot as i64, definition);
        }
    }
    fill(&mut p.root.builder, &walked.root, &ar.slots);
    Ok(p)
}
