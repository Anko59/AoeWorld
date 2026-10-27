//! Spatial targeting with fixed work bounds, shared by troops and buildings.
use super::tick::Goal;
use crate::{BuildingKind, GameWorld, UnitKind};
use aoe_core::{
    ChunkCoord, EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId, SPATIAL_CHUNK_TILES, TileRect,
    WorldPosition,
};

pub(super) const MAX_TARGET_INSPECTIONS: u32 = 16_384;
const BUCKET_WINDOW: usize = 128;
pub(super) const MAX_QUERY_INSPECTIONS: u32 = 16 * BUCKET_WINDOW as u32;

pub(super) struct TargetArea {
    pub player: PlayerId,
    pub source: Goal,
    pub min: i64,
    pub max: i64,
    pub actor: EntityId,
    pub ram: bool,
}

impl TargetArea {
    pub(super) fn distance(&self, goal: Goal) -> i64 {
        match self.source {
            Goal::Unit(from, _) => goal.distance_squared(from),
            Goal::Building(rect) => {
                let to = goal.centre();
                let tile = FIXED_SUBUNITS_PER_TILE;
                let from = WorldPosition::new(
                    to.x.clamp(rect.min.x * tile, rect.max.x * tile),
                    to.y.clamp(rect.min.y * tile, rect.max.y * tile),
                );
                goal.distance_squared(from)
            }
        }
    }
    pub(super) fn accepts(&self, goal: Goal) -> bool {
        let d = self.distance(goal);
        d >= self.min * self.min && d <= self.max * self.max
    }
    fn bounds(&self, world: &GameWorld) -> TileRect {
        let radius = (self.max.div_euclid(i64::from(FIXED_SUBUNITS_PER_TILE)) as i32 + 1).min(32);
        let rect = match self.source {
            Goal::Unit(at, _) => TileRect::from_xywh(at.tile_floor().x, at.tile_floor().y, 1, 1),
            Goal::Building(rect) => rect,
        };
        TileRect::from_xywh(
            rect.min.x - radius,
            rect.min.y - radius,
            rect.width() + 2 * radius,
            rect.height() + 2 * radius,
        )
        .clamp(world.config.width_tiles, world.config.height_tiles)
    }
}

impl GameWorld {
    /// Units precede structures. Stable (class, distance, id) ordering within
    /// a rotating bounded sample; no sort/allocation proportional to a crowd.
    pub(super) fn acquire_target(
        &mut self,
        area: &TargetArea,
        buildings: bool,
    ) -> Option<EntityId> {
        let bounds = area.bounds(self);
        let chunks_x = (bounds.max.x - 1).div_euclid(SPATIAL_CHUNK_TILES)
            - bounds.min.x.div_euclid(SPATIAL_CHUNK_TILES)
            + 1;
        let chunks_y = (bounds.max.y - 1).div_euclid(SPATIAL_CHUNK_TILES)
            - bounds.min.y.div_euclid(SPATIAL_CHUNK_TILES)
            + 1;
        let needed = chunks_x as u32 * chunks_y as u32 * BUCKET_WINDOW as u32;
        if MAX_TARGET_INSPECTIONS.saturating_sub(self.combat.stats.targeting_scanned) < needed {
            return None;
        }
        // Advance on this actor's actual probes, not global ticks: a sparse
        // polling cadence must never alias the bucket stride and hide enemies.
        let round = self.combat.target_rounds.entry(area.actor).or_default();
        let offset = *round;
        *round = round.saturating_add(BUCKET_WINDOW as u64);
        let mut best = None;
        let mut complete = true;
        for y in bounds.min.y.div_euclid(SPATIAL_CHUNK_TILES)
            ..=(bounds.max.y - 1).div_euclid(SPATIAL_CHUNK_TILES)
        {
            for x in bounds.min.x.div_euclid(SPATIAL_CHUNK_TILES)
                ..=(bounds.max.x - 1).div_euclid(SPATIAL_CHUNK_TILES)
            {
                let Some(bucket) = self.chunks.get(&ChunkCoord { x, y }) else {
                    continue;
                };
                let count = bucket.len().min(BUCKET_WINDOW);
                complete &= offset.saturating_add(count as u64) >= bucket.len() as u64;
                if count == 0 {
                    continue;
                }
                let start =
                    (offset.wrapping_add(u64::from(area.actor.0)) % bucket.len() as u64) as usize;
                for n in 0..count {
                    self.combat.stats.targeting_scanned += 1;
                    let id = bucket[(start + n) % bucket.len()];
                    let Some(u) = self.unit(id) else {
                        continue;
                    };
                    if u.player == area.player
                        || u.hp == 0
                        || self.embarked(id)
                        || !bounds.contains(u.position.tile_floor())
                        || !area.accepts(Goal::Unit(u.position, u.kind.collision_radius()))
                        || !self.unit_visible_to(area.player, id)
                        || (area.ram
                            && !matches!(u.kind, UnitKind::Mangonel | UnitKind::BatteringRam))
                    {
                        continue;
                    }
                    let civilian = matches!(
                        u.kind,
                        UnitKind::Villager | UnitKind::FishingShip | UnitKind::TransportShip
                    );
                    let key = (
                        u8::from(civilian),
                        area.distance(Goal::Unit(u.position, u.kind.collision_radius())),
                        id,
                    );
                    best = Some(best.map_or(key, |old| std::cmp::min(old, key)));
                }
            }
        }
        if let Some((_, _, id)) = best {
            return Some(id);
        }
        // In a dense crowd, give units one complete rotation before falling
        // back to buildings. The actor continues probing while attacking one.
        if !buildings || !complete {
            return None;
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut best = None;
        for y in bounds.min.y..bounds.max.y {
            for x in bounds.min.x..bounds.max.x {
                let Some(id) = self.building_at_tile(aoe_core::TileCoord::new(x, y)) else {
                    continue;
                };
                if !seen.insert(id) {
                    continue;
                }
                let Some(b) = self
                    .building(id)
                    .filter(|b| b.player != area.player && b.hp > 0)
                else {
                    continue;
                };
                if matches!(
                    b.kind,
                    BuildingKind::PalisadeWall | BuildingKind::PalisadeGate | BuildingKind::Bridge
                ) || !area.accepts(Goal::Building(b.footprint))
                    || !self.building_visible_to(area.player, b.footprint)
                {
                    continue;
                }
                let key = (
                    u8::from(self.building_arrow_count(id) == 0),
                    area.distance(Goal::Building(b.footprint)),
                    id,
                );
                best = Some(best.map_or(key, |old| std::cmp::min(old, key)));
            }
        }
        best.map(|(_, _, id)| id)
    }
}
