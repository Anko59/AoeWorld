//! An exact repair of a walking field after a few edges changed (#978): a
//! wall or gate placed or removed, or a bridge, in a 201×201 box used to walk
//! the whole box again, about 5M instructions a field. Only the tiles whose
//! shortest walk used a removed edge are solved again, from their neighbours
//! that kept theirs, and tiles that gained an edge spread from where they are.
use super::{Field, REACH};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

impl Field {
    /// The costs over `ground`, from `old_costs` over `old_ground`; `None`
    /// when an office seed changed, which only a fresh walk handles.
    pub(super) fn repair_costs(
        &self,
        old_ground: &[Option<u8>],
        old_costs: &[u16],
        ground: &[Option<u8>],
    ) -> Option<Vec<u16>> {
        let steps = self.steps();
        let mut costs = old_costs.to_vec();
        let mut suspects = BinaryHeap::new();
        let mut gained = Vec::new();
        for (slot, (&before, &after)) in old_ground.iter().zip(ground).enumerate() {
            if before == after {
                continue;
            }
            if self.seeded(slot) {
                return None;
            }
            let (before, after) = (before.unwrap_or(0), after.unwrap_or(0));
            for bit in bits(before & !after) {
                let to = (slot as i32 + steps[bit].0) as usize;
                if costs[to] <= REACH {
                    suspects.push(Reverse((costs[to], to)));
                }
            }
            if after & !before != 0 && costs[slot] <= REACH {
                gained.push(slot);
            }
        }
        // In rising cost, a tile keeps its cost while a neighbour that kept
        // its own still reaches it at that cost over a remaining edge.
        let mut lost = vec![false; costs.len()];
        let mut reset = Vec::new();
        while let Some(Reverse((cost, tile))) = suspects.pop() {
            if lost[tile] || self.seeded(tile) {
                continue;
            }
            let supported = self
                .arrivals(ground, tile)
                .any(|(from, step)| !lost[from] && costs[from].saturating_add(step) == cost);
            if supported {
                continue;
            }
            lost[tile] = true;
            reset.push(tile);
            for bit in bits(old_ground[tile].unwrap_or(0)) {
                let (offset, step) = steps[bit];
                let to = (tile as i32 + offset) as usize;
                if costs[to] == cost + step {
                    suspects.push(Reverse((costs[to], to)));
                }
            }
        }
        for &tile in &reset {
            costs[tile] = u16::MAX;
        }
        let mut wave = vec![Vec::new(); usize::from(REACH) + 1];
        for &tile in &reset {
            let best = self
                .arrivals(ground, tile)
                .map(|(from, step)| costs[from].saturating_add(step))
                .min()
                .unwrap_or(u16::MAX);
            if best <= REACH {
                costs[tile] = best;
                wave[usize::from(best)].push(tile);
            }
        }
        for tile in gained {
            if costs[tile] <= REACH {
                wave[usize::from(costs[tile])].push(tile);
            }
        }
        self.relax(ground, &mut costs, wave);
        Some(costs)
    }

    /// The tiles that step into `tile` over `ground`, with each step's cost.
    /// The office's own tiles hold a cost but never spread it.
    fn arrivals<'a>(
        &'a self,
        ground: &'a [Option<u8>],
        tile: usize,
    ) -> impl Iterator<Item = (usize, u16)> + 'a {
        let steps = self.steps();
        (0..8).filter_map(move |bit| {
            let (offset, step) = steps[bit];
            let from = usize::try_from(tile as i32 - offset).ok()?;
            let mask = (*ground.get(from)?)?;
            (mask & (1 << bit) != 0 && !self.inside(from)).then_some((from, step))
        })
    }

    /// An office tile or a seed on its border: its cost is fixed by the office.
    fn seeded(&self, slot: usize) -> bool {
        let tile = self.tile(slot);
        let origin = self.terrain.origin;
        tile.x >= origin.min.x - 1
            && tile.x <= origin.max.x
            && tile.y >= origin.min.y - 1
            && tile.y <= origin.max.y
    }

    fn inside(&self, slot: usize) -> bool {
        self.terrain.origin.contains(self.tile(slot))
    }
}

fn bits(mut mask: u8) -> impl Iterator<Item = usize> {
    std::iter::from_fn(move || {
        (mask != 0).then(|| {
            let bit = mask.trailing_zeros() as usize;
            mask &= mask - 1;
            bit
        })
    })
}
