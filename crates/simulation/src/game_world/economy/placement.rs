use super::{BuildingKind, EconomyError, VILLAGE_RULES};
use crate::{GameWorld, GameWorldError, Terrain, UnitKind};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use std::collections::{BTreeSet, VecDeque};

mod removal;

impl GameWorld {
    /// Player orders need tile-exact exploration of the entire footprint.
    /// Check knowledge before terrain/occupancy to avoid probing black ground.
    pub(super) fn validate_explored_footprint(
        &self,
        player: PlayerId,
        footprint: TileRect,
    ) -> Result<(), EconomyError> {
        if !footprint.valid(self.config.width_tiles, self.config.height_tiles) {
            return Err(EconomyError::InvalidFootprint);
        }
        for y in footprint.min.y..footprint.max.y {
            for x in footprint.min.x..footprint.max.x {
                if self.visibility(player, TileCoord::new(x, y))
                    == crate::TileVisibility::Unexplored
                {
                    return Err(EconomyError::UnexploredFootprint);
                }
            }
        }
        Ok(())
    }

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
        if !self.query(footprint).0.is_empty() {
            return Err(EconomyError::InvalidFootprint);
        }
        self.validate_footprint_ground(footprint)
    }

    /// Terrain, resource and building checks only: ordered foundations may
    /// stand on units, which step aside (see `construction::clearance`).
    pub(super) fn validate_footprint_ground(
        &self,
        footprint: TileRect,
    ) -> Result<(), EconomyError> {
        if !footprint.valid(self.config.width_tiles, self.config.height_tiles) {
            return Err(EconomyError::InvalidFootprint);
        }
        // The maintained tile index includes nonblocking Farms, open gates and
        // Town Center arches, so placement still reserves their full footprint.
        for y in footprint.min.y..footprint.max.y {
            for x in footprint.min.x..footprint.max.x {
                let tile = TileCoord::new(x, y);
                if self.building_at(tile).is_some()
                    || self.economy.blocked_tiles.contains(&tile)
                    || !self
                        .terrain
                        .passable_with_cancel(tile, self.config, &|| false)
                        .map_err(GameWorldError::Environment)?
                {
                    return Err(EconomyError::InvalidFootprint);
                }
                // Live passability already includes the resource overlay.
                // Immutable node records remain present after exhaustion.
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

    pub(super) fn open_town_center_arches(&mut self, footprint: TileRect) {
        let Some(solid) = BuildingKind::TownCenter.collision_rect(footprint, true) else {
            return;
        };
        for y in footprint.min.y..footprint.max.y {
            for x in footprint.min.x..footprint.max.x {
                let tile = TileCoord::new(x, y);
                if !solid.contains(tile) {
                    self.economy.blocked_tiles.remove(&tile);
                    if let Terrain::Map { overlay, .. } = &mut self.terrain {
                        overlay.remove_footprint(TileRect::from_xywh(x, y, 1, 1));
                    }
                }
            }
        }
        self.collision_opened(footprint);
    }

    /// Bounded local placement around the certified start, favoring inland
    /// clearings near real trees. The scout remains close to the town center.
    pub fn initialize_village(
        &mut self,
        player: PlayerId,
        start: TileCoord,
    ) -> Result<EntityId, EconomyError> {
        if self.player_buildings(player).next().is_some() {
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
        // The village already stands (Town Center, villagers): a kit that cannot
        // be placed in full must not fail the start, or a host that retries
        // another site would meet `AlreadyStarted` with a half-made village.
        match self.initialize_starting_kit(
            TileRect::new(origin, TileCoord::new(origin.x + side, origin.y + side)),
            scout_tile,
        ) {
            Ok(()) | Err(EconomyError::InvalidFootprint) => {}
            Err(error) => return Err(error),
        }
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

mod geometry;
pub(crate) use geometry::{adjacent, distance_squared, perimeter, position, rects_overlap};
