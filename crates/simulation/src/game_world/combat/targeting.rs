//! Spatial targeting shared by troops and buildings. A look reads the
//! enemies of the chunks it touches (see `contacts`); nothing is rationed
//! per tick.
use super::tick::Goal;
use crate::{BuildingKind, GameWorld, UnitKind};
use aoe_core::{
    ChunkCoord, EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId, SPATIAL_CHUNK_TILES, TileRect,
    WorldPosition,
};

pub(crate) mod contacts;
mod obstacles;
#[cfg(test)]
mod tests;

pub(super) struct TargetArea {
    pub player: PlayerId,
    pub source: Goal,
    pub min: i64,
    pub max: i64,
    pub sight: i64,
    pub actor: EntityId,
    pub ram: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum BuildingTargets {
    None,
    Tower,
    Unit,
    /// Buildings other than walls, gates and roads.
    Ordinary,
    Marching,
    RecheckObstacle,
}

#[derive(Default)]
pub(super) struct TargetProbe {
    pub target: Option<EntityId>,
    /// A sampled living enemy rules out quietness. False is not an empty proof:
    /// unseen entries of a dense bucket still require the full region check.
    pub sampled_hostile: bool,
    pub preferred_in_sight: bool,
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
    pub(super) fn bounds(&self, world: &GameWorld) -> TileRect {
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
    /// A live unit or building of another player, else `None`.
    pub(crate) fn attack_goal(&self, target: EntityId, attacker: PlayerId) -> Option<Goal> {
        if let Some(unit) = self.unit(target) {
            return (unit.hp > 0 && unit.player != attacker && !self.embarked(target))
                .then_some(Goal::Unit(unit.position, unit.kind.collision_radius()));
        }
        if let Some(animal) = self.animal(target) {
            return (animal.hp > 0).then_some(Goal::Unit(animal.position, 205));
        }
        self.economy
            .buildings
            .get(&target)
            .filter(|b| b.hp > 0 && b.player != attacker && !b.is_unstarted())
            .map(|building| Goal::Building(building.footprint))
    }

    /// Melee contact uses the fine movement cells around the attacker.
    pub(super) fn acquire_contact_target(&mut self, area: &TargetArea) -> Option<EntityId> {
        let Goal::Unit(at, _) = area.source else {
            return None;
        };
        let reach = i32::try_from(area.max).unwrap_or(i32::MAX);
        let naval = self.collisions.maximum_radii()[1] != 0;
        // Rotate on actual probes; global tick phases can hide crowded bodies.
        let round = self.combat.target_rounds.entry(area.actor).or_default();
        let salt = round.wrapping_add(u64::from(area.actor.0));
        *round = round.saturating_add(32);
        let mut best = None;
        let mut scanned = 0;
        for water in [false, true].into_iter().take(if naval { 2 } else { 1 }) {
            self.collisions.blocked(
                (at, reach),
                water,
                salt,
                &self.group_work,
                |id| {
                    scanned += 1;
                    let u = self.units[self.collisions.unit_index(id)].state;
                    let goal = Goal::Unit(u.position, u.kind.collision_radius());
                    if u.player == area.player
                        || u.hp == 0
                        || self.embarked(id)
                        || !area.accepts(goal)
                        || !self.unit_visible_to(area.player, id)
                        || (area.ram && !u.kind.is_siege())
                    {
                        return false;
                    }
                    let civilian = matches!(
                        u.kind,
                        UnitKind::Villager | UnitKind::FishingShip | UnitKind::TransportShip
                    );
                    let key = (u8::from(civilian), area.distance(goal), id);
                    best = Some(best.map_or(key, |old| std::cmp::min(old, key)));
                    false
                },
                |_| false,
            );
        }
        self.combat.stats.targeting_scanned += scanned;
        best.map(|(_, _, id)| id)
    }

    /// Units precede structures, in stable (class, distance, id) order; no
    /// sort or allocation proportional to a crowd.
    pub(super) fn acquire_target(
        &mut self,
        area: &TargetArea,
        buildings: BuildingTargets,
    ) -> TargetProbe {
        let bounds = area.bounds(self);
        let mut result = TargetProbe::default();
        let mut best = None;
        for y in bounds.min.y.div_euclid(SPATIAL_CHUNK_TILES)
            ..=(bounds.max.y - 1).div_euclid(SPATIAL_CHUNK_TILES)
        {
            for x in bounds.min.x.div_euclid(SPATIAL_CHUNK_TILES)
                ..=(bounds.max.x - 1).div_euclid(SPATIAL_CHUNK_TILES)
            {
                let chunk = ChunkCoord { x, y };
                let contacts = self.chunk_contacts(chunk, area.player);
                result.sampled_hostile |= contacts.hostile;
                // No visible enemy in the part of the chunk this look covers.
                if contacts.cells & contacts::cell_mask(chunk, bounds) == 0 {
                    continue;
                }
                for c in &contacts.visible {
                    let goal = Goal::Unit(c.position, c.radius);
                    if !bounds.contains(c.position.tile_floor())
                        || !area.accepts(goal)
                        || (area.ram && !c.siege)
                        || !self
                            .unit(c.id)
                            .is_some_and(|u| u.hp > 0 && u.player != area.player)
                    {
                        continue;
                    }
                    let key = (u8::from(c.civilian), area.distance(goal), c.id);
                    best = Some(best.map_or(key, |old| std::cmp::min(old, key)));
                }
            }
        }
        if let Some((_, _, id)) = best {
            result.target = Some(id);
            result.preferred_in_sight = true;
            return result;
        }
        // The actor continues probing for units while attacking a building.
        if buildings == BuildingTargets::None || self.economy.buildings.is_empty() {
            return result;
        }
        let obstacle_area = TargetArea {
            max: if buildings == BuildingTargets::Marching {
                area.sight
            } else {
                area.max
            },
            ..*area
        };
        let building_bounds = if buildings == BuildingTargets::Marching {
            obstacle_area.bounds(self)
        } else {
            bounds
        };
        // A few buildings at most: a list beats a tree for the repeats.
        let mut seen = Vec::new();
        let mut best = None;
        let tiles = if self.building_cells_near(building_bounds, area.player) {
            building_bounds.min.y..building_bounds.max.y
        } else {
            0..0
        };
        for y in tiles {
            for x in building_bounds.min.x..building_bounds.max.x {
                let Some(id) = self.building_at_tile(aoe_core::TileCoord::new(x, y)) else {
                    continue;
                };
                if seen.contains(&id) {
                    continue;
                }
                seen.push(id);
                let Some(b) = self
                    .building(id)
                    .filter(|b| b.player != area.player && b.hp > 0)
                else {
                    continue;
                };
                let obstacle = b.kind.is_obstacle_target();
                let goal = Goal::Building(b.footprint);
                let target_area = if obstacle { &obstacle_area } else { area };
                if b.kind == BuildingKind::Bridge
                    || (obstacle
                        && !matches!(buildings, BuildingTargets::Unit | BuildingTargets::Marching))
                    || !target_area.accepts(goal)
                    || !self.building_visible_to(area.player, b.footprint)
                {
                    continue;
                }
                let key = (
                    obstacle,
                    !obstacle && self.building_arrow_count(id) == 0,
                    area.distance(goal),
                    id,
                );
                best = Some(best.map_or(key, |old| std::cmp::min(old, key)));
            }
        }
        if let Some((false, _, _, id)) = best {
            result.target = Some(id);
            result.preferred_in_sight = true;
            return result;
        }
        let needs_sight_proof = area.min > 0 || area.max < area.sight;
        if best.is_some() || buildings == BuildingTargets::RecheckObstacle {
            let occupied = needs_sight_proof && self.preferred_target_in_sight(area);
            result.preferred_in_sight = occupied;
            if !occupied {
                result.target = best.map(|(_, _, _, id)| id);
            }
        }
        result
    }
}
