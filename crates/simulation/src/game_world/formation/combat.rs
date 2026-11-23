use super::*;
use crate::game_world::combat::tick::Goal;
use crate::game_world::unit_index::presence::{PRESENCE_CELL_TILES, cell_bit, cell_mask};

/// Ticks between an attack-move group's looks; groups are staggered by id.
const ENGAGE_CADENCE: u64 = 4;
/// Melee members one enemy unit can hold at once.
const MELEE_SPOTS: usize = 8;
/// Enemies with a melee spot left that a member weighs per look, nearest
/// first: each costs a spot check, so the work stays per member.
const TARGETS_PER_MEMBER: usize = 3;
/// Shared contacts only expand the local pursuit window around each soldier.
const GENERAL_CONTACT_RANGE: i64 = 12 * FIXED_SUBUNITS_PER_TILE as i64;

#[derive(Clone, Copy)]
struct GeneralSight {
    id: EntityId,
    position: WorldPosition,
    radius: i64,
}

/// Visible enemy units of one chunk, read once per group look and shared by
/// every member whose sight touches the chunk, with the bits of the 8-tile
/// cells (`presence::cell_bit`) that hold any.
type Contacts = BTreeMap<ChunkCoord, (u16, Vec<(EntityId, Goal)>)>;
/// Ordinary enemy buildings with a tile in one 8-tile cell.
type BuildingContacts = BTreeMap<ChunkCoord, Vec<(EntityId, Goal)>>;

impl GameWorld {
    /// Every `ENGAGE_CADENCE` ticks each attack-move group looks: every member
    /// that sees an enemy it could fight now (a free melee spot, or reach)
    /// breaks off and attacks it alone, and the others keep marching. Each
    /// chunk near the group is read once per look, and only chunks where
    /// another player is present at all. The way back to the slot travels with
    /// the attack (see `Resume`).
    #[inline(never)] // a symbol of its own for the Callgrind stage counters
    pub(crate) fn engage_formations(&mut self, changed: &mut Vec<EntityId>) {
        let leaders = self
            .formations
            .iter()
            .filter(|(id, g)| {
                g.attack_move
                    && !g.settled
                    && (u64::from(id.0) + self.tick.0)
                        .is_multiple_of(self.config.ticks(ENGAGE_CADENCE))
            })
            .map(|(&id, _)| id)
            .collect::<Vec<_>>();
        // Melee fighters already on each enemy, whoever released them: a man
        // has eight spots around him, so more would only stand around him.
        let mut fighting = BTreeMap::<EntityId, usize>::new();
        for &id in self.combat.attackers.keys().filter(|_| !leaders.is_empty()) {
            if let Some(unit) = self.unit(id)
                && Self::uses_melee_position(unit.kind)
                && let Some(target) = unit.attack_target
            {
                *fighting.entry(target).or_default() += 1;
            }
        }
        for leader in leaders {
            self.engage_group(leader, &mut fighting, changed);
        }
        self.advance_resumes(changed);
    }

