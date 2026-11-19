//! Fog exploration and last sightings in a save.
use super::GameWorld;
use crate::game_world::snapshot::{BuildingMemoryRecord, BuildingRecord};
use aoe_core::{PlayerId, TileCoord};

/// One player's explored 32x32-tile chunks with their bitsets.
pub type ExploredChunks = Vec<((i32, i32), [u64; 16])>;

impl GameWorld {
    pub(crate) fn explored_records(&self) -> Vec<(PlayerId, ExploredChunks)> {
        self.vision
            .players
            .iter()
            .filter(|(_, vision)| !vision.explored.is_empty())
            .map(|(player, vision)| {
                (
                    *player,
                    vision
                        .explored
                        .iter()
                        .map(|(at, bits)| (*at, *bits))
                        .collect(),
                )
            })
            .collect()
    }

    /// Marks the tiles explored again, as seen and then out of sight, so the
    /// strategic overview remembers them too. Call before any unit is placed.
    pub(crate) fn restore_explored(&mut self, records: &[(PlayerId, ExploredChunks)]) {
        let (width, height) = (self.config.width_tiles, self.config.height_tiles);
        for (player, chunks) in records {
            let vision = self.vision.players.entry(*player).or_default();
            // Bumped first, so the fog stamps below carry the new revision.
            vision.revision += 1;
            vision.reset_stamp = vision.revision;
            for ((cx, cy), bits) in chunks {
                for index in 0..1024_i32 {
                    if bits[(index / 64) as usize] & (1_u64 << (index % 64)) == 0 {
                        continue;
                    }
                    let tile = TileCoord::new(cx * 32 + index % 32, cy * 32 + index / 32);
                    if tile.x >= width || tile.y >= height {
                        continue;
                    }
                    vision.set_explored(tile);
                    vision.touch_overview(tile, true, width, height);
                    vision.touch_overview(tile, false, width, height);
                }
            }
        }
    }

    pub(crate) fn building_memory_records(&self) -> Vec<BuildingMemoryRecord> {
        self.vision
            .players
            .iter()
            .filter_map(|(&viewer, vision)| {
                let buildings: Vec<_> = vision
                    .remembered
                    .values()
                    .filter(|building| {
                        building.player != viewer
                            && (self.vision.disabled
                                || !self
                                    .vision
                                    .building_watchers
                                    .get(&building.id)
                                    .is_some_and(|watched| watched.watchers.contains(&viewer)))
                    })
                    .map(BuildingRecord::from)
                    .collect();
                (!buildings.is_empty()).then(|| BuildingMemoryRecord {
                    viewer,
                    buildings,
                    ghosts: vision.ghost_order.iter().copied().collect(),
                })
            })
            .collect()
    }

    /// Seed memory before rebuilding current sight so a newly visible site
    /// refreshes a live building or clears a ghost through the usual path.
    pub(crate) fn restore_building_memory(&mut self, records: &[BuildingMemoryRecord]) {
        for record in records {
            let vision = self.vision.players.entry(record.viewer).or_default();
            for sighting in &record.buildings {
                vision
                    .remembered
                    .insert(sighting.id, sighting.last_sighting());
                vision.changed.insert(sighting.id);
            }
            for id in &record.ghosts {
                if let Some(building) = vision.remembered.get(id) {
                    vision.haunt(*id, building.footprint);
                }
            }
        }
    }
}
