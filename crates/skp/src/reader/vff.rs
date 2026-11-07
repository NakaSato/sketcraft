//! SketchUp 2021+ files: a ZIP whose `model.dat` is a tree of tag-length-value records
//! (`full_parse`'s VFF branch in `core.cpp`, `tlv.cpp`, `geometry.cpp`).
//!
//! The C++ copied each top-level record into a `TlvNode` tree. Here a [`Node`] is a view into
//! `model.dat` and children are parsed on the fly, so nothing is copied and memory does not
//! grow with the size of a record. A node has children when its tag is a container and its
//! body starts with a well-formed record; otherwise its body is its payload, exactly as
//! `parse_tlv_recursive` decided.
//!
//! Only the parts that change the import are read: geometry, definitions, layers, material
//! IDs and the materials' colours. Scenes, the camera, styles, dimensions, construction
//! geometry, UV transforms, units and attribute dictionaries are skipped; their C++ readers
//! never throw (or catch their own errors), so skipping them cannot change the result. The ZIP
//! size checks those readers triggered are kept ([`Zip::probe`]).

use super::bytes::{f64_at, slice, u32_at, varint, varint_all};
use super::xml::{attr, find_tag};
use super::zip::Zip;
use super::{Builder, CoEdge, Definition, MAX_TLV_DEPTH, Parsed, RawFace, RawInstance, stoi};

/// A record that `full_parse` would have failed with "Failed while processing record".
struct Bad;

type R<T> = Result<T, Bad>;

fn var(d: &[u8]) -> R<u64> {
    varint_all(d).ok_or(Bad)
}

/// `containers` in `tlv.cpp`: the tags whose bodies are parsed as nested records.
fn is_container(tag: u16) -> bool {
    matches!(
        tag,
        0xF401
            | 0xF701
            | 0xD430
            | 0xD530
            | 0xC832
            | 0x7C15
            | 0x8813
            | 0x8913
            | 0x8A13
            | 0x8B13
            | 0x8C13
            | 0x8D13
            | 0x4C1D
            | 0x6419
            | 0xF901
            | 0x7017
            | 0x7117
            | 0xD007
            | 0xC409
            | 0x9411
            | 0x9511
            | 0x0F01
            | 0x384A
            | 0xB80B
            | 0x9713
            | 0x2C4C
            | 0xAC0D
            | 0xAE0D
            | 0xF601
            | 0xF801
            | 0x983A
            | 0x993A
            | 0x8C3C
            | 0x8D3C
            | 0x9013
            | 0x401F
            | 0x9113
            | 0x9213
            | 0x6942
            | 0x6C42
            | 0x6842
    )
}

/// One record. `tag` holds the two tag bytes in file order (`"C409"` is `0xC409`).
#[derive(Clone, Copy)]
struct Node<'a> {
    tag: u16,
    body: &'a [u8],
}

/// The records in a byte range (`parse_tlv_recursive` and `parse_flat` at one level): it stops
/// at the first record whose length runs past the range.
#[derive(Clone)]
struct Records<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Records<'a> {
    fn new(d: &'a [u8]) -> Self {
        Self { d, p: 0 }
    }
}

impl<'a> Iterator for Records<'a> {
    type Item = Node<'a>;

    fn next(&mut self) -> Option<Node<'a>> {
        let rest = self.d.get(self.p..)?;
        let node = (rest.len() >= 6)
            .then(|| {
                let size = u32_at(rest, 2)? as usize;
                let body = rest.get(6..)?.get(..size)?;
                Some(Node { tag: u16::from_be_bytes([rest[0], rest[1]]), body })
            })
            .flatten();
        match node {
            Some(n) => self.p += 6 + n.body.len(),
            None => self.p = self.d.len(),
        }
        node
    }
}

impl<'a> Node<'a> {
    fn children(self) -> Records<'a> {
        Records::new(if is_container(self.tag) { self.body } else { &[] })
    }

    /// The body, or nothing when the body parsed into children (`TlvNode::payload`).
    fn payload(self) -> &'a [u8] {
        if self.children().next().is_some() { &[] } else { self.body }
    }
}

