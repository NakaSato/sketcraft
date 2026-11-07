//! The topology adapter that was `Import` in `src/skp_native.cpp`, together with the parts of
//! `build_model` it relied on (material and layer resolution). It flattens component
//! placements into one [`Model`], numbering entities in the same order as the C++.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use glam::DVec3;

use super::{Definition, Parsed, parse};
use crate::{Edge, Face, Model, Object, Vertex};

/// Reads a SketchUp file. The root group is named `name`.
pub(crate) fn import(bytes: &[u8], name: &str) -> Result<Model, String> {
    let parsed = parse(bytes)?;
    Import::new(&parsed).run(name)
}

/// The adapter's entity limit.
const MAX_ENTITIES: u64 = 100_000_000;
/// The adapter's component nesting limit.
const MAX_DEPTH: u32 = 64;

struct Import<'a> {
    parsed: &'a Parsed,
    /// `SkpModel::materials_by_id`, as sRGB.
    colors: BTreeMap<i64, [u8; 3]>,
    hidden_layers: BTreeSet<&'a [u8]>,
    active: BTreeSet<i64>,
    next: u64,
    out: Output,
}

/// The model's maps as lists in ID order. IDs are allocated in increasing order, so each list is
/// sorted, and collecting it builds a packed B-tree; inserting one by one would leave the nodes
/// about half full, nearly doubling the memory of a large model.
#[derive(Default)]
struct Output {
    vertices: Vec<(u64, Vertex)>,
    edges: Vec<(u64, Edge)>,
    faces: Vec<(u64, Face)>,
    face_colors: Vec<(u64, [u8; 3])>,
    objects: Vec<(u64, Object)>,
    hidden: Vec<u64>,
}

impl Output {
    /// Reserves the counted sizes where the allocator allows; growing is the fallback.
    fn reserve(&mut self, counts: &Counts) {
        let cap = |n: u64| usize::try_from(n.min(MAX_ENTITIES)).unwrap_or(0);
        let _ = self.vertices.try_reserve_exact(cap(counts.vertices));
        let _ = self.edges.try_reserve_exact(cap(counts.edges));
        let _ = self.faces.try_reserve_exact(cap(counts.faces));
        let _ = self.objects.try_reserve_exact(cap(counts.groups));
    }

    /// One map at a time, so only one list and one map exist at once.
    fn into_model(self) -> Model {
        let Output { vertices, edges, faces, face_colors, objects, hidden } = self;
        let mut model = Model { vertices: vertices.into_iter().collect(), ..Model::default() };
        model.edges = edges.into_iter().collect();
        model.faces = faces.into_iter().collect();
        model.face_colors = face_colors.into_iter().collect();
        model.objects = objects.into_iter().collect();
        model.hidden = hidden.into_iter().collect();
        model
    }
}

impl<'a> Import<'a> {
    fn new(parsed: &'a Parsed) -> Self {
        // build_model: a material ID resolves through its name, then its folder, to a material
        // that is still listed by name (a later material with the same name replaces it).
        let listed: BTreeSet<usize> = parsed.materials_by_name.values().copied().collect();
        let colors = parsed
            .material_id_to_name
            .iter()
            .filter_map(|(&id, name)| {
                let index = parsed.materials_by_name.get(name).or_else(|| parsed.materials_by_folder.get(name))?;
                let [r, g, b] = *parsed.materials.get(*index).filter(|_| listed.contains(index))?;
                Some((id, [r as u8, g as u8, b as u8]))
            })
            .collect();
        let hidden_layers =
            parsed.layer_order.iter().filter(|name| parsed.layer_hidden.get(*name).copied().unwrap_or(false)).map(Vec::as_slice).collect();
        Self { parsed, colors, hidden_layers, active: BTreeSet::new(), next: 0, out: Output::default() }
    }

    fn allocate(&mut self) -> Result<u64, String> {
        if self.next >= MAX_ENTITIES {
            return Err("The imported model is too large (100 million entities maximum)".into());
        }
        self.next += 1;
        Ok(self.next)
    }

