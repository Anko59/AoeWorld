//! The two union-find levels and per-player gate closures. Every union keeps
//! the smaller root, so roots are canonical whatever the union order.
use super::{Regions, SUPER};
use aoe_core::TileCoord;
use std::collections::{BTreeMap, BTreeSet};

/// A gated edge between two regions: the gates on each side (`None` for
/// plain ground) and the regions as (chunk, label).
type Gated = (Option<u32>, Option<u32>, (u32, u16), (u32, u16), bool);
type GlobalLink = (Option<u32>, Option<u32>, u32, u32, bool);

#[derive(Debug, Default)]
pub(super) struct SuperRegions {
    count: u32,
    /// Plain links leaving the super-chunk: (own component, chunk, label).
    cross: Vec<(u16, u32, u16)>,
    gated: Vec<Gated>,
}

#[derive(Debug, Default)]
pub(super) struct Global {
    offsets: Vec<u32>,
    roots: Vec<u32>,
    /// Gated edges between different roots: (gate, gate, root, root).
    gated: Vec<GlobalLink>,
}
impl Global {
    pub(super) fn root(&self, node: u32) -> u32 {
        self.roots[node as usize]
    }
}

fn find(parent: &mut [u32], mut at: u32) -> u32 {
    while parent[at as usize] != at {
        let up = parent[parent[at as usize] as usize];
        parent[at as usize] = up;
        at = up;
    }
    at
}

fn union(parent: &mut [u32], a: u32, b: u32) {
    let (a, b) = (find(parent, a), find(parent, b));
    if a != b {
        parent[a.max(b) as usize] = a.min(b);
    }
}

fn map_find(parent: &mut BTreeMap<u32, u32>, mut at: u32) -> u32 {
    while let Some(&up) = parent.get(&at).filter(|&&up| up != at) {
        at = up;
    }
    at
}

impl Regions {
    fn super_chunks(&self, at: u32) -> Vec<u32> {
        let supers_x = (self.chunks_x + SUPER - 1) / SUPER;
        let (sx, sy) = (at as i32 % supers_x, at as i32 / supers_x);
        let mut out = Vec::new();
        for cy in sy * SUPER..((sy + 1) * SUPER).min(self.chunks_y) {
            for cx in sx * SUPER..((sx + 1) * SUPER).min(self.chunks_x) {
                out.push((cy * self.chunks_x + cx) as u32);
            }
        }
        out
    }

    /// Join the regions of one super-chunk over its inner plain links.
    pub(super) fn rebuild_super(&mut self, at: u32) {
        self.stats.super_rebuilds += 1;
        let chunks = self.super_chunks(at);
        let chunks_x = self.chunks_x;
        let local = |chunk: u32| {
            let (x, y) = (chunk as i32 % chunks_x, chunk as i32 / chunks_x);
            ((y % SUPER) * SUPER + x % SUPER) as usize
        };
        let mut base = [0_u32; (SUPER * SUPER) as usize];
        let mut total = 0_u32;
        for &chunk in &chunks {
            base[local(chunk)] = total;
            total += self.chunks[chunk as usize].gates.len() as u32;
        }
        let mut parent: Vec<u32> = (0..total).collect();
        let mut gated = Vec::new();
        let mut leaving = Vec::new();
        for &chunk in &chunks {
            let regions = &self.chunks[chunk as usize];
            let gate = |label: u16| regions.gates[usize::from(label) - 1];
            for &(a, b, both) in &regions.local {
                gated.push((gate(a), gate(b), (chunk, a), (chunk, b), both));
            }
            for &(own, other, label, both) in &regions.links {
                let theirs = self.chunks[other as usize].gates[usize::from(label) - 1];
                if gate(own) != theirs || !both {
                    gated.push((gate(own), theirs, (chunk, own), (other, label), both));
                } else if self.super_of(other) == at {
                    let own = base[local(chunk)] + u32::from(own) - 1;
                    union(&mut parent, own, base[local(other)] + u32::from(label) - 1);
                } else {
                    leaving.push((chunk, own, other, label));
                }
            }
        }
        let mut comps = vec![u32::MAX; total as usize];
        let mut component_count = 0_u32;
        for &chunk in &chunks {
            let start = base[local(chunk)];
            let count = self.chunks[chunk as usize].gates.len() as u32;
            for region in 0..count {
                let root = find(&mut parent, start + region);
                let comp = &mut comps[root as usize];
                if *comp == u32::MAX {
                    *comp = component_count;
                    component_count += 1;
                }
                self.chunks[chunk as usize].comp[region as usize] = *comp as u16;
            }
        }
        let mut cross: Vec<_> = leaving
            .into_iter()
            .map(|(chunk, own, other, label)| {
                (
                    self.chunks[chunk as usize].comp[usize::from(own) - 1],
                    other,
                    label,
                )
            })
            .collect();
        cross.sort_unstable();
        cross.dedup();
        gated.sort_unstable();
        gated.dedup();
        self.supers[at as usize] = SuperRegions {
            count: component_count,
            cross,
            gated,
        };
    }

