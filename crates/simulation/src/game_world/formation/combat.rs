use super::*;
use crate::game_world::combat::tick::Goal;

/// Ticks between an attack-move group's looks; groups are staggered by id.
const ENGAGE_CADENCE: u64 = 4;
/// Enemies one look collects; each member compares only against these.
const ENGAGE_CANDIDATES: usize = 32;
/// Spatial bucket entries one look may inspect, whatever the group's extent.
const ENGAGE_INSPECTIONS: usize = 2_048;
/// Building tiles one look may read; a wider window rotates by rows.
const ENGAGE_BUILDING_TILES: i32 = 1_024;
/// Members one look may send into a fight; the others look again next time.
const ENGAGE_PER_LOOK: usize = 8;
/// Enemies in sight a member weighs per look, nearest first.
const TARGETS_PER_MEMBER: usize = 3;

/// A member as one look sees it: id, position, sight radius in subunits and
/// its slot at the group destination.
type Looker = (EntityId, WorldPosition, i64, WorldPosition);
/// What one look found: the owner, its members and the enemies near them.
type Look = (PlayerId, Vec<Looker>, Vec<(EntityId, Goal)>);

impl GameWorld {
    /// Every `ENGAGE_CADENCE` ticks an attack-move group makes one bounded
    /// look for enemies over its whole extent. A member breaks off only for an
    /// enemy inside its own line of sight that it could fight now (a free
    /// melee spot, or reach); it attacks alone and leaves the group, and the
    /// others keep marching. Its way back to its slot travels with the attack
    /// (see `Resume`).
    pub(crate) fn engage_formations(&mut self, changed: &mut Vec<EntityId>) {
        let leaders = self
            .formations
            .iter()
            .filter(|(id, g)| {
                g.attack_move
                    && !g.settled
                    && (u64::from(id.0) + self.tick.0).is_multiple_of(ENGAGE_CADENCE)
            })
            .map(|(&id, _)| id)
            .collect::<Vec<_>>();
        for leader in leaders {
            let Some((player, lookers, enemies)) = self.formation_look(leader) else {
                continue;
            };
            let start = (self.tick.0 / ENGAGE_CADENCE) as usize % lookers.len();
            let mut engaged = 0;
            for n in 0..lookers.len() {
                if engaged == ENGAGE_PER_LOOK {
                    break;
                }
                let (id, at, sight, slot) = lookers[(start + n) % lookers.len()];
                let mut seen = enemies
                    .iter()
                    .filter_map(|&(enemy, goal)| {
                        let d = goal.distance_squared(at);
                        (d <= sight * sight).then_some((d, enemy))
                    })
                    .collect::<Vec<_>>();
                seen.sort_unstable();
                // Checked one by one: an earlier member may just have taken
                // the last free spot around the same enemy.
                let target = seen
                    .into_iter()
                    .take(TARGETS_PER_MEMBER)
                    .map(|(_, enemy)| enemy)
                    .find(|&enemy| self.attack_spot_free(id, enemy));
                if let Some(target) = target
                    && self.issue_attack(player, id, target).is_ok()
                {
                    if let Some(attacker) = self.combat.attackers.get_mut(&id) {
                        attacker.resume = Some(slot);
                    }
                    changed.push(id);
                    engaged += 1;
                }
            }
        }
        self.advance_resumes(changed);
    }