    /// The layer name `build_model` leaves on a face or instance: the name for a known ID,
    /// else the ID as text, or empty when there is no layer.
    fn layer_name(&self, layer: Option<u64>) -> Cow<'a, [u8]> {
        let Some(id) = layer else {
            return Cow::Borrowed(&[]);
        };
        match i64::try_from(id).ok().and_then(|id| self.parsed.layer_id_to_name.get(&id)) {
            Some(name) => Cow::Borrowed(name.as_slice()),
            None => Cow::Owned(id.to_string().into_bytes()),
        }
    }

    fn on_hidden_layer(&self, layer: Option<u64>) -> bool {
        !self.hidden_layers.is_empty() && self.hidden_layers.contains(&*self.layer_name(layer))
    }

    fn group(&mut self, name: String, entities: Vec<u64>) -> Result<u64, String> {
        let id = self.allocate()?;
        self.out.objects.push((id, Object { id, name, entities: entities.into_iter().collect() }));
        Ok(id)
    }

    /// `Import::visit`: places one definition under `transforms` (innermost first) and
    /// returns its top-level entities.
    fn visit(&mut self, def_id: i64, def: &'a Definition, transforms: &[&'a [f64]], inherited: Option<i64>, depth: u32) -> Result<Vec<u64>, String> {
        if depth > MAX_DEPTH || !self.active.insert(def_id) {
            return Err("Recursive or excessively nested SketchUp components".into());
        }
        let b = &def.builder;
        let mut entities = Vec::new();
        let mut remap = BTreeMap::new();
        for (&old, v) in &b.vertices {
            let mut p = *v;
            for &t in transforms {
                p = transform(t, p)?;
            }
            for n in &mut p {
                *n *= 0.0254;
                if !n.is_finite() || n.abs() > 1e10 {
                    return Err("Invalid SketchUp coordinates".into());
                }
            }
            let id = self.allocate()?;
            remap.insert(old, id);
            self.out.vertices.push((id, Vertex { id, position: DVec3::from_array(p) }));
        }
        let mut pairs = BTreeMap::new();
        let ends = |e: &(Option<i64>, Option<i64>)| (e.0.unwrap_or(0), e.1.unwrap_or(0));
        for (old, e) in &b.edges {
            let (v1, v2) = ends(e);
            let (Some(&a), Some(&c)) = (remap.get(&v1), remap.get(&v2)) else {
                return Err("SketchUp edge refers to a missing vertex".into());
            };
            let key = (a.min(c), a.max(c));
            if pairs.contains_key(&key) {
                continue;
            }
            let id = self.allocate()?;
            pairs.insert(key, id);
            entities.push(id);
            self.out.edges.push((id, Edge { id, a, b: c }));
            if b.edge_flags.get(old).is_some_and(|f| f & 1 != 0) {
                self.out.hidden.push(id);
            }
        }
        for f in b.faces.values() {
            let mut rings = Vec::with_capacity(f.loops.len());
            for lp in &f.loops {
                if lp.len() < 3 {
                    return Err("SketchUp face has an invalid boundary".into());
                }
                let mut ring = Vec::with_capacity(lp.len());
                for (i, ce) in lp.iter().enumerate() {
                    let ne = lp.get((i + 1) % lp.len()).unwrap_or(ce);
                    let (Some(e), Some(next)) = (b.edges.get(&ce.edge), b.edges.get(&ne.edge)) else {
                        return Err("SketchUp face refers to a missing edge".into());
                    };
                    let (e, next) = (ends(e), ends(next));
                    let (start, end) = if ce.orientation >= 0 { e } else { (e.1, e.0) };
                    if end != (if ne.orientation >= 0 { next.0 } else { next.1 }) {
                        return Err("SketchUp face has a disconnected boundary".into());
                    }
                    // Every edge of the definition passed the vertex check above.
                    let Some(&start) = remap.get(&start) else {
                        return Err("SketchUp edge refers to a missing vertex".into());
                    };
                    ring.push(start);
                }
                rings.push(ring);
            }
            let mut rings = rings.into_iter();
            let Some(outer) = rings.next() else {
                return Err("SketchUp face has an invalid boundary".into());
            };
            let id = self.allocate()?;
            entities.push(id);
            self.out.faces.push((id, Face { id, vertices: outer, holes: rings.collect() }));
            if f.hidden || self.on_hidden_layer(f.layer) {
                self.out.hidden.push(id);
            }
            if let Some(color) = f.material.or(inherited).and_then(|m| self.colors.get(&m)) {
                self.out.face_colors.push((id, *color));
            }
        }
        for instance in &b.instances {
            let parsed = self.parsed;
            let Some((child_id, child)) = instance.ref_idx.and_then(|r| Some((r, parsed.definitions.get(&r)?))) else {
                return Err("SketchUp instance refers to a missing definition".into());
            };
            let mut chain = Vec::with_capacity(transforms.len() + 1);
            chain.push(instance.matrix.as_slice());
            chain.extend_from_slice(transforms);
            let children = self.visit(child_id, child, &chain, instance.material.or(inherited), depth + 1)?;
            if children.is_empty() {
                continue;
            }
            let name = [&instance.name, &child.name]
                .into_iter()
                .find(|n| !n.is_empty())
                .map_or_else(|| "SketchUp group".to_owned(), |n| String::from_utf8_lossy(n).into_owned());
            let id = self.group(name, children)?;
            entities.push(id);
            if instance.hidden || self.on_hidden_layer(instance.layer) {
                self.out.hidden.push(id);
            }
        }
        self.active.remove(&def_id);
        Ok(entities)
    }

    /// `Import::run`, with `decode_model`'s renaming of the root group.
    fn run(mut self, name: &str) -> Result<Model, String> {
        let parsed = self.parsed;
        self.out.reserve(&Counts::of(parsed));
        let entities = self.visit(0, &parsed.root, &[], None, 0)?;
        if entities.is_empty() {
            return Err("This SketchUp file contains no importable edges or faces".into());
        }
        self.group(name.to_owned(), entities)?;
        Ok(self.out.into_model())
    }
}

