//! Sparse entities bucketed by area once per tick so sessions can visit their region.
use super::super::relics;
use aoe_core::{PlayerId, TileCoord, TileRect};
use aoe_simulation::{GameAnimal, GameWorld, Projectile};
use std::collections::{BTreeMap, HashMap};

/// Tiles on a side of one bucket.
const CELL: i32 = 32;

fn cell(tile: TileCoord) -> (i32, i32) {
    (tile.x.div_euclid(CELL), tile.y.div_euclid(CELL))
}

/// Entries in id order, and each bucket's entries as ascending indices.
struct Buckets<T> {
    items: Vec<T>,
    cells: HashMap<(i32, i32), Vec<u32>>,
}

impl<T> Buckets<T> {
    fn new(items: Vec<T>, tile: impl Fn(&T) -> TileCoord) -> Self {
        let mut cells = HashMap::<_, Vec<u32>>::new();
        for (at, item) in items.iter().enumerate() {
            cells.entry(cell(tile(item))).or_default().push(at as u32);
        }
        Self { items, cells }
    }

    /// Indices of every entry whose bucket meets `region`, plus `extra`, in
    /// id order. A region spanning more buckets than there are entries
    /// visits the entries instead.
    fn near(&self, region: TileRect, extra: &[u32], out: &mut Vec<u32>) {
        out.clear();
        let (min, max) = (
            cell(region.min),
            cell(TileCoord::new(region.max.x - 1, region.max.y - 1)),
        );
        let span = i64::from(max.0 - min.0 + 1) * i64::from(max.1 - min.1 + 1);
        if span >= self.items.len() as i64 {
            out.extend(0..self.items.len() as u32);
            return;
        }
        for y in min.1..=max.1 {
            for x in min.0..=max.0 {
                if let Some(bucket) = self.cells.get(&(x, y)) {
                    out.extend_from_slice(bucket);
                }
            }
        }
        out.extend_from_slice(extra);
        out.sort_unstable();
        out.dedup();
    }
}

pub(super) struct Nearby<'w> {
    animals: Buckets<&'w GameAnimal>,
    owned: BTreeMap<PlayerId, Vec<u32>>,
    projectiles: Buckets<&'w Projectile>,
    relics: Buckets<relics::Marker>,
    owned_relics: BTreeMap<PlayerId, Vec<u32>>,
}

impl<'w> Nearby<'w> {
    pub(super) fn new(world: &'w GameWorld) -> Self {
        let tick = world.tick();
        let animals = Buckets::new(world.animals().collect(), |a| a.position.tile_floor());
        let mut owned = BTreeMap::<_, Vec<u32>>::new();
        for (at, animal) in animals.items.iter().enumerate() {
            if let Some(owner) = animal.owner {
                owned.entry(owner).or_default().push(at as u32);
            }
        }
        let projectiles = Buckets::new(world.projectiles().collect(), |p| {
            p.position(tick).tile_floor()
        });
        let relics = Buckets::new(relics::markers(world).collect(), |marker| marker.tile());
        let mut owned_relics = BTreeMap::<PlayerId, Vec<u32>>::new();
        for (index, marker) in relics.items.iter().enumerate() {
            if let Some(owner) = marker.owner() {
                owned_relics.entry(owner).or_default().push(index as u32);
            }
        }
        Self {
            relics,
            owned_relics,
            animals,
            owned,
            projectiles,
        }
    }

    pub(super) fn animals<'a>(
        &'a self,
        region: TileRect,
        player: PlayerId,
        scratch: &'a mut Vec<u32>,
    ) -> impl Iterator<Item = &'w GameAnimal> + 'a {
        let owned = self.owned.get(&player).map_or(&[][..], Vec::as_slice);
        self.animals.near(region, owned, scratch);
        scratch.iter().map(|at| self.animals.items[*at as usize])
    }

    pub(super) fn relics<'a>(
        &'a self,
        region: TileRect,
        viewer: PlayerId,
        scratch: &'a mut Vec<u32>,
    ) -> impl Iterator<Item = relics::Marker> + 'a {
        let owned = self
            .owned_relics
            .get(&viewer)
            .map_or(&[][..], Vec::as_slice);
        self.relics.near(region, owned, scratch);
        scratch.iter().map(|at| self.relics.items[*at as usize])
    }

    pub(super) fn projectiles<'a>(
        &'a self,
        region: TileRect,
        scratch: &'a mut Vec<u32>,
    ) -> impl Iterator<Item = &'w Projectile> + 'a {
        self.projectiles.near(region, &[], scratch);
        scratch
            .iter()
            .map(|at| self.projectiles.items[*at as usize])
    }
}
