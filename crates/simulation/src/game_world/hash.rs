use super::{GameWorld, GameWorldError};

fn movement_error_code(error: GameWorldError) -> u8 {
    match error {
        GameWorldError::InvalidConfig(error) => match error {
            aoe_core::CoordinateError::NegativeTile => 1,
            aoe_core::CoordinateError::Overflow => 2,
            aoe_core::CoordinateError::InvalidDimensions => 3,
            aoe_core::CoordinateError::InvalidSimulationConfig => 4,
        },
        GameWorldError::InvalidPosition => 5,
        GameWorldError::Unreachable => 6,
        GameWorldError::PathBudgetExceeded => 7,
        GameWorldError::UnknownEntity => 8,
        GameWorldError::StartSearchLimit => 9,
        GameWorldError::EntityIdExhausted => 10,
        GameWorldError::InvalidTerrain => 11,
        GameWorldError::Environment(error) => {
            20 + match error {
                aoe_map::EnvironmentPageError::Missing => 0,
                aoe_map::EnvironmentPageError::Corrupt => 1,
                aoe_map::EnvironmentPageError::Unavailable => 2,
                aoe_map::EnvironmentPageError::Cancelled => 3,
                aoe_map::EnvironmentPageError::Invalid => 4,
            }
        }
    }
}

impl GameWorld {
    pub fn canonical_hash(&self) -> [u8; 32] {
        let mut hash = blake3::Hasher::new();
        hash.update(&self.config.width_tiles.to_le_bytes());
        hash.update(&self.config.height_tiles.to_le_bytes());
        hash.update(&self.config.seed.0.to_le_bytes());
        hash.update(&self.config.tick_hz.to_le_bytes());
        hash.update(&self.config.move_speed_subunits_per_tick.to_le_bytes());
        hash.update(
            &self
                .config
                .move_speed_subunits_per_tick_denominator
                .to_le_bytes(),
        );
        hash.update(&self.tick.0.to_le_bytes());
        hash.update(&(self.planning_cursor as u64).to_le_bytes());
        hash.update(&(self.active_planner_count as u64).to_le_bytes());
        hash.update(&self.planning_budget.to_le_bytes());
        self.terrain.update_mutable_state_hash(&mut hash);
        for unit in &self.units {
            hash.update(&unit.state.id.0.to_le_bytes());
            hash.update(&unit.state.player.0.to_le_bytes());
            hash.update(&unit.state.position.x.to_le_bytes());
            hash.update(&unit.state.position.y.to_le_bytes());
            hash.update(&unit.state.previous_position.x.to_le_bytes());
            hash.update(&unit.state.previous_position.y.to_le_bytes());
            hash.update(&[
                unit.state.moving as u8,
                unit.state.planning as u8,
                unit.state.facing as u8,
            ]);
            if let Some(order) = unit.order {
                hash.update(&order.origin.x.to_le_bytes());
                hash.update(&order.origin.y.to_le_bytes());
                hash.update(&order.destination.x.to_le_bytes());
                hash.update(&order.destination.y.to_le_bytes());
                hash.update(&order.waypoint.x.to_le_bytes());
                hash.update(&order.waypoint.y.to_le_bytes());
                hash.update(&order.target_tile.x.to_le_bytes());
                hash.update(&order.target_tile.y.to_le_bytes());
                hash.update(&order.segment_length.to_le_bytes());
                hash.update(&order.travelled.to_le_bytes());
                hash.update(&order.speed_carry.to_le_bytes());
            } else {
                hash.update(&[0; 48]);
            }
            hash.update(&(unit.route.len() as u64).to_le_bytes());
            for tile in &unit.route {
                hash.update(&tile.x.to_le_bytes());
                hash.update(&tile.y.to_le_bytes());
            }
            if let Some(planner) = &unit.planner {
                hash.update(&[1]);
                planner.update_hash(&mut hash);
            } else {
                hash.update(&[0]);
            }
            if let Some(error) = unit.last_movement_error {
                hash.update(&[1, movement_error_code(error)]);
            } else {
                hash.update(&[0, 0]);
            }
        }
        *hash.finalize().as_bytes()
    }

    pub fn canonical_hash_hex(&self) -> String {
        self.canonical_hash()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}