/// Upper bounds on what the import of a file produces (edges are counted before merging
/// duplicates), computed once per definition.
#[derive(Clone, Copy, Default)]
struct Counts {
    vertices: u64,
    edges: u64,
    faces: u64,
    groups: u64,
}

impl Counts {
    fn of(parsed: &Parsed) -> Counts {
        let mut memo = BTreeMap::new();
        Self::definition(parsed, &parsed.root, 0, &mut memo)
    }

    fn definition(parsed: &Parsed, def: &Definition, depth: u32, memo: &mut BTreeMap<i64, Counts>) -> Counts {
        let b = &def.builder;
        let mut counts = Counts { vertices: b.vertices.len() as u64, edges: b.edges.len() as u64, faces: b.faces.len() as u64, groups: 0 };
        // Deeper nesting or a cycle fails the import anyway.
        if depth > MAX_DEPTH {
            return counts;
        }
        for instance in &b.instances {
            let Some((id, child)) = instance.ref_idx.and_then(|r| Some((r, parsed.definitions.get(&r)?))) else {
                continue;
            };
            let child = match memo.get(&id) {
                Some(child) => *child,
                None => {
                    let child = Self::definition(parsed, child, depth + 1, memo);
                    memo.insert(id, child);
                    child
                }
            };
            counts.vertices = counts.vertices.saturating_add(child.vertices);
            counts.edges = counts.edges.saturating_add(child.edges);
            counts.faces = counts.faces.saturating_add(child.faces);
            counts.groups = counts.groups.saturating_add(child.groups).saturating_add(1);
        }
        counts
    }
}

/// One placement transform: empty (none), 12 or 13 numbers (row-major 3x3 then the
/// translation, `transform_point`), or 16 (a column-major 4x4).
fn transform(m: &[f64], [x, y, z]: [f64; 3]) -> Result<[f64; 3], String> {
    match *m {
        [] => Ok([x, y, z]),
        [m0, m1, m2, m3, m4, m5, m6, m7, m8, m9, m10, m11, ..] if m.len() == 12 || m.len() == 13 => {
            Ok([m0 * x + m1 * y + m2 * z + m9, m3 * x + m4 * y + m5 * z + m10, m6 * x + m7 * y + m8 * z + m11])
        }
        [m0, m1, m2, _, m4, m5, m6, _, m8, m9, m10, _, m12, m13, m14, _] => {
            Ok([m0 * x + m4 * y + m8 * z + m12, m1 * x + m5 * y + m9 * z + m13, m2 * x + m6 * y + m10 * z + m14])
        }
        _ => Err("Unsupported SketchUp instance transform".into()),
    }
}
