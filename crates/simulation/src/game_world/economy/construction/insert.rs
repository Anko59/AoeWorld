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
        if !complete && self.site_has_building(footprint, span, Some(player)) {
            return Err(EconomyError::InvalidFootprint);
        }
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
        let age = self
            .player_economy(player)
            .map_or(crate::Age::Dark, |p| p.age);
        let researched = self.player_economy(player).map_or(0, |p| p.researched);
        let kind = crate::game_world::research::researched_building_kind(kind, researched);
        let rules = kind.rules();
        let los = kind.age_stats(age).1;
        let max_hp = crate::building_max_hp(kind, age, researched);
        if complete {
            self.install_building_collision(kind, footprint, true, bridge_span)?;
        }
        let id = self.allocate_entity_id()?;
        // Counting a player's units walks every unit: only a new player needs it.
        let population_used = if self.economy.players.contains_key(&player) {
            0
        } else {
            self.units()
                .filter(|unit| unit.player == player)
                .count()
                .min(u32::MAX as usize) as u32
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
            }
            * 100
            / crate::build_speed_percent(researched);
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
                    ConstructionState::Planned
                },
                work_done: if complete { work_total } else { 0 },
                work_total,
                hp: if complete { max_hp } else { 1 },
                max_hp,
                los: los
                    .saturating_add(self.researched_los_bonus(player))
                    .saturating_add(
                        if kind.is_tower()
                            || matches!(kind, BuildingKind::TownCenter | BuildingKind::Castle)
                        {
                            u8::from(self.has_researched(player, crate::Tech::Fletching))
                                + u8::from(self.has_researched(player, crate::Tech::BodkinArrow))
                                + u8::from(self.has_researched(player, crate::Tech::Bracer))
                        } else {
                            0
                        },
                    ),
                research: None,
                gate_open: false,
                gate_locked: false,
                rally_point: None,
                food_remaining: match kind {
                    BuildingKind::Farm => Some(self.farm_food(player)),
                    BuildingKind::FishTrap => Some(rules.food),
                    _ => None,
                },
                farm_reseeds: 0,
                garrison: Vec::new(),
                bridge_span,
            },
        );
        if !complete {
            self.index_foundation(id, player, footprint);
        }
        if complete && kind == BuildingKind::Road {
            self.economy.roads.insert(footprint.min);
        }
        if kind == BuildingKind::Farm {
            self.reset_farm_production(id);
        }
        let bucket = self.economy.dropoffs.entry((player, kind)).or_default();
        let at = bucket.binary_search(&id).unwrap_or_else(|at| at);
        bucket.insert(at, id);
        self.refresh_population_cap(player);
        self.index_new_building(id, player, footprint);
        self.steward_building_added(id, player, kind, footprint);
        if complete {
            self.register_gate_footprint(id, kind, footprint);
            if kind.is_tower() || kind == BuildingKind::Castle {
                self.combat.tower_ready.insert(id, 0);
            }
            self.update_building_vision(id);
            self.remember_visible_buildings();
        }
        Ok(id)
    }

    /// Publish the site and install movement collision when work first begins.
    pub(in crate::game_world) fn start_foundation(
        &mut self,
        id: EntityId,
    ) -> Result<(), EconomyError> {
        let b = self
            .economy
            .buildings
            .get(&id)
            .ok_or(EconomyError::UnknownEntity)?;
        if !b.is_unstarted() {
            return Ok(());
        }
        let (player, kind, footprint, span) = (b.player, b.kind, b.footprint, b.bridge_span);
        if self.site_has_building(footprint, span, None) {
            return Err(EconomyError::InvalidFootprint);
        }
        self.install_building_collision(kind, footprint, false, span)?;
        self.remove_foundation_reservation(id, player, footprint, span);
        self.economy
            .buildings
            .get_mut(&id)
            .ok_or(EconomyError::UnknownEntity)?
            .state = ConstructionState::UnderConstruction;
        self.register_gate_footprint(id, kind, footprint);
        self.index_new_building(id, player, footprint);
        Ok(())
    }

    fn install_building_collision(
        &mut self,
        kind: BuildingKind,
        footprint: TileRect,
        complete: bool,
        span: Option<aoe_core::BridgeSpan>,
    ) -> Result<(), EconomyError> {
        let Some(rect) = kind.collision_rect(footprint, complete) else {
            return Ok(());
        };
        if let Terrain::Map { overlay, .. } = &mut self.terrain {
            if let Some(span) = span {
                overlay.add_bridge_foundation(span)
            } else {
                overlay.add_footprint(rect)
            }
            .map_err(|_| EconomyError::Capacity)?;
        }
        for y in rect.min.y..rect.max.y {
            for x in rect.min.x..rect.max.x {
                let tile = TileCoord::new(x, y);
                if span.is_none_or(|span| span.contains(tile)) {
                    self.economy.blocked_tiles.insert(tile);
                }
            }
        }
        if kind.is_wall() {
            self.management_wall_changed(rect, true);
        }
        self.collision_closed(footprint);
        Ok(())
    }

    fn register_gate_footprint(&mut self, id: EntityId, kind: BuildingKind, footprint: TileRect) {
        if kind.is_gate() {
            self.economy.gates.insert(id);
            self.management_barrier_changed(footprint);
            for y in footprint.min.y..footprint.max.y {
                for x in footprint.min.x..footprint.max.x {
                    self.economy.gate_tiles.insert(TileCoord::new(x, y), id);
                }
            }
        }
    }
}
