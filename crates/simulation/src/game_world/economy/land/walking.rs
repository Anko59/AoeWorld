//! Walk-cost fields shared by the offices of one player. Offices in different
//! branches divide work independently: e.g. a Steward can gather under a
//! Vassal's protection.
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use aoe_map::{CHUNK_TILES, RegionChunk};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

use super::super::gathering::BLOCK_TILES;
mod field;
mod ground;
mod groups;
mod office;
mod patches;
mod shape;

use field::Field;
use groups::Groups;
use office::OfficeField;
use patches::{Claim, PatchBlocks, PatchOwners};

/// A walking-cost guide towards `origin` over `bounds`, with `player`'s walls
/// and gates and every building as barriers, read by route guides
/// (`route_scheduler/fields.rs`). Mud and later changes are not in it: a
/// descent checks each step against the live ground.
#[derive(Debug)]
pub(crate) struct RouteGuide(Field);

impl RouteGuide {
    pub(crate) fn cost(&self, tile: TileCoord) -> Option<u16> {
        self.0.cost(tile)
    }
}

impl GameWorld {
    pub(crate) fn route_guide(
        &self,
        player: PlayerId,
        origin: TileRect,
        bounds: TileRect,
    ) -> Option<RouteGuide> {
        // Buildings too: a descent into a house is a dead end.
        let blocked = &self.economy.blocked_tiles;
        let buildings = field::tiles(bounds).filter(|tile| blocked.contains(tile));
        let terrain = field::TerrainKey::new(self, player, origin, bounds).with_barriers(buildings);
        Field::new(self, terrain).map(RouteGuide)
    }
}

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
    fields: Vec<OfficeField>,
    nearby: BTreeMap<(i32, i32), Vec<usize>>,
    patch_owners: Arc<PatchOwners>,
    affected: BTreeSet<EntityId>,
    shapes: OnceLock<Box<[shape::CachedShape]>>,
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
            fields,
            nearby,
            patch_owners,
            affected,
            shapes: OnceLock::new(),
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
        walking.prepare_shapes(previous.map(Arc::as_ref));
        Some(walking)
    }

    fn mark_changed_fields(&mut self, old: &Self) {
        // Only tiles whose reach or steps differ can move an owner or a
        // region: a field rebuilt to the same costs moves no zone (#978).
        let mut areas = Vec::new();
        for (&(id, _), field) in old.sources.iter().zip(&old.fields) {
            let before = &field.active.field;
            match self.index(id).map(|i| &self.fields[i].active.field) {
                Some(after) if Arc::ptr_eq(before, after) => {}
                Some(after) => match after.changed_tiles(before) {
                    Some(area) => areas.extend(area),
                    None => areas.extend([before.terrain.bounds, after.terrain.bounds]),
                },
                None => areas.push(before.terrain.bounds),
            }
        }
        for (&(id, _), field) in self.sources.iter().zip(&self.fields) {
            if old.index(id).is_none() {
                areas.push(field.active.field.terrain.bounds);
            }
        }
        for area in areas {
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
                || self.fields.iter().all(|field| field.current(cache)))
    }

    pub(in super::super) fn offices(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.sources.iter().map(|&(id, _)| id)
    }

    pub(in super::super) fn regions(&self, id: EntityId) -> Option<(TileRect, Vec<u32>)> {
        let index = self.index(id)?;
        let field = &self.fields[index].active.field;
        Some((field.terrain.bounds, field.regions(&self.owned(index))?))
    }

    /// Whether office `index` owns each tile of its box, row by row: `owns`
    /// for every tile, with the rivals and patch blocks looked up once per
    /// box and block instead of once per tile (#978).
    fn owned(&self, index: usize) -> Vec<bool> {
        let (id, field) = (self.sources[index].0, &self.fields[index].active.field);
        let r = field.terrain.bounds;
        let expanded = TileRect::from_xywh(r.min.x - 1, r.min.y - 1, r.width() + 2, r.height() + 2);
        // A field with a cost at a tile is listed on that tile's page, so the
        // offices listed on the box's pages are every candidate of its tiles.
        let mut rivals = pages(expanded, LOCALITY)
            .flat_map(|cell| self.nearby.get(&cell).into_iter().flatten().copied())
            .filter(|&i| i != index)
            .collect::<Vec<_>>();
        rivals.sort_unstable();
        rivals.dedup();
        let rivals = rivals
            .into_iter()
            .map(|i| (self.sources[i].0, self.fields[i].active.field.as_ref()))
            .collect::<Vec<_>>();
        let mut block = (None, None);
        field::tiles(r)
            .map(|tile| {
                let key = page(tile, BLOCK_TILES);
                if block.0 != Some(key) {
                    block = (Some(key), self.patch_owners.block(tile));
                }
                if let Some(owner) = block.1.and_then(|owners| owners[patches::slot(tile)]) {
                    return owner == id;
                }
                field.cost(tile).is_some_and(|mine| {
                    rivals.iter().all(|&(rival, other)| {
                        other
                            .cost(tile)
                            .is_none_or(|theirs| (mine, id) < (theirs, rival))
                    })
                })
            })
            .collect()
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
    /// Units that stepped onto another tile this tick, while any Steward or
    /// Vassal domain exists: the only input of domain membership.
    pub(in super::super) moved: Vec<EntityId>,
}

#[derive(Debug, Default)]
struct GroundRevision {
    barriers: u64,
    bridges: u64,
}

impl ZoneCache {
    fn latest(&self, rect: TileRect) -> u64 {
        pages(rect, CHUNK_TILES)
            .filter_map(|page| self.changed.get(&page))
            .map(|change| change.barriers.max(change.bridges))
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
    /// Movement's tile change of one unit, kept only while domains exist.
    pub(crate) fn management_tile_changed(&mut self, id: EntityId) {
        if !self.economy.stewards.active.is_empty() || self.vassal_domains_exist() {
            self.economy.walking.moved.push(id);
        }
    }
    pub(crate) fn management_ground_changed(&mut self, area: TileRect) {
        let cache = &mut self.economy.walking;
        cache.revision += 1;
        for page in pages(area, CHUNK_TILES) {
            cache.changed.entry(page).or_default().bridges = cache.revision;
        }
        cache.stewards.ground_changed();
        cache.vassals.ground_changed();
    }

    pub(in crate::game_world) fn management_barrier_changed(&mut self, area: TileRect) {
        let cache = &mut self.economy.walking;
        cache.revision += 1;
        for page in pages(area, CHUNK_TILES) {
            cache.changed.entry(page).or_default().barriers = cache.revision;
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
