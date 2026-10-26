//! Same-profession food replacement through the existing wildlife cell index.
use super::{AnimalKind, GameAnimal, cell_of, distance, radius};
use crate::{GameWorld, TileVisibility, VILLAGE_RULES};
use aoe_core::{EntityId, PlayerId, TileCoord, WorldPosition};

impl GameWorld {
    /// At most sixteen eight-tile cells for the shared ten-tile replacement
    /// radius. Work depends on nearby animals, never on distant seats or map
    /// area. Equal distances choose the smaller identity on every replay.
    pub(crate) fn next_food_animal(
        &mut self,
        player: PlayerId,
        from: TileCoord,
        kind: AnimalKind,
        exclude: EntityId,
    ) -> Option<GameAnimal> {
        let reach = VILLAGE_RULES.replacement_radius_tiles;
        let min = cell_of(TileCoord::new(from.x - reach, from.y - reach));
        let max = cell_of(TileCoord::new(from.x + reach, from.y + reach));
        let origin = WorldPosition::from_tile_center(from).ok()?;
        let squared = radius(i64::from(reach));
        let mut nearest: Option<(i64, EntityId, GameAnimal)> = None;
        for y in min.1..=max.1 {
            for x in min.0..=max.0 {
                self.wildlife.stats.food_search_cells =
                    self.wildlife.stats.food_search_cells.saturating_add(1);
                let Some(ids) = self.wildlife.cells.get(&(x, y)) else {
                    continue;
                };
                for id in ids {
                    self.wildlife.stats.food_search_candidates =
                        self.wildlife.stats.food_search_candidates.saturating_add(1);
                    let Some(a) = self.wildlife.animals.get(id).map(|a| a.state) else {
                        continue;
                    };
                    let same_kind = if kind == AnimalKind::Sheep {
                        a.kind == AnimalKind::Sheep && a.owner == Some(player)
                    } else {
                        matches!(a.kind, AnimalKind::Deer | AnimalKind::Boar)
                    };
                    if *id == exclude || a.food_remaining == 0 || !same_kind {
                        continue;
                    }
                    if a.owner != Some(player)
                        && self.visibility(player, a.position.tile_floor())
                            != TileVisibility::Visible
                    {
                        continue;
                    }
                    let gap = distance(origin, a.position);
                    if gap <= squared
                        && nearest
                            .as_ref()
                            .is_none_or(|(d, id, _)| (gap, a.id) < (*d, *id))
                    {
                        nearest = Some((gap, a.id, a));
                    }
                }
            }
        }
        nearest.map(|(_, _, animal)| animal)
    }
}
