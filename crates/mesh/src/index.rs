//! A spatial index of boxes, to find the edges and faces near a point or a segment without
//! looking at the whole model.
//!
//! Grids at sizes 1 mm, 2 mm, 4 mm, … hold each box in the grid whose cells are as big as the
//! box, so it lands in at most 2 × 2 × 2 cells whether it is a millimetre or a kilometre long.
//! A query looks in each grid that holds something; a grid whose cells the query would cover by
//! the thousand is read whole instead.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;

use sketcore_record::Vec3;
use sketcraft_geom::prelude::*;

use crate::TOLERANCE;

/// The smallest cells, in mm.
const BASE: f64 = 1.0;
/// The most cells one query reads in one grid before it reads the grid's whole list.
const MAX_CELLS: i64 = 4096;

type Cell = (u8, i64, i64, i64);

#[derive(Clone, Debug)]
pub(crate) struct Index<K: Copy + Eq + Hash> {
    cells: HashMap<Cell, Vec<K>>,
    /// Where each key is: its cells.
    of: HashMap<K, Vec<Cell>>,
    /// Every key in each grid.
    levels: BTreeMap<u8, HashSet<K>>,
}

impl<K: Copy + Eq + Hash> Default for Index<K> {
    fn default() -> Self {
        Index { cells: HashMap::new(), of: HashMap::new(), levels: BTreeMap::new() }
    }
}

fn size(level: u8) -> f64 {
    BASE * 2f64.powi(i32::from(level))
}

fn level_for(lo: Vec3, hi: Vec3) -> u8 {
    let extent = (hi - lo).len() + 2.0 * TOLERANCE;
    if extent <= BASE {
        return 0;
    }
    // Coordinates stay under 1e9 mm, so this is at most about 31.
    (extent / BASE).log2().ceil().clamp(0.0, 60.0) as u8
}

fn range(lo: f64, hi: f64, s: f64) -> (i64, i64) {
    ((lo / s).floor() as i64, (hi / s).floor() as i64)
}

impl<K: Copy + Eq + Hash> Index<K> {
    /// Put `k` in the index with the box from `lo` to `hi` (any previous place goes).
    pub fn insert(&mut self, k: K, lo: Vec3, hi: Vec3) {
        self.remove(k);
        let (lo, hi) = (lo - Vec3::new(TOLERANCE, TOLERANCE, TOLERANCE), hi + Vec3::new(TOLERANCE, TOLERANCE, TOLERANCE));
        let level = level_for(lo, hi);
        let s = size(level);
        let (x, y, z) = (range(lo.x, hi.x, s), range(lo.y, hi.y, s), range(lo.z, hi.z, s));
        let mut at = Vec::new();
        for i in x.0..=x.1 {
            for j in y.0..=y.1 {
                for k2 in z.0..=z.1 {
                    let c = (level, i, j, k2);
                    self.cells.entry(c).or_default().push(k);
                    at.push(c);
                }
            }
        }
        self.of.insert(k, at);
        self.levels.entry(level).or_default().insert(k);
    }

    pub fn remove(&mut self, k: K) {
        let Some(at) = self.of.remove(&k) else { return };
        for c in &at {
            if let Some(list) = self.cells.get_mut(c) {
                list.retain(|x| *x != k);
                if list.is_empty() {
                    self.cells.remove(c);
                }
            }
        }
        if let Some(level) = at.first().map(|c| c.0)
            && let Some(set) = self.levels.get_mut(&level)
        {
            set.remove(&k);
            if set.is_empty() {
                self.levels.remove(&level);
            }
        }
    }

    /// The keys whose boxes may meet the box from `lo` to `hi` (each once; check them exactly).
    pub fn query(&self, lo: Vec3, hi: Vec3) -> Vec<K> {
        let (lo, hi) = (lo - Vec3::new(TOLERANCE, TOLERANCE, TOLERANCE), hi + Vec3::new(TOLERANCE, TOLERANCE, TOLERANCE));
        let mut seen: HashSet<K> = HashSet::new();
        let mut out = Vec::new();
        for (level, members) in &self.levels {
            let s = size(*level);
            // Boxes that meet share a cell: the one holding a point they share.
            let (x, y, z) = (range(lo.x, hi.x, s), range(lo.y, hi.y, s), range(lo.z, hi.z, s));
            let count = (x.1 - x.0 + 1).saturating_mul(y.1 - y.0 + 1).saturating_mul(z.1 - z.0 + 1);
            if count > MAX_CELLS || count > members.len() as i64 {
                for k in members {
                    if seen.insert(*k) {
                        out.push(*k);
                    }
                }
                continue;
            }
            for i in x.0..=x.1 {
                for j in y.0..=y.1 {
                    for k2 in z.0..=z.1 {
                        for k in self.cells.get(&(*level, i, j, k2)).into_iter().flatten() {
                            if seen.insert(*k) {
                                out.push(*k);
                            }
                        }
                    }
                }
            }
        }
        out
    }
}

/// The box around some points.
pub(crate) fn bounds(points: impl IntoIterator<Item = Vec3>) -> Option<(Vec3, Vec3)> {
    let mut it = points.into_iter();
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
}