    /// The group's members with their sight, and the enemies within the
    /// members' extent widened by the longest sight, or `None` when there is
    /// nothing to fight.
    fn formation_look(&self, leader: EntityId) -> Option<Look> {
        let group = self.formations.get(&leader)?;
        let player = self.units[self.lookup[&leader]].state.player;
        let mut lookers = Vec::with_capacity(group.members.len());
        let (mut min, mut max) = ((i32::MAX, i32::MAX), (i32::MIN, i32::MIN));
        let mut widest = 0;
        for member in &group.members {
            let Some(unit) = self.unit(member.id) else {
                continue;
            };
            let Some((_, tile, los)) = self.unit_observer(&unit) else {
                continue;
            };
            min = (min.0.min(tile.x), min.1.min(tile.y));
            max = (max.0.max(tile.x), max.1.max(tile.y));
            widest = widest.max(i32::from(los));
            let slot = layout::oriented(group.destination, member.offset, group.heading);
            let sight = i64::from(los) * i64::from(FIXED_SUBUNITS_PER_TILE);
            lookers.push((member.id, unit.position, sight, slot));
        }
        if lookers.is_empty() {
            return None;
        }
        let margin = widest + 1;
        let area = TileRect::from_xywh(
            min.0 - margin,
            min.1 - margin,
            max.0 - min.0 + 2 * margin + 1,
            max.1 - min.1 + 2 * margin + 1,
        )
        .clamp(self.config.width_tiles, self.config.height_tiles);
        let mut enemies = self.formation_enemies(leader, player, area);
        if enemies.is_empty() {
            enemies = self.formation_enemy_buildings(player, area);
        }
        (!enemies.is_empty()).then_some((player, lookers, enemies))
    }

    /// Up to `ENGAGE_CANDIDATES` visible enemy units in `area`, inspecting at
    /// most `ENGAGE_INSPECTIONS` bucket entries. The window rotates per look
    /// so neither a dense allied crowd nor a large enemy army hides anyone.
    fn formation_enemies(
        &self,
        leader: EntityId,
        player: PlayerId,
        area: TileRect,
    ) -> Vec<(EntityId, Goal)> {
        let look = self.tick.0 / ENGAGE_CADENCE;
        let mut out = Vec::new();
        let mut budget = ENGAGE_INSPECTIONS;
        for y in area.min.y / SPATIAL_CHUNK_TILES..=(area.max.y - 1) / SPATIAL_CHUNK_TILES {
            for x in area.min.x / SPATIAL_CHUNK_TILES..=(area.max.x - 1) / SPATIAL_CHUNK_TILES {
                let Some(ids) = self.chunks.get(&ChunkCoord { x, y }) else {
                    continue;
                };
                let start = ((look * ENGAGE_CANDIDATES as u64 + u64::from(leader.0))
                    % ids.len() as u64) as usize;
                for n in 0..ids.len().min(128).min(budget) {
                    budget -= 1;
                    let id = ids[(start + n) % ids.len()];
                    let unit = self.units[self.lookup[&id]].state;
                    if unit.hp > 0
                        && unit.player != player
                        && area.contains(unit.position.tile_floor())
                        && !self.embarked(id)
                        && self.unit_visible_to(player, id)
                    {
                        out.push((id, Goal::Unit(unit.position, unit.kind.collision_radius())));
                        if out.len() == ENGAGE_CANDIDATES {
                            return out;
                        }
                    }
                }
            }
        }
        out
    }

    /// Visible enemy buildings in `area`, read through the sight system's
    /// tile index. A window over `ENGAGE_BUILDING_TILES` reads a rotating
    /// band of rows per look.
    fn formation_enemy_buildings(&self, player: PlayerId, area: TileRect) -> Vec<(EntityId, Goal)> {
        let (width, height) = (area.width(), area.height());
        if width <= 0 || height <= 0 {
            return Vec::new();
        }
        let rows = (ENGAGE_BUILDING_TILES / width).clamp(1, height);
        let first = ((self.tick.0 / ENGAGE_CADENCE) as i32 * rows) % height;
        let mut out = Vec::<(EntityId, Goal)>::new();
        for row in 0..rows {
            let y = area.min.y + (first + row) % height;
            for x in area.min.x..area.max.x {
                let tile = TileCoord::new(x, y);
                let Some(id) = self.building_at_tile(tile) else {
                    continue;
                };
                if out.iter().any(|&(seen, _)| seen == id) {
                    continue;
                }
                if let Some(b) = self.building(id).filter(|b| b.player != player && b.hp > 0)
                    && self.visibility(player, tile) == TileVisibility::Visible
                {
                    out.push((id, Goal::Building(b.footprint)));
                    if out.len() == ENGAGE_CANDIDATES {
                        return out;
                    }
                }
            }
        }
        out
    }
}
