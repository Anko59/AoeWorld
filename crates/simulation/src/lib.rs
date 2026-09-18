//! Deterministic integer-only synthetic motion with a sparse spatial index.
mod game_world;
mod terrain;

use aoe_core::{CHUNK_SIZE, EntityId, PlayerId, Position, Region, Tick};
use aoe_scenario::Scenario;
pub use game_world::{Facing, GameQueryStats, GameUnit, GameWorld, GameWorldError, MovementOrder};
use std::collections::{BTreeMap, BTreeSet};
pub use terrain::UniformGrass;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Entity {
    pub id: EntityId,
    pub player: PlayerId,
    pub position: Position,
    pub velocity: Position,
}

#[derive(Debug)]
pub struct World {
    scenario: Scenario,
    tick: Tick,
    entities: Vec<Entity>,
    chunks: BTreeMap<(i32, i32), Vec<EntityId>>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct QueryStats {
    pub visited_chunks: u32,
    pub candidate_entities: u32,
}

fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

impl World {
    pub fn new(scenario: Scenario) -> Self {
        let mut state = scenario.seed.0.max(1);
        let mut entities = Vec::with_capacity(scenario.entities as usize);
        for raw in 0..scenario.entities {
            let (x, y) = if raw < scenario.hotspot_entities {
                (
                    (next_random(&mut state) % 128) as i32,
                    (next_random(&mut state) % 128) as i32,
                )
            } else {
                (
                    (next_random(&mut state) % scenario.active_extent as u64) as i32,
                    (next_random(&mut state) % scenario.active_extent as u64) as i32,
                )
            };
            let vx = (next_random(&mut state) % 3) as i32 - 1;
            let vy = (next_random(&mut state) % 3) as i32 - 1;
            entities.push(Entity {
                id: EntityId(raw),
                player: PlayerId((raw % u32::from(scenario.players)) as u16),
                position: Position { x, y },
                velocity: Position { x: vx, y: vy },
            });
        }
        let mut world = Self {
            scenario,
            tick: Tick(0),
            entities,
            chunks: BTreeMap::new(),
        };
        world.reindex();
        world
    }

    pub fn tick(&self) -> Tick {
        self.tick
    }
    pub fn scenario(&self) -> Scenario {
        self.scenario
    }
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }
    pub fn loaded_chunks(&self) -> usize {
        self.chunks.len()
    }

    fn reindex(&mut self) {
        self.chunks.clear();
        for e in &self.entities {
            self.chunks
                .entry((e.position.x / CHUNK_SIZE, e.position.y / CHUNK_SIZE))
                .or_default()
                .push(e.id);
        }
    }

    pub fn advance(&mut self) {
        let mut needs_reindex = false;
        for e in &mut self.entities {
            let limit = if e.id.0 < self.scenario.hotspot_entities {
                128
            } else {
                self.scenario.active_extent
            };
            let old_chunk = (e.position.x / CHUNK_SIZE, e.position.y / CHUNK_SIZE);
            e.position.x = (e.position.x + e.velocity.x).rem_euclid(limit);
            e.position.y = (e.position.y + e.velocity.y).rem_euclid(limit);
            let new_chunk = (e.position.x / CHUNK_SIZE, e.position.y / CHUNK_SIZE);
            if old_chunk != new_chunk {
                let removed = self.chunks.get_mut(&old_chunk).is_some_and(|ids| {
                    if let Ok(index) = ids.binary_search(&e.id) {
                        ids.remove(index);
                        true
                    } else {
                        false
                    }
                });
                if !removed {
                    needs_reindex = true;
                }
                if self.chunks.get(&old_chunk).is_some_and(Vec::is_empty) {
                    self.chunks.remove(&old_chunk);
                }
                let ids = self.chunks.entry(new_chunk).or_default();
                if let Err(index) = ids.binary_search(&e.id) {
                    ids.insert(index, e.id);
                }
            }
        }
        self.tick.0 += 1;
        if needs_reindex {
            self.reindex();
        }
    }

    pub fn query(&self, region: Region) -> Vec<Entity> {
        self.query_with_stats(region).0
    }