    pub(super) fn node_of(&self, chunk: u32, label: u16) -> u32 {
        self.global.offsets[self.super_of(chunk) as usize]
            + u32::from(self.chunks[chunk as usize].comp[usize::from(label) - 1])
    }

    pub(super) fn node(&self, tile: TileCoord) -> Option<u32> {
        self.region_at(tile)
            .map(|(chunk, label)| self.node_of(chunk, label))
    }

    /// Join super-chunk components over the plain links between them.
    pub(super) fn rebuild_global(&mut self) {
        self.stats.global_rebuilds += 1;
        self.closures.clear();
        let mut offsets = Vec::with_capacity(self.supers.len());
        let mut total = 0;
        for at in &self.supers {
            offsets.push(total);
            total += at.count;
        }
        self.global.offsets = offsets;
        let mut parent: Vec<u32> = (0..total).collect();
        for at in 0..self.supers.len() {
            for &(comp, other, label) in &self.supers[at].cross {
                let theirs = self.node_of(other, label);
                union(
                    &mut parent,
                    self.global.offsets[at] + u32::from(comp),
                    theirs,
                );
            }
        }
        let roots: Vec<u32> = (0..total).map(|n| find(&mut parent, n)).collect();
        let mut gated = Vec::new();
        for at in &self.supers {
            for &(ga, gb, (ca, la), (cb, lb), both) in &at.gated {
                let a = roots[self.node_of(ca, la) as usize];
                let b = roots[self.node_of(cb, lb) as usize];
                if a != b {
                    gated.push((ga, gb, a, b, both));
                }
            }
        }
        gated.sort_unstable();
        gated.dedup();
        self.global.roots = roots;
        self.global.gated = gated;
    }

    fn admits(&self, player: u32, gate: Option<u32>) -> bool {
        player == super::ANY_PLAYER
            || gate.is_none_or(|gate| {
                self.gate_access
                    .get(&gate)
                    .is_some_and(|&(owner, open)| owner == player || open)
            })
    }

    /// The player's root of a global root: gated edges it may pass joined.
    pub(super) fn closure_root(&mut self, player: u32, root: u32) -> u32 {
        if !self.closures.contains_key(&player) {
            let mut parent = BTreeMap::new();
            for &(ga, gb, a, b, both) in &self.global.gated {
                if both && self.admits(player, ga) && self.admits(player, gb) {
                    let (a, b) = (map_find(&mut parent, a), map_find(&mut parent, b));
                    if a != b {
                        parent.insert(a.max(b), a.min(b));
                    }
                }
            }
            let flat: BTreeMap<u32, u32> = parent
                .keys()
                .copied()
                .collect::<Vec<_>>()
                .into_iter()
                .map(|node| (node, map_find(&mut parent, node)))
                .collect();
            self.closures.insert(player, flat);
        }
        self.closures[&player].get(&root).copied().unwrap_or(root)
    }

    /// Reachable roots, including directed drops. Union-find joins only
    /// reversible links; a directed edge must never certify its reverse.
    pub(super) fn reachable_roots(&mut self, player: u32, from: TileCoord) -> BTreeSet<u32> {
        let Some((chunk, label)) = self.region_at(from) else {
            return BTreeSet::new();
        };
        let start = self.global.root(self.node_of(chunk, label));
        let root = self.closure_root(player, start);
        let gate = self.chunks[chunk as usize].gates[usize::from(label) - 1];
        let mut links = BTreeMap::<u32, Vec<u32>>::new();
        // Physical collision allows a unit already in a foreign gateway to
        // leave. It may not enter a different foreign gate on the way out.
        for (ga, gb, a, b, both) in self.global.gated.clone() {
            let a_root = self.closure_root(player, a);
            let b_root = self.closure_root(player, b);
            if (self.admits(player, ga) || (a == start && ga == gate)) && self.admits(player, gb) {
                links.entry(a_root).or_default().push(b_root);
            }
            if both
                && (self.admits(player, gb) || (b == start && gb == gate))
                && self.admits(player, ga)
            {
                links.entry(b_root).or_default().push(a_root);
            }
        }
        let mut roots = BTreeSet::from([root]);
        let mut queue = vec![root];
        while let Some(at) = queue.pop() {
            for &next in links.get(&at).into_iter().flatten() {
                if roots.insert(next) {
                    queue.push(next);
                }
            }
        }
        roots
    }
}
