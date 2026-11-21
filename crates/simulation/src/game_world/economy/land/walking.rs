//! Walk-cost fields shared by the offices of one player. Offices in different
//! branches divide work independently: e.g. a Steward can gather under a
//! Vassal's protection.
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use aoe_map::{CHUNK_TILES, RegionChunk};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::super::gathering::BLOCK_TILES;
mod field;
mod ground;
mod groups;
mod office;
mod patches;

use field::Field;
use groups::Groups;
use office::OfficeField;
use patches::{Claim, PatchBlocks, PatchOwners};

const LOCALITY: i32 = 64;
const NEIGHBORS: [(i32, i32); 8] = RegionChunk::STEP_OFFSETS;

fn neighbors(at: TileCoord) -> impl Iterator<Item = TileCoord> {
    NEIGHBORS
        .into_iter()
        .map(move |(dx, dy)| TileCoord::new(at.x + dx, at.y + dy))
}

fn pages(rect: TileRect, side: i32) -> impl Iterator<Item = (i32, i32)> {
    let lo = (rect.min.x.div_euclid(side), rect.min.y.div_euclid(side));
    let hi = (
        (rect.max.x - 1).div_euclid(side),
        (rect.max.y - 1).div_euclid(side),
    );
    (lo.1..=hi.1).flat_map(move |y| (lo.0..=hi.0).map(move |x| (x, y)))
}

fn page(tile: TileCoord, side: i32) -> (i32, i32) {
    (tile.x.div_euclid(side), tile.y.div_euclid(side))
}

#[derive(Debug)]
pub(in super::super) struct Walking {
    sources: Vec<(EntityId, TileRect)>,
    player: PlayerId,
    fields: Vec<OfficeField>,
    nearby: BTreeMap<(i32, i32), Vec<usize>>,
    patch_owners: Arc<PatchOwners>,
    affected: BTreeSet<EntityId>,
    ground_revision: u64,
    patch_revision: u64,
}

impl Walking {
    fn new(
        world: &GameWorld,
        player: PlayerId,
        sources: Vec<(EntityId, TileRect)>,
        previous: Option<&Arc<Self>>,
    ) -> Option<Self> {
        let cache = &world.economy.walking;
        let same_geography = previous.is_some_and(|old| old.patch_revision == cache.patch_revision);
        let mut blocks = PatchBlocks::new(world);
        let fields = sources
            .iter()
            .map(|&(id, origin)| {
                let old = previous.and_then(|old| Some(&old.fields[old.index(id)?]));
                OfficeField::refresh(world, player, origin, old, &mut blocks)
            })
            .collect::<Option<Vec<_>>>()?;
        if blocks.as_ref().is_some_and(|store| !store.available()) {
            return None;
        }
        let mut nearby = BTreeMap::<_, Vec<_>>::new();
        for (i, field) in fields.iter().enumerate() {
            let r = field.active.field.terrain.bounds;
            let expanded =
                TileRect::from_xywh(r.min.x - 1, r.min.y - 1, r.width() + 2, r.height() + 2);
            for cell in pages(expanded, LOCALITY) {
                nearby.entry(cell).or_default().push(i);
            }
        }
        let mut affected = BTreeSet::new();
        let unchanged = previous.filter(|old| {
            same_geography
                && old.sources == sources
                && fields
                    .iter()
                    .zip(&old.fields)
                    .all(|(field, prior)| Arc::ptr_eq(&field.active.field, &prior.active.field))
        });
        let patch_owners = unchanged.map_or_else(
            || {
                Arc::new(PatchOwners::new(
                    sources
                        .iter()
                        .zip(&fields)
                        .map(|(&(id, _), field)| (id, field.active.claims.as_ref())),
                    previous.map(|old| old.patch_owners.as_ref()),
                    same_geography,
                    &mut affected,
                ))
            },
            |old| old.patch_owners.clone(),
        );
        let mut walking = Self {
            sources,
            player,
            fields,
            nearby,
            patch_owners,
            affected,
            ground_revision: cache.revision,
            patch_revision: cache.patch_revision,
        };
        if let Some(old) = previous.filter(|_| same_geography) {
            walking.mark_changed_fields(old);
        } else {
            walking
                .affected
                .extend(walking.sources.iter().map(|&(id, _)| id));
        }
        Some(walking)
    }

    fn mark_changed_fields(&mut self, old: &Self) {
        let changed = |before: &Self, after: &Self| {
            before
                .sources
                .iter()
                .zip(&before.fields)
                .filter_map(|(&(id, _), field)| {
                    let field = &field.active.field;
                    after
                        .index(id)
                        .is_none_or(|i| !Arc::ptr_eq(field, &after.fields[i].active.field))
                        .then_some(field.terrain.bounds)
                })
                .collect::<Vec<_>>()
        };
        for area in changed(old, self).into_iter().chain(changed(self, old)) {
            for page in pages(area, LOCALITY) {
                for &index in self.nearby.get(&page).into_iter().flatten() {
                    let bounds = self.fields[index].active.field.terrain.bounds;
                    if area.min.x < bounds.max.x
                        && area.max.x > bounds.min.x
                        && area.min.y < bounds.max.y
                        && area.max.y > bounds.min.y
                    {
                        self.affected.insert(self.sources[index].0);
                    }
                }
            }
        }
    }

