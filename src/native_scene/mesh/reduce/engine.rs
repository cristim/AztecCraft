//! Quadric edge-collapse engine behind `NativeMesh::reduced`; see the parent module for the rules.

use super::ReduceRefusal;
use glam::DVec3;
use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, HashMap};

#[derive(Clone, Copy, Default)]
struct Quadric {
    a: [f64; 6],
    b: DVec3,
    c: f64,
}
impl Quadric {
    fn plane(normal: DVec3, point: DVec3, weight: f64) -> Self {
        let d = -normal.dot(point);
        Self {
            a: [
                normal.x * normal.x,
                normal.x * normal.y,
                normal.x * normal.z,
                normal.y * normal.y,
                normal.y * normal.z,
                normal.z * normal.z,
            ]
            .map(|v| v * weight),
            b: normal * (d * weight),
            c: d * d * weight,
        }
    }
    fn add(&mut self, other: &Self) {
        for (x, y) in self.a.iter_mut().zip(other.a) {
            *x += y;
        }
        self.b += other.b;
        self.c += other.c;
    }
    fn error(&self, v: DVec3) -> f64 {
        let a = &self.a;
        let av = DVec3::new(
            a[0] * v.x + a[1] * v.y + a[2] * v.z,
            a[1] * v.x + a[3] * v.y + a[4] * v.z,
            a[2] * v.x + a[4] * v.y + a[5] * v.z,
        );
        (v.dot(av) + 2.0 * self.b.dot(v) + self.c).max(0.0)
    }
}

fn area_normal(p: [DVec3; 3]) -> DVec3 {
    (p[1] - p[0]).cross(p[2] - p[0])
}

/// Triangles of one polygon, or `None` when no fan keeps every triangle on the polygon's side.
pub(super) fn triangulate(corners: &[usize], points: &[DVec3]) -> Option<Vec<[usize; 3]>> {
    let n = corners.len();
    if n == 3 {
        return Some(vec![[corners[0], corners[1], corners[2]]]);
    }
    let at = |i: usize| points[corners[i % n]];
    let newell = (0..n).fold(DVec3::ZERO, |sum, i| sum + at(i).cross(at(i + 1)));
    let mut starts: Vec<usize> = (0..n).collect();
    if n == 4 {
        let short = |i: usize| (at(i) - at(i + 2)).length_squared();
        if short(1) < short(0) {
            starts.swap(0, 1);
        }
    }
    starts.into_iter().find_map(|start| {
        let fan: Vec<[usize; 3]> = (1..n - 1)
            .map(|k| {
                [
                    corners[start],
                    corners[(start + k) % n],
                    corners[(start + k + 1) % n],
                ]
            })
            .collect();
        fan.iter()
            .all(|t| area_normal(t.map(|i| points[i])).dot(newell) > 0.0)
            .then_some(fan)
    })
}

pub(super) struct Reducer {
    pub(super) pos: Vec<DVec3>,
    quadric: Vec<Quadric>,
    pub(super) tris: Vec<[usize; 3]>,
    pub(super) tri_alive: Vec<bool>,
    star: Vec<BTreeSet<usize>>,
    border: Vec<bool>,
    version: Vec<u32>,
    pub(super) live_tris: usize,
    pub(super) live_verts: usize,
    min_tris: usize,
    area_floor: f64,
}

type Candidate = Reverse<(u64, usize, usize, u32, u32)>;