    pub fn query_with_stats(&self, region: Region) -> (Vec<Entity>, QueryStats) {
        if !region.valid(self.scenario.world_size) {
            return (Vec::new(), QueryStats::default());
        }
        let mut ids = BTreeSet::<EntityId>::new();
        let mut stats = QueryStats::default();
        let end_x = (region.x + i32::from(region.width) - 1) / CHUNK_SIZE;
        let end_y = (region.y + i32::from(region.height) - 1) / CHUNK_SIZE;
        for cy in region.y / CHUNK_SIZE..=end_y {
            for cx in region.x / CHUNK_SIZE..=end_x {
                stats.visited_chunks += 1;
                if let Some(chunk) = self.chunks.get(&(cx, cy)) {
                    stats.candidate_entities += chunk.len() as u32;
                    ids.extend(chunk);
                }
            }
        }
        let entities = ids
            .into_iter()
            .filter_map(|id| self.entities.get(id.0 as usize).copied())
            .filter(|e| region.contains(e.position))
            .collect();
        (entities, stats)
    }

    pub fn canonical_hash(&self) -> [u8; 32] {
        let mut hash = blake3::Hasher::new();
        hash.update(&self.tick.0.to_le_bytes());
        for e in &self.entities {
            hash.update(&e.id.0.to_le_bytes());
            hash.update(&e.player.0.to_le_bytes());
            hash.update(&e.position.x.to_le_bytes());
            hash.update(&e.position.y.to_le_bytes());
            hash.update(&e.velocity.x.to_le_bytes());
            hash.update(&e.velocity.y.to_le_bytes());
        }
        *hash.finalize().as_bytes()
    }

    pub fn canonical_hash_hex(&self) -> String {
        self.canonical_hash()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoe_scenario::{SMOKE, SPARSE_LARGE, SPARSE_SMALL, TARGET_DISTRIBUTED, TARGET_HOTSPOT};

    #[test]
    fn spatial_query_matches_brute_force() {
        let mut world = World::new(SMOKE);
        for tick in 0..80 {
            let region = Region {
                x: (tick * 37) % 800,
                y: (tick * 53) % 800,
                width: 170,
                height: 122,
            };
            let actual = world.query(region);
            let expected: Vec<_> = world
                .entities()
                .iter()
                .copied()
                .filter(|e| region.contains(e.position))
                .collect();
            assert_eq!(actual, expected);
            world.advance();
        }
    }

    #[test]
    fn replay_is_stable() {
        let mut a = World::new(SMOKE);
        let mut b = World::new(SMOKE);
        for _ in 0..20 {
            a.advance();
            b.advance();
        }
        assert_eq!(a.canonical_hash(), b.canonical_hash());
    }

    #[test]
    fn target_counts_and_sparse_chunks() {
        let distributed = World::new(TARGET_DISTRIBUTED);
        let hotspot = World::new(TARGET_HOTSPOT);
        assert_eq!(distributed.entities().len(), 128_000);
        assert_eq!(hotspot.entities().len(), 128_000);
        assert!(
            hotspot
                .query(Region {
                    x: 0,
                    y: 0,
                    width: 128,
                    height: 128
                })
                .len()
                >= 10_000
        );
        assert!(distributed.loaded_chunks() <= (TARGET_DISTRIBUTED.entities as usize));
    }

    #[test]
    fn empty_logical_map_growth_does_not_expand_active_storage_or_query_work() {
        let mut small = World::new(SPARSE_SMALL);
        let mut large = World::new(SPARSE_LARGE);
        let region = Region {
            x: 64,
            y: 64,
            width: 256,
            height: 256,
        };
        for _ in 0..20 {
            assert_eq!(small.loaded_chunks(), large.loaded_chunks());
            assert_eq!(
                small.query_with_stats(region),
                large.query_with_stats(region)
            );
            assert_eq!(small.canonical_hash(), large.canonical_hash());
            small.advance();
            large.advance();
        }
    }
}

/// Local single-unit playground simulation.
pub mod playground;
