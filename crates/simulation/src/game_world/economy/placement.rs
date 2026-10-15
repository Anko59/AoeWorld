use super::{
    BuildingKind, EconomyError, GameBuilding, MAX_ECONOMY_BUILDINGS, MAX_ECONOMY_PLAYERS,
    PlayerEconomy, Stockpile, VILLAGE_RULES,
};
use crate::{GameWorld, GameWorldError, Terrain, UnitKind};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect, WorldPosition};
use std::collections::VecDeque;

impl GameWorld {
    pub fn tile_passable(&self, tile: TileCoord) -> bool {
        !self.economy.blocked_tiles.contains(&tile) && self.terrain.passable(tile, self.config)
    }

    pub(crate) fn tile_free(&self, tile: TileCoord) -> bool {
        self.tile_passable(tile)
            && self
                .query(TileRect::new(tile, TileCoord::new(tile.x + 1, tile.y + 1)))
                .0
                .is_empty()
    }

    pub(super) fn validate_footprint(&self, footprint: TileRect) -> Result<(), EconomyError> {
        if !footprint.valid(self.config.width_tiles, self.config.height_tiles)
            || self
                .units()
                .any(|unit| footprint.contains(unit.position.tile_floor()))
        {
            return Err(EconomyError::InvalidFootprint);
        }
        for y in footprint.min.y..footprint.max.y {
            for x in footprint.min.x..footprint.max.x {
                let tile = TileCoord::new(x, y);
                if self.economy.blocked_tiles.contains(&tile)
                    || !self
                        .terrain
                        .passable_with_cancel(tile, self.config, &|| false)
                        .map_err(GameWorldError::Environment)?
                {
                    return Err(EconomyError::InvalidFootprint);
                }
                if let Terrain::Map { generator, .. } = &self.terrain
                    && generator
                        .object_at_with_cancel(tile, &|| false)
                        .map_err(GameWorldError::Environment)?
                        .is_some()
                {
                    return Err(EconomyError::InvalidFootprint);
                }
            }
        }
        Ok(())
    }

    pub fn place_town_center(
        &mut self,
        player: PlayerId,
        origin: TileCoord,
    ) -> Result<EntityId, EconomyError> {
        if self.economy.buildings.len() >= MAX_ECONOMY_BUILDINGS
            || (!self.economy.players.contains_key(&player)
                && self.economy.players.len() >= MAX_ECONOMY_PLAYERS)
        {
            return Err(EconomyError::Capacity);
        }
        let side = VILLAGE_RULES.town_center_side;
        let footprint = TileRect::new(
            origin,
            TileCoord::new(origin.x.saturating_add(side), origin.y.saturating_add(side)),
        );
        self.validate_footprint(footprint)?;
        if self.next_entity_id > u64::from(u32::MAX) {
            return Err(GameWorldError::EntityIdExhausted.into());
        }
        if let Terrain::Map { overlay, .. } = &mut self.terrain {
            overlay
                .add_footprint(footprint)
                .map_err(|_| EconomyError::Capacity)?;
        }
        let id = self.allocate_entity_id()?;
        for y in footprint.min.y..footprint.max.y {
            for x in footprint.min.x..footprint.max.x {
                self.economy.blocked_tiles.insert(TileCoord::new(x, y));
            }
        }
        let population_used = self
            .units()
            .filter(|unit| unit.player == player)
            .count()
            .min(u16::MAX as usize) as u16;
        let state = self.economy.players.entry(player).or_insert(PlayerEconomy {
            player,
            stockpile: Stockpile::default(),
            population_used,
            population_cap: 0,
        });
        state.population_cap = state
            .population_cap
            .saturating_add(VILLAGE_RULES.population_support);
        self.economy.buildings.insert(
            id,
            GameBuilding {
                id,
                player,
                kind: BuildingKind::TownCenter,
                footprint,
                training_queue: VecDeque::new(),
            },
        );
        self.resource_collision_changed();
        Ok(id)
    }

