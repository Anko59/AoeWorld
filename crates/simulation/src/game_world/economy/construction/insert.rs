//! Ordinary and shaped building insertion after placement validation.
use super::{
    BuildingKind, ConstructionState, EconomyError, GameBuilding, MarketPrices, PlayerEconomy,
    Stockpile,
};
use crate::{GameWorld, GameWorldError, Terrain};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use std::collections::VecDeque;

impl GameWorld {
    /// Create a building (or foundation) after its footprint was validated.
    /// Nothing is charged here; the caller owns the cost. Foundations reveal
    /// only after a builder contributes construction work.
    pub(in crate::game_world) fn insert_building(
        &mut self,
        player: PlayerId,
        kind: BuildingKind,
        footprint: TileRect,
        complete: bool,
    ) -> Result<EntityId, EconomyError> {
        let span = if kind == BuildingKind::Bridge {
            Some(
                aoe_core::BridgeSpan::new(
                    footprint.min,
                    TileCoord::new(footprint.max.x - 1, footprint.max.y - 1),
                )
                .ok_or(EconomyError::InvalidFootprint)?,
            )
        } else {
            None
        };
        self.insert_building_shape(player, kind, footprint, complete, span)
    }

    pub(in crate::game_world) fn insert_building_shape(
        &mut self,
        player: PlayerId,
        kind: BuildingKind,
        footprint: TileRect,
        complete: bool,
        bridge_span: Option<aoe_core::BridgeSpan>,
    ) -> Result<EntityId, EconomyError> {
        if self.next_entity_id > u64::from(u32::MAX) {
            return Err(GameWorldError::EntityIdExhausted.into());
        }
        let rules = kind.rules();
        let age = self
            .player_economy(player)
            .map_or(crate::Age::Dark, |p| p.age);
        let (mut max_hp, los) = kind.age_stats(age);
        if kind != BuildingKind::Farm
            && !kind.is_barrier()
            && self.has_researched(player, crate::Tech::Masonry)
        {
            max_hp += max_hp / 10;
        }
        let collision = kind.collision_rect(footprint, complete);
        if let Some(rect) = collision
            && let Terrain::Map { overlay, .. } = &mut self.terrain
        {
            if let Some(span) = bridge_span {
                overlay.add_bridge_foundation(span)
            } else {
                overlay.add_footprint(rect)
            }
            .map_err(|_| EconomyError::Capacity)?;
        }
        let id = self.allocate_entity_id()?;
        if let Some(rect) = collision {
            for y in rect.min.y..rect.max.y {
                for x in rect.min.x..rect.max.x {
                    let tile = TileCoord::new(x, y);
                    if bridge_span.is_none_or(|span| span.contains(tile)) {
                        self.economy.blocked_tiles.insert(tile);
                    }
                }
            }
        }
        // Counting a player's units walks every unit: only a new player needs it.
        let population_used = if self.economy.players.contains_key(&player) {
            0
        } else {
            self.units()
                .filter(|unit| unit.player == player)
                .count()
                .min(u16::MAX as usize) as u16
        };
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
            * super::buildings::BUILD_WORK_PER_BASE_TICK
            * if kind == BuildingKind::Bridge {
                (footprint.width().max(footprint.height()) - 2) as u64
            } else {
                1
            };
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
                    .saturating_add(
                        if matches!(
                            kind,
                            BuildingKind::TownCenter
                                | BuildingKind::WatchTower
                                | BuildingKind::Castle
                        ) {
                            u8::from(self.has_researched(player, crate::Tech::Fletching))
                                + u8::from(self.has_researched(player, crate::Tech::BodkinArrow))
                        } else {
                            0
                        },
                    ),
                research: None,
                gate_open: false,
                rally_point: None,
                food_remaining: (kind == BuildingKind::Farm).then(|| self.farm_food(player)),
                farm_reseeds: 0,
                garrison: Vec::new(),
                bridge_span,
            },
        );
        if kind == BuildingKind::Farm {
            self.reset_farm_production(id);
        }
        let bucket = self.economy.dropoffs.entry((player, kind)).or_default();
        let at = bucket.binary_search(&id).unwrap_or_else(|at| at);
        bucket.insert(at, id);
        if complete && matches!(kind, BuildingKind::Castle | BuildingKind::WatchTower) {
            self.combat.tower_ready.insert(id, 0);
        }
        self.refresh_population_cap(player);
        self.index_new_building(id, player, footprint);
        if complete {
            if matches!(kind, BuildingKind::WatchTower | BuildingKind::Castle) {
                self.combat.tower_ready.insert(id, 0);
            }
            self.update_building_vision(id);
            self.remember_visible_buildings();
        }
        if rules.blocks {
            self.collision_closed(footprint);
        }
        Ok(id)
    }
}
