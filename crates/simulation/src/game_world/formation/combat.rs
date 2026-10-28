use super::*;

/// Members probed per attack-move group per tick; each is looked at every
/// `members / 8` ticks, whatever the army's size.
const ENGAGE_PROBES_PER_TICK: usize = 8;

impl GameWorld {
    /// Members of an attack-move group look for enemies one by one, a bounded
    /// window per tick. A member that finds one attacks it and leaves the
    /// group; the others keep marching. Its way back to its slot at the group
    /// destination travels with the attack (see `Resume`).
    pub(crate) fn engage_formations(&mut self, changed: &mut Vec<EntityId>) {
        let leaders = self
            .formations
            .iter()
            .filter(|(_, g)| g.attack_move && !g.settled)
            .map(|(&id, _)| id)
            .collect::<Vec<_>>();
        for leader in leaders {
            let group = &self.formations[&leader];
            let player = self.units[self.lookup[&leader]].state.player;
            let count = group.members.len();
            let mut found = Vec::new();
            for n in 0..count.min(ENGAGE_PROBES_PER_TICK) {
                let member =
                    &group.members[(self.tick.0 as usize * ENGAGE_PROBES_PER_TICK + n) % count];
                let Some(unit) = self.unit(member.id) else {
                    continue;
                };
                let tile = unit.position.tile_floor();
                let area = TileRect::from_xywh(tile.x - 6, tile.y - 6, 13, 13)
                    .clamp(self.config.width_tiles, self.config.height_tiles);
                if let Some(target) = self
                    .formation_enemy(member.id, player, area)
                    .or_else(|| self.formation_enemy_building(player, area))
                {
                    let slot = layout::oriented(group.destination, member.offset, group.heading);
                    found.push((member.id, slot, target));
                }
            }
            for (id, slot, target) in found {
                if self.issue_attack(player, id, target).is_ok() {
                    if let Some(attacker) = self.combat.attackers.get_mut(&id) {
                        attacker.resume = Some(slot);
                    }
                    changed.push(id);
                }
            }
        }
        self.advance_resumes(changed);
    }

    /// At most four spatial buckets and 128 inspections each. Rotate the
    /// window so a dense allied crowd cannot permanently conceal an enemy.
    fn formation_enemy(
        &self,
        seeker: EntityId,
        player: PlayerId,
        area: TileRect,
    ) -> Option<EntityId> {
        for y in area.min.y / SPATIAL_CHUNK_TILES..=(area.max.y - 1) / SPATIAL_CHUNK_TILES {
            for x in area.min.x / SPATIAL_CHUNK_TILES..=(area.max.x - 1) / SPATIAL_CHUNK_TILES {
                let Some(ids) = self.chunks.get(&ChunkCoord { x, y }) else {
                    continue;
                };
                let start =
                    ((self.tick.0 / 4 * 128 + u64::from(seeker.0)) % ids.len() as u64) as usize;
                for n in 0..ids.len().min(128) {
                    let id = ids[(start + n) % ids.len()];
                    let unit = self.units[self.lookup[&id]].state;
                    if unit.hp > 0
                        && unit.player != player
                        && area.contains(unit.position.tile_floor())
                        && self.unit_visible_to(player, id)
                    {
                        return Some(id);
                    }
                }
            }
        }
        None
    }

    /// Reuse the sight system's tile index: a fixed 13x13 window, with no
    /// scan through all players' buildings or remembered foreign structures.
    fn formation_enemy_building(&self, player: PlayerId, area: TileRect) -> Option<EntityId> {
        for y in area.min.y..area.max.y {
            for x in area.min.x..area.max.x {
                let tile = TileCoord::new(x, y);
                if let Some(id) = self.building_at_tile(tile)
                    && self
                        .building(id)
                        .is_some_and(|b| b.player != player && b.hp > 0)
                    && self.visibility(player, tile) == TileVisibility::Visible
                {
                    return Some(id);
                }
            }
        }
        None
    }
}