impl Reducer {
    pub(super) fn new(tris: Vec<[usize; 3]>, pos: Vec<DVec3>) -> Result<Self, ReduceRefusal> {
        let mut star = vec![BTreeSet::new(); pos.len()];
        let mut halves: HashMap<[usize; 2], usize> = HashMap::new();
        for (t, tri) in tris.iter().enumerate() {
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                if a == b || halves.insert([a, b], t).is_some() {
                    return Err(ReduceRefusal::NotManifold);
                }
                star[a].insert(t);
            }
        }
        let mut border = vec![false; pos.len()];
        let mut closed = true;
        for &[a, b] in halves.keys() {
            if !halves.contains_key(&[b, a]) {
                border[a] = true;
                border[b] = true;
                closed = false;
            }
        }
        if star.iter().any(BTreeSet::is_empty) {
            return Err(ReduceRefusal::LooseGeometry);
        }
        let mut reducer = Self {
            quadric: vec![Quadric::default(); pos.len()],
            tri_alive: vec![true; tris.len()],
            live_tris: tris.len(),
            live_verts: pos.len(),
            min_tris: if closed { 4 } else { 1 },
            version: vec![0; pos.len()],
            area_floor: 0.0,
            border,
            star,
            tris,
            pos,
        };
        reducer.check_vertex_fans()?;
        reducer.seed_quadrics();
        Ok(reducer)
    }

    /// Each vertex's triangles must form one fan: connected across edges that touch the vertex.
    fn check_vertex_fans(&self) -> Result<(), ReduceRefusal> {
        for (v, star) in self.star.iter().enumerate() {
            let mut seen = BTreeSet::new();
            let mut stack = vec![*star.first().ok_or(ReduceRefusal::LooseGeometry)?];
            while let Some(t) = stack.pop() {
                if !seen.insert(t) {
                    continue;
                }
                let neighbours: Vec<usize> =
                    self.tris[t].iter().copied().filter(|w| *w != v).collect();
                for u in neighbours {
                    stack.extend(
                        star.iter()
                            .copied()
                            .filter(|o| !seen.contains(o) && self.tris[*o].contains(&u)),
                    );
                }
            }
            if seen.len() != star.len() {
                return Err(ReduceRefusal::NotManifold);
            }
        }
        Ok(())
    }

    fn seed_quadrics(&mut self) {
        let (min, max) = self.pos.iter().fold(
            (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        );
        self.area_floor = 1e-12 * (max - min).length_squared();
        for tri in &self.tris {
            let normal = area_normal(tri.map(|i| self.pos[i]));
            let length = normal.length();
            if length <= f64::MIN_POSITIVE {
                continue;
            }
            let plane = Quadric::plane(normal / length, self.pos[tri[0]], length / 2.0);
            for &v in tri {
                self.quadric[v].add(&plane);
            }
        }
    }

    fn neighbours(&self, v: usize) -> BTreeSet<usize> {
        self.star[v]
            .iter()
            .flat_map(|t| self.tris[*t])
            .filter(|w| *w != v)
            .collect()
    }

    /// Where an edge would collapse to: `(kept, removed, position)`, or `None` when it never may.
    fn plan(&self, u: usize, v: usize) -> Option<(usize, usize, DVec3)> {
        match (self.border[u], self.border[v]) {
            (true, true) => None,
            (true, false) => Some((u, v, self.pos[u])),
            (false, true) => Some((v, u, self.pos[v])),
            (false, false) => {
                let mut q = self.quadric[u];
                q.add(&self.quadric[v]);
                let mid = (self.pos[u] + self.pos[v]) / 2.0;
                [self.pos[u], self.pos[v], mid]
                    .into_iter()
                    .min_by(|a, b| q.error(*a).total_cmp(&q.error(*b)))
                    .map(|p| (u, v, p))
            }
        }
    }

    fn cost(&self, kept: usize, removed: usize, at: DVec3) -> f64 {
        let mut q = self.quadric[kept];
        q.add(&self.quadric[removed]);
        q.error(at)
    }

    fn candidate(&self, a: usize, b: usize) -> Option<Candidate> {
        let (u, v) = (a.min(b), a.max(b));
        let (kept, removed, at) = self.plan(u, v)?;
        Some(Reverse((
            self.cost(kept, removed, at).to_bits(),
            u,
            v,
            self.version[u],
            self.version[v],
        )))
    }

    fn push_edges_of(&self, v: usize, heap: &mut BinaryHeap<Candidate>) {
        for w in self.neighbours(v) {
            heap.extend(self.candidate(v, w));
        }
    }

    /// Whether collapsing `removed` into `kept` at `at` keeps the mesh a valid manifold.
    fn legal(&self, kept: usize, removed: usize, at: DVec3) -> bool {
        let shared: Vec<usize> = self.star[kept]
            .intersection(&self.star[removed])
            .copied()
            .collect();
        if shared.is_empty() {
            return false;
        }
        let apexes: BTreeSet<usize> = shared
            .iter()
            .flat_map(|t| self.tris[*t])
            .filter(|w| *w != kept && *w != removed)
            .collect();
        let common: BTreeSet<usize> = self
            .neighbours(kept)
            .intersection(&self.neighbours(removed))
            .copied()
            .collect();
        let removed_tris = shared.len();
        if common != apexes || self.live_tris - removed_tris < self.min_tris {
            return false;
        }
        let moved = self.pos[kept] != at;
        self.star[kept]
            .union(&self.star[removed])
            .filter(|t| !shared.contains(t))
            .all(|&t| {
                let before = self.tris[t].map(|i| self.pos[i]);
                let after = self.tris[t].map(|i| {
                    if i == kept || i == removed {
                        at
                    } else {
                        self.pos[i]
                    }
                });
                if !moved && !self.tris[t].contains(&removed) {
                    return true;
                }
                let (old, new) = (area_normal(before), area_normal(after));
                new.length() > self.area_floor.max(f64::MIN_POSITIVE)
                    && (old.length() <= self.area_floor || new.dot(old) > 0.0)
            })
    }

    fn collapse(&mut self, kept: usize, removed: usize, at: DVec3) {
        let star = std::mem::take(&mut self.star[removed]);
        for t in star {
            if self.tris[t].contains(&kept) {
                self.tri_alive[t] = false;
                self.live_tris -= 1;
                for v in self.tris[t] {
                    if v != removed {
                        self.star[v].remove(&t);
                    }
                }
            } else {
                for v in &mut self.tris[t] {
                    if *v == removed {
                        *v = kept;
                    }
                }
                self.star[kept].insert(t);
            }
        }
        let removed_quadric = self.quadric[removed];
        self.quadric[kept].add(&removed_quadric);
        self.pos[kept] = at;
        self.version[kept] += 1;
        self.version[removed] += 1;
        self.live_verts -= 1;
    }

    pub(super) fn run(&mut self, done: impl Fn(&Self) -> bool) -> bool {
        let mut heap = BinaryHeap::new();
        for v in 0..self.pos.len() {
            for w in self.neighbours(v).into_iter().filter(|w| *w > v) {
                heap.extend(self.candidate(v, w));
            }
        }
        while !done(self) {
            let Some(Reverse((_, u, v, version_u, version_v))) = heap.pop() else {
                return false;
            };
            if self.star[u].is_empty()
                || self.star[v].is_empty()
                || self.version[u] != version_u
                || self.version[v] != version_v
            {
                continue;
            }
            let Some((kept, removed, at)) = self.plan(u, v) else {
                continue;
            };
            if !self.legal(kept, removed, at) {
                continue;
            }
            self.collapse(kept, removed, at);
            let ring: Vec<usize> = self.neighbours(kept).into_iter().collect();
            self.push_edges_of(kept, &mut heap);
            for w in ring {
                self.push_edges_of(w, &mut heap);
            }
        }
        true
    }
}