    fn index(&self, id: EntityId) -> Option<usize> {
        self.sources
            .binary_search_by_key(&id, |&(source, _)| source)
            .ok()
    }

    pub(in super::super) fn zone_changed(&self, id: EntityId) -> bool {
        self.affected.contains(&id)
    }

    fn current(&self, cache: &ZoneCache) -> bool {
        self.patch_revision == cache.patch_revision
            && (self.ground_revision == cache.revision
                || self
                    .fields
                    .iter()
                    .all(|field| field.current(cache, self.player)))
    }

    pub(in super::super) fn offices(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.sources.iter().map(|&(id, _)| id)
    }

    pub(in super::super) fn regions(&self, id: EntityId) -> Option<(TileRect, Vec<u32>)> {
        let index = self.index(id)?;
        let field = &self.fields[index].active.field;
        Some((field.terrain.bounds, field.regions(id, self)?))
    }

    fn candidates(&self, at: TileCoord) -> impl Iterator<Item = (EntityId, &Field)> {
        self.nearby
            .get(&page(at, LOCALITY))
            .into_iter()
            .flatten()
            .map(|&i| (self.sources[i].0, self.fields[i].active.field.as_ref()))
    }

    pub(in super::super) fn owns(&self, id: EntityId, at: TileCoord) -> bool {
        self.owner(at) == Some(id)
    }

    pub(in super::super) fn patch_owner(&self, at: TileCoord) -> Option<EntityId> {
        self.patch_owners.owner(at)
    }

    fn owner(&self, at: TileCoord) -> Option<EntityId> {
        self.patch_owner(at).or_else(|| {
            self.candidates(at)
                .filter_map(|(id, field)| Some((field.cost(at)?, id)))
                .min()
                .map(|(_, id)| id)
        })
    }
}

#[derive(Debug, Default)]
pub(crate) struct ZoneCache {
    revision: u64,
    patch_revision: u64,
    changed: BTreeMap<(i32, i32), GroundRevision>,
    walls: BTreeSet<TileCoord>,
    ground: std::sync::Mutex<ground::Ground>,
    patches: std::sync::Mutex<patches::PatchCatalog>,
    pub(in super::super) stewards: Groups,
    pub(in super::super) vassals: Groups,
}

#[derive(Debug, Default)]
struct GroundRevision {
    barriers: u64,
    bridges: u64,
    // An unlocked friendly gate admits its owner whether open or closed.
    // Keep the two most recent distinct owners to answer any player's lookup.
    foreign_gates: [Option<(PlayerId, u64)>; 2],
}

impl ZoneCache {
    fn latest(&self, rect: TileRect, player: PlayerId) -> u64 {
        pages(rect, CHUNK_TILES)
            .filter_map(|page| self.changed.get(&page))
            .map(|change| {
                let gates = change
                    .foreign_gates
                    .iter()
                    .flatten()
                    .find(|(owner, _)| *owner != player)
                    .map_or(0, |&(_, revision)| revision);
                change.barriers.max(change.bridges).max(gates)
            })
            .max()
            .unwrap_or(0)
    }

    fn bridge_revision(&self, rect: TileRect) -> u64 {
        pages(rect, CHUNK_TILES)
            .filter_map(|page| self.changed.get(&page))
            .map(|change| change.bridges)
            .max()
            .unwrap_or(0)
    }
}

impl GameWorld {
    pub(crate) fn management_ground_changed(&mut self, area: TileRect) {
        let cache = &mut self.economy.walking;
        cache.revision += 1;
        for page in pages(area, CHUNK_TILES) {
            cache.changed.entry(page).or_default().bridges = cache.revision;
        }
    }

    pub(in crate::game_world) fn management_barrier_changed(&mut self, area: TileRect) {
        let cache = &mut self.economy.walking;
        cache.revision += 1;
        for page in pages(area, CHUNK_TILES) {
            cache.changed.entry(page).or_default().barriers = cache.revision;
        }
    }

    pub(in crate::game_world) fn management_gate_toggled(
        &mut self,
        area: TileRect,
        owner: PlayerId,
    ) {
        let cache = &mut self.economy.walking;
        cache.revision += 1;
        for page in pages(area, CHUNK_TILES) {
            let gates = &mut cache.changed.entry(page).or_default().foreign_gates;
            if gates[0].is_some_and(|(previous, _)| previous != owner) {
                gates[1] = gates[0];
            }
            gates[0] = Some((owner, cache.revision));
        }
    }

    pub(in crate::game_world) fn management_wall_changed(&mut self, area: TileRect, blocked: bool) {
        for tile in field::tiles(area) {
            if blocked {
                self.economy.walking.walls.insert(tile);
            } else {
                self.economy.walking.walls.remove(&tile);
            }
        }
        self.management_barrier_changed(area);
    }

    pub(in crate::game_world) fn management_resources_changed(&mut self) {
        self.economy.walking.patch_revision += 1;
    }
}

#[cfg(test)]
mod tests;