    /// Bounded local placement around the certified start, favoring inland
    /// clearings near real trees. The scout remains close to the town center.
    pub fn initialize_village(
        &mut self,
        player: PlayerId,
        start: TileCoord,
    ) -> Result<EntityId, EconomyError> {
        if self.buildings().any(|building| building.player == player) {
            return Err(EconomyError::AlreadyStarted);
        }
        if self.economy.buildings.len() >= MAX_ECONOMY_BUILDINGS
            || (!self.economy.players.contains_key(&player)
                && self.economy.players.len() >= MAX_ECONOMY_PLAYERS)
            || self
                .next_entity_id
                .saturating_add(u64::from(VILLAGE_RULES.starting_villagers) + 1)
                > u64::from(u32::MAX)
        {
            return Err(EconomyError::Capacity);
        }
        let radius = VILLAGE_RULES.start_search_radius_tiles;
        let preferred = TileCoord::new(start.x.saturating_add(1), start.y.saturating_add(1));
        let mut candidates = Vec::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                candidates.push(TileCoord::new(
                    start.x.saturating_add(dx),
                    start.y.saturating_add(dy),
                ));
            }
        }
        candidates.sort_by_key(|tile| {
            (
                usize::from(*tile != preferred),
                distance_squared(*tile, start),
                tile.y,
                tile.x,
            )
        });
        self.rank_start_sites(&mut candidates, start)?;
        let side = VILLAGE_RULES.town_center_side;
        let start_is_free = self.tile_free(start);
        let mut selected = None;
        for origin in candidates {
            let rect = TileRect::new(
                origin,
                TileCoord::new(origin.x.saturating_add(side), origin.y.saturating_add(side)),
            );
            let preserve_start = start_is_free
                && !rect.contains(start)
                && distance_squared(
                    start,
                    TileCoord::new(origin.x + side / 2, origin.y + side / 2),
                ) <= 64;
            match self.validate_footprint(rect) {
                Ok(()) => {}
                Err(EconomyError::InvalidFootprint) => continue,
                Err(error) => return Err(error),
            }
            let perimeter = perimeter(rect)
                .into_iter()
                .filter(|tile| self.tile_free(*tile) && (!preserve_start || *tile != start))
                .collect::<Vec<_>>();
            let required =
                usize::from(VILLAGE_RULES.starting_villagers) + usize::from(!preserve_start);
            if perimeter.len() >= required {
                selected = Some((origin, perimeter, preserve_start));
                break;
            }
        }
        let (origin, mut perimeter, preserve_start) =
            selected.ok_or(EconomyError::InvalidFootprint)?;
        let scout_tile = if preserve_start {
            start
        } else {
            perimeter.remove(0)
        };
        // Put the scout first in the shared allocator so the existing client's
        // primary unit and default unit id remain the scout.
        let scout = self.spawn_kind(player, UnitKind::Scout, position(scout_tile)?)?;
        self.place_town_center(player, origin)?;
        if let Some(state) = self.economy.players.get_mut(&player) {
            state.stockpile = VILLAGE_RULES.starting_stockpile;
        }
        for tile in perimeter
            .into_iter()
            .take(usize::from(VILLAGE_RULES.starting_villagers))
        {
            self.spawn_villager(player, position(tile)?)?;
        }
        self.initialize_starting_kit(TileRect::new(
            origin,
            TileCoord::new(origin.x + side, origin.y + side),
        ))?;
        Ok(scout)
    }
}

pub(crate) fn position(tile: TileCoord) -> Result<WorldPosition, GameWorldError> {
    WorldPosition::from_tile_center(tile).map_err(|_| GameWorldError::InvalidPosition)
}
pub(crate) fn distance_squared(a: TileCoord, b: TileCoord) -> u64 {
    let x = u64::from(a.x.abs_diff(b.x));
    let y = u64::from(a.y.abs_diff(b.y));
    x.saturating_mul(x).saturating_add(y.saturating_mul(y))
}
pub(crate) fn perimeter(rect: TileRect) -> Vec<TileCoord> {
    let mut tiles = Vec::new();
    for y in rect.min.y - 1..=rect.max.y {
        for x in rect.min.x - 1..=rect.max.x {
            let tile = TileCoord::new(x, y);
            if !rect.contains(tile) {
                tiles.push(tile);
            }
        }
    }
    tiles
}
