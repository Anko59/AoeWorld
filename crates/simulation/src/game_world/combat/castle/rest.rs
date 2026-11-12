//! Empty healing regions wake from wounds and spatial/source events.
use crate::{GameUnit, GameWorld};
use aoe_core::{ChunkCoord, EntityId, SPATIAL_CHUNK_TILES, TileCoord, TileRect};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
struct Sleep {
    chunks: Vec<ChunkCoord>,
    #[cfg(test)]
    source: GameUnit,
}

#[derive(Debug, Default)]
pub(crate) struct Rest {
    awake: BTreeSet<EntityId>,
    parked: BTreeSet<EntityId>,
    jobs: BTreeMap<EntityId, Sleep>,
    chunks: BTreeMap<ChunkCoord, BTreeSet<EntityId>>,
}

fn chunk(tile: TileCoord) -> ChunkCoord {
    ChunkCoord {
        x: tile.x.div_euclid(SPATIAL_CHUNK_TILES),
        y: tile.y.div_euclid(SPATIAL_CHUNK_TILES),
    }
}

fn chunks(area: TileRect) -> impl Iterator<Item = ChunkCoord> {
    let first = chunk(area.min);
    let last = chunk(TileCoord::new(area.max.x - 1, area.max.y - 1));
    (first.y..=last.y).flat_map(move |y| (first.x..=last.x).map(move |x| ChunkCoord { x, y }))
}

impl Rest {
    pub(crate) fn register(&mut self, id: EntityId) {
        self.remove(id);
        self.awake.insert(id);
    }

    fn take(&mut self, id: EntityId) -> Option<Sleep> {
        let job = self.jobs.remove(&id)?;
        for chunk in &job.chunks {
            if let Some(ids) = self.chunks.get_mut(chunk) {
                ids.remove(&id);
                if ids.is_empty() {
                    self.chunks.remove(chunk);
                }
            }
        }
        Some(job)
    }

    pub(crate) fn remove(&mut self, id: EntityId) {
        self.take(id);
        self.parked.remove(&id);
        self.awake.remove(&id);
    }

    #[inline]
    pub(crate) fn wake(&mut self, id: EntityId) {
        if self.jobs.is_empty() && self.parked.is_empty() {
            return;
        }
        let region = self.take(id).is_some();
        if self.parked.remove(&id) || region {
            self.awake.insert(id);
        }
    }

    pub(crate) fn notify(&mut self, tile: TileCoord) {
        let Some(ids) = self.chunks.get(&chunk(tile)).cloned() else {
            return;
        };
        for id in ids {
            self.wake(id);
        }
    }

    pub(crate) fn wake_all(&mut self) {
        self.awake
            .extend(std::mem::take(&mut self.jobs).into_keys());
        self.awake.append(&mut self.parked);
        self.chunks.clear();
    }

    pub(crate) fn ready(&mut self, no_sleep: bool, healers: &BTreeSet<EntityId>) -> Vec<EntityId> {
        if no_sleep {
            self.wake_all();
            healers.iter().copied().collect()
        } else {
            self.awake.iter().copied().collect()
        }
    }

    pub(super) fn park(&mut self, id: EntityId) {
        self.awake.remove(&id);
        self.parked.insert(id);
    }

    pub(super) fn sleep(&mut self, source: &GameUnit, area: TileRect) {
        self.take(source.id);
        self.awake.remove(&source.id);
        let chunks: Vec<_> = chunks(area).collect();
        for chunk in &chunks {
            self.chunks.entry(*chunk).or_default().insert(source.id);
        }
        self.jobs.insert(
            source.id,
            Sleep {
                chunks,
                #[cfg(test)]
                source: *source,
            },
        );
    }
}

impl GameWorld {
    #[cfg(test)]
    pub(in crate::game_world::combat) fn assert_resting_healers(&self) {
        for (&id, job) in &self.combat.healing.jobs {
            let m = self.unit(id).expect("missed wake: healer removed");
            assert!(
                m.hp > 0
                    && !m.moving
                    && !m.planning
                    && m.attack_target.is_none()
                    && !self.embarked(id)
                    && m.position == job.source.position
                    && m.player == job.source.player,
                "missed wake: healer {id:?} source"
            );
            let tile = m.position.tile_floor();
            let radius = 4;
            let bounds = TileRect::from_xywh(
                tile.x - radius,
                tile.y - radius,
                radius * 2 + 1,
                radius * 2 + 1,
            )
            .clamp(self.config.width_tiles, self.config.height_tiles);
            let wounded = self.query(bounds).0.into_iter().any(|u| {
                let dx = i64::from(u.position.x) - i64::from(m.position.x);
                let dy = i64::from(u.position.y) - i64::from(m.position.y);
                u.id != id
                    && u.player == m.player
                    && u.hp > 0
                    && u.hp < u.max_hp
                    && !self.embarked(u.id)
                    && !u.kind.is_naval()
                    && !u.kind.is_siege()
                    && dx * dx + dy * dy
                        <= i64::from(radius * aoe_core::FIXED_SUBUNITS_PER_TILE).pow(2)
            });
            assert!(!wounded, "missed wake: healer {id:?} wound");
        }
        for &id in &self.combat.healing.parked {
            let m = self
                .unit(id)
                .expect("missed wake: healer parked after removal");
            assert!(
                self.embarked(id) || m.moving || m.planning || m.attack_target.is_some(),
                "missed wake: healer {id:?} ready again"
            );
        }
    }
}
