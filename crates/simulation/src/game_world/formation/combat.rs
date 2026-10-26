use super::*;

impl GameWorld {
    pub(crate) fn engage_formations(&mut self, changed: &mut Vec<EntityId>) {
        let leaders = self
            .formations
            .iter()
            .filter(|(id, g)| g.attack_move && u64::from(id.0) % 4 == self.tick.0 % 4)
            .map(|(&id, _)| id)
            .collect::<Vec<_>>();
        for leader in leaders {
            let group = &self.formations[&leader];
            let player = self.units[self.lookup[&leader]].state.player;
            // Check the centroid and four rotating, distributed members.
            // Wide wings can contact an enemy outside the centroid's window;
            // the number of spatial probes stays independent of army size.
            let probes = group.members.len().min(4);
            let target = (0..=probes).find_map(|n| {
                let point = if n == 0 {
                    group.anchor
                } else {
                    let at = (self.tick.0 as usize / 4 * 4
                        + (n - 1) * group.members.len() / probes)
                        % group.members.len();
                    self.unit(group.members[at].id)?.position
                };
                let tile = point.tile_floor();
                let area = TileRect::from_xywh(tile.x - 6, tile.y - 6, 13, 13)
                    .clamp(self.config.width_tiles, self.config.height_tiles);
                self.formation_enemy(leader, player, area)
                    .or_else(|| self.formation_enemy_building(player, area))
            });
            if let Some(target) = target {
                let ids = group.members.iter().map(|m| m.id).collect::<Vec<_>>();
                self.break_formation(leader);
                for id in ids {
                    let _ = self.issue_attack(player, id, target);
                    changed.push(id);
                }
            }
        }
    }

    /// At most four spatial buckets and 128 inspections each. Rotate the
    /// window so a dense allied crowd cannot permanently conceal an enemy.
    fn formation_enemy(
        &self,
        leader: EntityId,
        player: PlayerId,
        area: TileRect,
    ) -> Option<EntityId> {
        for y in area.min.y / SPATIAL_CHUNK_TILES..=(area.max.y - 1) / SPATIAL_CHUNK_TILES {
            for x in area.min.x / SPATIAL_CHUNK_TILES..=(area.max.x - 1) / SPATIAL_CHUNK_TILES {
                let Some(ids) = self.chunks.get(&ChunkCoord { x, y }) else {
                    continue;
                };
                let start =
                    ((self.tick.0 / 4 * 128 + u64::from(leader.0)) % ids.len() as u64) as usize;
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