    fn engage_group(
        &mut self,
        leader: EntityId,
        fighting: &mut BTreeMap<EntityId, usize>,
        changed: &mut Vec<EntityId>,
    ) {
        let Some(group) = self.formations.get(&leader) else {
            return;
        };
        let player = self.units[self.lookup[&leader]].state.player;
        let general = self
            .general_controller(leader)
            .and_then(|id| self.unit(id))
            .and_then(|unit| {
                let (_, _, sight) = self.unit_observer(&unit)?;
                Some(GeneralSight {
                    id: unit.id,
                    position: unit.position,
                    radius: i64::from(sight) * i64::from(FIXED_SUBUNITS_PER_TILE),
                })
            });
        let breach = group.breach.and_then(|id| {
            let b = self.building(id)?;
            self.building_visible_to(player, b.footprint)
                .then_some((id, Goal::Building(b.footprint)))
        });
        let lookers: Vec<_> = group
            .members
            .iter()
            .map(|m| (m.id, group.resume_at(m.goal)))
            .collect();
        let mut units = Contacts::new();
        let mut buildings = BuildingContacts::new();
        for (id, resume) in lookers {
            let Some(unit) = self.unit(id) else {
                continue;
            };
            if unit.stance == aoe_core::CombatStance::NoAttack {
                continue;
            }
            let Some((_, tile, los)) = self.unit_observer(&unit) else {
                continue;
            };
            let general = general.filter(|g| self.general_controller(id) == Some(g.id));
            let sight = i64::from(los) * i64::from(FIXED_SUBUNITS_PER_TILE);
            let reach = sight.max(if general.is_some() {
                GENERAL_CONTACT_RANGE
            } else {
                0
            });
            let sees = |goal: Goal| {
                let d = goal.distance_squared(unit.position);
                (d <= sight * sight
                    || general.is_some_and(|g| {
                        d <= GENERAL_CONTACT_RANGE.pow(2)
                            && goal.distance_squared(g.position) <= g.radius * g.radius
                    }))
                .then_some(d)
            };
            let tiles = (reach / i64::from(FIXED_SUBUNITS_PER_TILE)) as i32 + 1;
            let bounds =
                TileRect::from_xywh(tile.x - tiles, tile.y - tiles, 2 * tiles + 1, 2 * tiles + 1)
                    .clamp(self.config.width_tiles, self.config.height_tiles);
            let mut seen = Vec::new();
            if let Some((enemy, goal)) = breach
                && let Some(d) = sees(goal)
            {
                seen.push((false, d, enemy));
            }
            for chunk in look_chunks(bounds, SPATIAL_CHUNK_TILES) {
                let (cells, contacts) = units
                    .entry(chunk)
                    .or_insert_with(|| self.formation_unit_contacts(player, chunk));
                if *cells & cell_mask(chunk, bounds) == 0 {
                    continue;
                }
                seen.extend(contacts.iter().filter_map(|&(enemy, goal)| {
                    sees(goal).map(|d| (Some(enemy) != breach.map(|b| b.0), d, enemy))
                }));
            }
            if seen.is_empty() && self.enemy_buildings_near(bounds, player) {
                for cell in look_chunks(bounds, PRESENCE_CELL_TILES) {
                    let contacts = buildings
                        .entry(cell)
                        .or_insert_with(|| self.formation_building_contacts(player, cell));
                    seen.extend(
                        contacts
                            .iter()
                            .filter_map(|&(enemy, goal)| sees(goal).map(|d| (true, d, enemy))),
                    );
                }
            }
            if seen.is_empty() {
                continue;
            }
            seen.sort_unstable();
            seen.dedup();
            // Checked one by one: an earlier member may just have taken
            // the last free spot around the same enemy.
            let melee = Self::uses_melee_position(unit.kind);
            let target = seen
                .into_iter()
                .map(|(_, _, enemy)| enemy)
                // Buildings have a wider perimeter; their spots are
                // checked by `attack_spot_free` alone.
                .filter(|enemy| {
                    !melee
                        || self.unit(*enemy).is_none()
                        || fighting.get(enemy).copied().unwrap_or(0) < MELEE_SPOTS
                })
                .take(TARGETS_PER_MEMBER)
                .find(|&enemy| self.attack_spot_free(id, enemy));
            let Some(target) = target else {
                continue;
            };
            let queue = self.detached_waypoints(id);
            if self.issue_attack(player, id, target).is_err() {
                continue;
            }
            if melee {
                *fighting.entry(target).or_default() += 1;
            }
            if let Some(attacker) = self.combat.attackers.get_mut(&id) {
                attacker.resume = Some(resume);
            }
            if !queue.pending.is_empty() {
                self.move_queues.insert(id, queue);
            }
            changed.push(id);
        }
    }

    /// Every visible living enemy unit in `chunk`; none at all is read where
    /// the presence summary shows no other player.
    fn formation_unit_contacts(
        &self,
        player: PlayerId,
        chunk: ChunkCoord,
    ) -> (u16, Vec<(EntityId, Goal)>) {
        let mut out = (0, Vec::new());
        if !self.chunks.presence.others(chunk, player) {
            return out;
        }
        let Some(ids) = self.chunks.get(&chunk) else {
            return out;
        };
        for &id in ids {
            let unit = self.units[self.lookup[&id]].state;
            if unit.hp > 0
                && unit.player != player
                && !self.embarked(id)
                && self.unit_visible_to(player, id)
            {
                out.0 |= cell_bit(chunk, unit.position.tile_floor());
                out.1
                    .push((id, Goal::Unit(unit.position, unit.kind.collision_radius())));
            }
        }
        out
    }

    /// Ordinary visible enemy buildings with a tile in the 8-tile `cell`; walls, gates
    /// and roads are left to a member's own acquisition, which can rule out
    /// alternatives in its sight before it stops for one.
    fn formation_building_contacts(
        &self,
        player: PlayerId,
        cell: ChunkCoord,
    ) -> Vec<(EntityId, Goal)> {
        let size = PRESENCE_CELL_TILES;
        let rect = TileRect::from_xywh(cell.x * size, cell.y * size, size, size);
        if !self.enemy_buildings_near(rect, player) {
            return Vec::new();
        }
        let mut out = BTreeMap::new();
        for y in rect.min.y..rect.max.y {
            for x in rect.min.x..rect.max.x {
                let tile = TileCoord::new(x, y);
                let Some(id) = self.building_at_tile(tile) else {
                    continue;
                };
                if out.contains_key(&id) {
                    continue;
                }
                if let Some(b) = self
                    .building(id)
                    .filter(|b| b.player != player && b.hp > 0 && !b.kind.is_obstacle_target())
                    && self.visibility(player, tile) == TileVisibility::Visible
                {
                    out.insert(id, Goal::Building(b.footprint));
                }
            }
        }
        out.into_iter().collect()
    }
}

/// The squares of side `size` (spatial chunks or presence cells) that a tile
/// rectangle touches.
fn look_chunks(bounds: TileRect, size: i32) -> impl Iterator<Item = ChunkCoord> {
    let (x0, y0) = (bounds.min.x.div_euclid(size), bounds.min.y.div_euclid(size));
    let (x1, y1) = (
        (bounds.max.x - 1).div_euclid(size),
        (bounds.max.y - 1).div_euclid(size),
    );
    (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| ChunkCoord { x, y }))
}