/// `find_node`: depth-first, first match.
fn find_node<'a>(nodes: Records<'a>, tag: u16) -> Option<Node<'a>> {
    for n in nodes {
        if n.tag == tag {
            return Some(n);
        }
        if let Some(found) = find_node(n.children(), tag) {
            return Some(found);
        }
    }
    None
}

/// `find_all`: every match, depth-first, including matches inside matches.
fn find_all<'a>(nodes: Records<'a>, tag: u16, out: &mut Vec<Node<'a>>) {
    for n in nodes {
        if n.tag == tag {
            out.push(n);
        }
        find_all(n.children(), tag, out);
    }
}

/// Whether container records nest deeper than [`MAX_TLV_DEPTH`]. Checked once per top-level
/// record so the recursive walks below stay within the stack.
fn too_deep(record: Records<'_>) -> bool {
    let mut stack = vec![record];
    while let Some(top) = stack.last_mut() {
        match top.next() {
            Some(n) if is_container(n.tag) => {
                if stack.len() >= MAX_TLV_DEPTH {
                    return true;
                }
                stack.push(n.children());
            }
            Some(_) => {}
            None => {
                stack.pop();
            }
        }
    }
    false
}

/// `entity_id`: the record's persistent ID, from a `DE05` child or a `DC05` child wrapping
/// one, else from the first descendant that has one.
fn entity_id(n: Node<'_>) -> R<Option<i64>> {
    for c in n.children() {
        let p = c.payload();
        if c.tag == 0xDE05 && !p.is_empty() {
            return Ok(Some(var(p)? as i64));
        }
        if c.tag == 0xDC05 && p.len() >= 6 && p[0] == 0xde && p[1] == 5 {
            let z = u32_at(p, 2).ok_or(Bad)? as usize;
            if z <= p.len() - 6 {
                return Ok(Some(varint(p, 6, z).ok_or(Bad)? as i64));
            }
        }
    }
    for c in n.children() {
        if let Some(id) = entity_id(c)? {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

/// `dc_id`: a `DC05` payload's ID.
fn dc_id(p: &[u8]) -> R<i64> {
    if p.len() >= 6 && p[0] == 0xde && p[1] == 5 {
        let z = u32_at(p, 2).ok_or(Bad)? as usize;
        if z <= p.len() - 6 {
            return Ok(varint(p, 6, z).ok_or(Bad)? as i64);
        }
    }
    Ok(var(p)? as i64)
}

/// `D007` children with tag `tag` (the entity base: material, layer, flags).
fn d007_children<'a>(e: Node<'a>) -> impl Iterator<Item = Node<'a>> {
    e.children().filter(|z| z.tag == 0xD007).flat_map(Node::children)
}

/// `collect_geometry`.
fn collect_geometry(nodes: Records<'_>, b: &mut Builder) -> R<()> {
    for e in nodes {
        match e.tag {
            0xC409 => {
                let id = entity_id(e)?;
                let position = find_node(e.children(), 0xC509).map(Node::payload).filter(|p| p.len() >= 24);
                if let (Some(id), Some(p)) = (id, position) {
                    let xyz = [f64_at(p, 0), f64_at(p, 8), f64_at(p, 16)];
                    if let [Some(x), Some(y), Some(z)] = xyz {
                        b.vertices.insert(id, [x, y, z]);
                    }
                }
            }
            0xB80B => {
                if let Some(id) = entity_id(e)? {
                    let end = |tag| -> R<Option<i64>> {
                        match find_node(e.children(), tag).map(Node::payload) {
                            Some(p) if !p.is_empty() => Ok(Some(var(p)? as i64)),
                            _ => Ok(None),
                        }
                    };
                    b.edges.insert(id, (end(0xB90B)?, end(0xBA0B)?));
                    for w in d007_children(e) {
                        if w.tag == 0xD307
                            && let Some(&flags) = w.payload().first()
                        {
                            b.edge_flags.insert(id, i32::from(flags));
                        }
                    }
                }
            }
            0xAC0D => {
                if let Some(id) = entity_id(e)? {
                    let face = read_face(e)?;
                    b.faces.insert(id, face);
                }
            }
            0x6419 => {
                let instance = read_instance(e)?;
                b.instances.push(instance);
            }
            // Construction lines and points: not imported, and never descended into.
            0x6942 | 0x6C42 => {}
            _ => collect_geometry(e.children(), b)?,
        }
    }
    Ok(())
}

/// The `AC0D` branch of `collect_geometry`.
fn read_face(e: Node<'_>) -> R<RawFace> {
    let mut f = RawFace::default();
    if let Some(l) = find_node(e.children(), 0xAE0D) {
        let mut loops = Vec::new();
        find_all(l.children(), 0x9411, &mut loops);
        for ln in loops {
            let mut coedges = Vec::new();
            find_all(ln.children(), 0xA00F, &mut coedges);
            let mut co = Vec::new();
            for cn in coedges {
                let (mut edge, mut orientation) = (None, None);
                for v in Records::new(cn.payload()) {
                    match v.tag {
                        0xA10F => edge = Some(var(v.body)? as i64),
                        0xA20F => orientation = Some(if var(v.body)? == 0 { 1 } else { -1 }),
                        _ => {}
                    }
                }
                if let (Some(edge), Some(orientation)) = (edge, orientation) {
                    co.push(CoEdge { edge, orientation });
                }
            }
            if !co.is_empty() {
                f.loops.push(co);
            }
        }
    }
    for x in d007_children(e) {
        let p = x.payload();
        match x.tag {
            0xD107 if !p.is_empty() => f.material = Some(var(p)? as i64),
            0xD307 if !p.is_empty() => f.hidden = p[0] & 1 != 0,
            0xD207 if !p.is_empty() => f.layer = Some(var(p)?),
            _ => {}
        }
    }
    // The back material is not imported, but reading it can fail the record.
    for x in e.children() {
        if x.tag == 0xAF0D && !x.payload().is_empty() {
            var(x.payload())?;
        }
    }
    Ok(f)
}

/// The `6419` branch of `collect_geometry`.
fn read_instance(e: Node<'_>) -> R<RawInstance> {
    // The placement's own ID is not imported, but reading it can fail the record.
    entity_id(e)?;
    let mut i = RawInstance::default();
    if let Some(r) = find_node(e.children(), 0x6719).map(Node::payload)
        && !r.is_empty()
    {
        i.ref_idx = Some(var(r)? as i64);
    }
    if let Some(n) = find_node(e.children(), 0x6519) {
        i.name = n.payload().to_vec();
    }
    if let Some(m) = find_node(e.children(), 0x6619).map(Node::payload)
        && m.len() >= 104
    {
        i.matrix = (0..13).filter_map(|q| f64_at(m, q * 8)).collect();
    }
    for x in d007_children(e) {
        let p = x.payload();
        match x.tag {
            0xD107 if !p.is_empty() => i.material = Some(var(p)? as i64),
            0xD207 if !p.is_empty() => i.layer = Some(var(p)?),
            0xD307 if !p.is_empty() => i.hidden = p[0] & 1 != 0,
            _ => {}
        }
    }
    Ok(i)
}

/// `collect_layers`: layer IDs to names, and each layer's hidden byte.
fn collect_layers(nodes: Records<'_>, p: &mut Parsed) -> R<()> {
    for e in nodes {
        if e.tag == 0x993A {
            for c in e.children().filter(|c| c.tag == 0x8C3C) {
                let d = find_node(c.children(), 0xDC05).map(Node::payload);
                let n = find_node(c.children(), 0x8D3C).map(Node::payload);
                if let (Some(d), Some(n)) = (d, n)
                    && !d.is_empty()
                {
                    p.layer_id_to_name.insert(dc_id(d)?, n.to_vec());
                }
                if let Some(n) = n
                    && let Some(&h) = find_node(c.children(), 0x8E3C).and_then(|h| h.payload().first())
                {
                    p.layer_hidden.insert(n.to_vec(), h == 1);
                }
            }
        }
        collect_layers(e.children(), p)?;
    }
    Ok(())
}

/// `collect_material_ids`: material IDs to names.
fn collect_material_ids(nodes: Records<'_>, p: &mut Parsed) -> R<()> {
    for e in nodes {
        if e.tag == 0xC832 {
            let d = find_node(e.children(), 0xDC05).map(Node::payload);
            let n = find_node(e.children(), 0xCC32).map(Node::payload);
            if let (Some(d), Some(n)) = (d, n)
                && !d.is_empty()
            {
                p.material_id_to_name.insert(dc_id(d)?, n.to_vec());
            }
        }
        collect_material_ids(e.children(), p)?;
    }
    Ok(())
}

/// `collect_definitions`: every `7C15` component definition, nested ones included.
fn collect_definitions(nodes: Records<'_>, p: &mut Parsed) -> R<()> {
    for e in nodes {
        if e.tag == 0x7C15 {
            let mut d = Definition::default();
            for c in e.children() {
                let payload = c.payload();
                match c.tag {
                    0x7E15 => d.name = payload.to_vec(),
                    // Image and camera flags: not imported, but reading them can fail.
                    0x8315 if !payload.is_empty() => {
                        var(payload)?;
                    }
                    0x581B => {
                        for v in Records::new(payload) {
                            if matches!(v.tag, 0x5D1B | 0x5E1B) && !v.body.is_empty() {
                                var(v.body)?;
                            }
                        }
                    }
                    _ => {}
                }
            }
            collect_geometry(e.children(), &mut d.builder)?;
            if let Some(id) = entity_id(e)? {
                p.definitions.insert(id, d);
            }
        }
        collect_definitions(e.children(), p)?;
    }
    Ok(())
}

/// `headers`: (offset, size) of each record in `d[start..end]`.
fn headers(d: &[u8], start: usize, end: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut p = start;
    while p + 6 <= end {
        let Some(n) = u32_at(d, p + 2) else {
            break;
        };
        let n = n as usize;
        if n > end - p - 6 {
            break;
        }
        out.push((p, n));
        p += 6 + n;
    }
    out
}

fn tag_at(d: &[u8], at: usize) -> u16 {
    slice(d, at, 2).map_or(0, |t| u16::from_be_bytes([t[0], t[1]]))
}

/// `unwrap_definitions_container`: `F901 > 7017 > 7117 > 7C15...`, as separate records.
fn unwrap_definitions(d: &[u8], offset: usize, size: usize) -> Vec<(usize, usize)> {
    let mut level = headers(d, offset + 6, offset + 6 + size);
    for expected in [0x7017, 0x7117] {
        let Some(&(o, s)) = level.iter().find(|&&(o, _)| tag_at(d, o) == expected) else {
            return Vec::new();
        };
        level = headers(d, o + 6, o + 6 + s);
    }
    level.retain(|&(o, _)| tag_at(d, o) == 0x7C15);
    level
}

/// `zip_offset`: the ZIP starts within the first 4 KiB.
fn zip_offset(d: &[u8]) -> Option<usize> {
    (0..d.len().min(4096)).find(|&i| d.get(i..i + 4) == Some(b"PK\x03\x04"))
}

/// A material's name and `[r, g, b]`.
type Material = (Vec<u8>, [i32; 3]);

/// One `material.xml` (`material_xml`): its name and colour. The texture lookups only matter
/// for the ZIP size checks they trigger.
fn material_xml(zip: &Zip<'_>, path: &[u8], xml: &[u8]) -> Result<Option<Material>, String> {
    let Some(a) = find_tag(xml, b"material", true) else {
        return Ok(None);
    };
    let mut name = attr(a, b"name");
    if name.is_empty() {
        name = b"unknown".to_vec();
    }
    let channel = |key: &[u8]| stoi(&attr(a, key)).unwrap_or(128);
    let colour = [channel(b"colorRed"), channel(b"colorGreen"), channel(b"colorBlue")];
    if let Some(ta) = find_tag(xml, b"texture", true) {
        let filename = attr(ta, b"textureFilename");
        let folder = path.iter().rposition(|&c| c == b'/').map_or(&[][..], |slash| &path[..slash]);
        let mut in_folder = folder.to_vec();
        in_folder.push(b'/');
        let mut found = false;
        if !filename.is_empty() {
            found = zip.probe(&[&in_folder[..], &filename].concat())?;
        }
        if !found
            && let Some(n) = zip.names().iter().find(|n| n.starts_with(&in_folder) && n.as_slice() != path && n.len() > 4 && !n.ends_with(b".xml"))
        {
            found = zip.probe(n)?;
        }
        if !found && let Some(ia) = find_tag(xml, b"image", false) {
            let path_attr = attr(ia, b"path");
            let start = path_attr.iter().take_while(|&&c| c == b'.' || c == b'/').count();
            let ip = path_attr.get(start..).unwrap_or(&[]);
            for candidate in [ip.to_vec(), [&in_folder[..], ip].concat()] {
                if !candidate.is_empty() && zip.probe(&candidate)? {
                    break;
                }
            }
        }
    }
    Ok(Some((name, colour)))
}

/// `full_parse` for a 2021+ file.
pub(super) fn parse(data: &[u8]) -> Result<Parsed, String> {
    let off = zip_offset(data).ok_or("No ZIP container found | stage=zip_extract")?;
    let zip = Zip::open(data.get(off..).unwrap_or(&[])).ok_or("Invalid ZIP container | stage=zip_extract")?;
    let mut p = Parsed::default();
    for n in zip.names() {
        if !(n.starts_with(b"materials/") && n.len() >= 12 && n.ends_with(b"material.xml")) {
            continue;
        }
        let Some(bytes) = zip.get(n)? else {
            continue;
        };
        let Some((name, colour)) = material_xml(&zip, n, &bytes)? else {
            continue;
        };
        let rest = n.get(10..).unwrap_or(&[]);
        let folder = rest.get(..rest.iter().position(|&c| c == b'/').unwrap_or(rest.len())).unwrap_or(&[]);
        let index = p.materials.len();
        p.materials.push(colour);
        p.materials_by_name.insert(name.clone(), index);
        p.materials_by_folder.insert(folder.to_vec(), index);
        if let Some(layer) = name.strip_prefix(b"Layer_") {
            p.add_layer(layer, false);
        }
    }
    // Styles are not imported; `Zip::get` on them can still refuse the file.
    for n in zip.names() {
        if n.starts_with(b"styles/") && n.len() >= 9 && n.ends_with(b"style.xml") {
            zip.probe(n)?;
        }
    }
    let model = zip.get(b"model.dat")?.ok_or("model.dat not found in ZIP container | stage=zip_extract")?;
    read_model(&model, &mut p)?;
    p.layer_id_to_name.entry(1).or_insert_with(|| b"Layer0".to_vec());
    p.add_default_layer0();
    Ok(p)
}

/// The record loop of `full_parse`.
fn read_model(model: &[u8], p: &mut Parsed) -> Result<(), String> {
    let mut hs = headers(model, 0, model.len());
    if let [(o, s)] = hs[..]
        && tag_at(model, o) == 0xF401
    {
        hs = headers(model, o + 6, o + 6 + s);
    }
    let mut expanded = Vec::with_capacity(hs.len());
    for (o, s) in hs {
        if tag_at(model, o) == 0xF901 {
            let definitions = unwrap_definitions(model, o, s);
            if !definitions.is_empty() {
                expanded.extend(definitions);
                continue;
            }
        }
        expanded.push((o, s));
    }
    let total = expanded.len();
    for (i, &(o, s)) in expanded.iter().enumerate() {
        let tag = tag_at(model, o);
        let record = slice(model, o, 6 + s).unwrap_or(&[]);
        read_record(record, tag, p)
            .map_err(|Bad| format!("Failed while processing record | stage=tlv_walk | record={i}/{total} | tag={tag:04X} | offset=0x{o:x}"))?;
    }
    Ok(())
}

fn read_record(record: &[u8], tag: u16, p: &mut Parsed) -> R<()> {
    if too_deep(Records::new(record)) {
        return Err(Bad);
    }
    collect_layers(Records::new(record), p)?;
    collect_material_ids(Records::new(record), p)?;
    collect_definitions(Records::new(record), p)?;
    if tag == 0xF601
        && let Some(root) = Records::new(record).next()
    {
        collect_geometry(root.children(), &mut p.root.builder)?;
    }
    Ok(())
}
