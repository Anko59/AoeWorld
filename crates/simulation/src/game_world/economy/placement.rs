use super::{
    BuildingKind, ConstructionState, EconomyError, GameBuilding, MAX_POPULATION, MarketPrices,
    PlayerEconomy, Stockpile, VILLAGE_RULES,
};
use crate::{GameWorld, GameWorldError, Terrain, UnitKind};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect, WorldPosition};
use std::collections::{BTreeSet, VecDeque};

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
        // Farms are walked on, so they are not in `blocked_tiles`. This scan
        // runs per placement command, never per tick.
        if self
            .economy
            .buildings
            .values()
            .any(|building| rects_overlap(building.footprint, footprint))
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
        if self.economy.buildings.len() >= self.economy.limits.buildings
            || (!self.economy.players.contains_key(&player)
                && self.economy.players.len() >= self.economy.limits.players)
        {
            return Err(EconomyError::Capacity);
        }
        let side = VILLAGE_RULES.town_center_side;
        let footprint = TileRect::new(
            origin,
            TileCoord::new(origin.x.saturating_add(side), origin.y.saturating_add(side)),
        );
        self.validate_footprint(footprint)?;
        self.insert_building(player, BuildingKind::TownCenter, footprint, true)
    }

    /// Create a building (or foundation) after its footprint was validated.
    /// Nothing is charged here; the caller owns the cost. Foundations reveal
    /// only after the caller accepts their builder approaches.
    pub(in crate::game_world) fn insert_building(
        &mut self,
        player: PlayerId,
        kind: BuildingKind,
        footprint: TileRect,
        complete: bool,
    ) -> Result<EntityId, EconomyError> {
        if self.next_entity_id > u64::from(u32::MAX) {
            return Err(GameWorldError::EntityIdExhausted.into());
        }
        let rules = kind.rules();
        let age = self
            .player_economy(player)
            .map_or(crate::Age::Dark, |p| p.age);
        let (max_hp, los) = kind.age_stats(age);
        if rules.blocks
            && let Terrain::Map { overlay, .. } = &mut self.terrain
        {
            overlay
                .add_footprint(footprint)
                .map_err(|_| EconomyError::Capacity)?;
        }
        let id = self.allocate_entity_id()?;
        if rules.blocks {
            for y in footprint.min.y..footprint.max.y {
                for x in footprint.min.x..footprint.max.x {
                    self.economy.blocked_tiles.insert(TileCoord::new(x, y));
                }
            }
        }
        let population_used = self
            .units()
            .filter(|unit| unit.player == player)
            .count()
            .min(u16::MAX as usize) as u16;
        self.economy.players.entry(player).or_insert(PlayerEconomy {
            player,
            stockpile: Stockpile::default(),
            population_used,
            population_cap: 0,
            age: crate::Age::Dark,
            researched: 0,
            market_prices: MarketPrices::default(),
        });
        let work_total = u64::from(rules.build_seconds)
            * u64::from(self.config.tick_hz)
            * super::buildings::BUILD_WORK_PER_BASE_TICK;
        self.economy.buildings.insert(
            id,
            GameBuilding {
                age,
                id,
                player,
                kind,
                footprint,
                training_queue: VecDeque::new(),
                last_hit: None,
                state: if complete {
                    ConstructionState::Complete
                } else {
                    ConstructionState::UnderConstruction
                },
                work_done: if complete { work_total } else { 0 },
                work_total,
                hp: if complete { max_hp } else { 1 },
                max_hp,
                los: los
                    .saturating_add(self.researched_los_bonus(player))
                    .saturating_add(u8::from(
                        self.has_researched(player, crate::Tech::Fletching)
                            && matches!(kind, BuildingKind::TownCenter | BuildingKind::WatchTower),
                    )),
                research: None,
                gate_open: false,
                food_remaining: (kind == BuildingKind::Farm).then(|| self.farm_food(player)),
            },
        );
        self.refresh_population_cap(player);
        self.index_new_building(id, player, footprint);
        if complete {
            self.update_building_vision(id);
            self.remember_visible_buildings();
        }
        if rules.blocks {
            self.resource_collision_changed();
        }
        Ok(id)
    }

    /// Housing: the supporting completed buildings, capped at 200.
    pub(super) fn refresh_population_cap(&mut self, player: PlayerId) {
        let cap = self
            .economy
            .buildings
            .values()
            .filter(|building| building.player == player && building.is_complete())
            .map(|building| u32::from(building.kind.rules().population_support))
            .sum::<u32>()
            .min(u32::from(MAX_POPULATION)) as u16;
        if let Some(state) = self.economy.players.get_mut(&player) {
            state.population_cap = cap;
        }
    }

    /// Delete a building and its collision (cancelled foundation, spent farm).
    pub(crate) fn remove_building(&mut self, id: EntityId) {
        let Some(building) = self.economy.buildings.remove(&id) else {
            return;
        };
        self.forget_building(id, building.footprint);
        if building.kind.rules().blocks {
            for y in building.footprint.min.y..building.footprint.max.y {
                for x in building.footprint.min.x..building.footprint.max.x {
                    self.economy.blocked_tiles.remove(&TileCoord::new(x, y));
                }
            }
            if let Terrain::Map { overlay, .. } = &mut self.terrain {
                overlay.remove_footprint(building.footprint);
            }
            self.resource_collision_changed();
        }
        self.economy.open_gates.remove(&id);
        self.combat.tower_ready.remove(&id);
        self.refresh_population_cap(building.player);
        self.update_observer(id, None);
        self.remember_visible_buildings();
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
        if self.economy.buildings.len() >= self.economy.limits.buildings
            || (!self.economy.players.contains_key(&player)
                && self.economy.players.len() >= self.economy.limits.players)
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
            let mut preserve_start = start_is_free
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
            let mut perimeter = self.village_spawns(rect, preserve_start.then_some(start));
            if preserve_start && perimeter.len() < usize::from(VILLAGE_RULES.starting_villagers) {
                preserve_start = false;
                perimeter = self.village_spawns(rect, None);
            }
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
        self.initialize_starting_kit(
            TileRect::new(origin, TileCoord::new(origin.x + side, origin.y + side)),
            scout_tile,
        )?;
        self.initialize_wildlife(TileCoord::new(origin.x + side / 2, origin.y + side / 2));
        self.remember_visible_resources();
        Ok(scout)
    }
    fn village_spawns(&self, footprint: TileRect, scout: Option<TileCoord>) -> Vec<TileCoord> {
        let edge = perimeter(footprint);
        let roots = scout.map_or_else(|| edge.clone(), |tile| vec![tile]);
        let center = TileCoord::new(footprint.min.x + 2, footprint.min.y + 2);
        let mut surveyed = BTreeSet::<TileCoord>::new();
        let mut best = Vec::new();
        for root in roots {
            if !self.tile_passable(root) || surveyed.contains(&root) {
                continue;
            }
            let mut reached = BTreeSet::from([root]);
            let mut queue = VecDeque::from([root]);
            while let Some(tile) = queue.pop_front() {
                for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                    let next = TileCoord::new(tile.x + dx, tile.y + dy);
                    if next.x.abs_diff(center.x) > 8
                        || next.y.abs_diff(center.y) > 8
                        || footprint.contains(next)
                        || reached.contains(&next)
                        || !self.tile_passable(next)
                        || !self.terrain.crossable(tile, next, self.config)
                    {
                        continue;
                    }
                    reached.insert(next);
                    queue.push_back(next);
                }
            }
            surveyed.extend(&reached);
            let mut free = reached
                .into_iter()
                .filter(|tile| Some(*tile) != scout && self.tile_free(*tile))
                .collect::<Vec<_>>();
            free.sort_by_key(|tile| {
                (
                    !edge.contains(tile),
                    distance_squared(*tile, center),
                    tile.y,
                    tile.x,
                )
            });
            // Keep the original canonical perimeter order when it has room.
            let mut perimeter = edge
                .iter()
                .copied()
                .filter(|tile| free.contains(tile))
                .collect::<Vec<_>>();
            perimeter.extend(free.into_iter().filter(|tile| !edge.contains(tile)));
            if perimeter.len() > best.len() {
                best = perimeter;
            }
            if best.len()
                >= usize::from(VILLAGE_RULES.starting_villagers) + usize::from(scout.is_none())
            {
                break;
            }
        }
        best
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

pub(crate) fn rects_overlap(a: TileRect, b: TileRect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}
/// Is `tile` in the one-tile ring around `rect`, corners included?
pub(crate) fn adjacent(rect: TileRect, tile: TileCoord) -> bool {
    tile.x >= rect.min.x - 1
        && tile.x <= rect.max.x
        && tile.y >= rect.min.y - 1
        && tile.y <= rect.max.y
        && !rect.contains(tile)
}
