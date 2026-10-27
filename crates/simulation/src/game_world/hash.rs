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
        GameWorldError::VassalLeash => 12,
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
        self.idle_spacing.update_hash(&mut hash);
        hash.update(&(self.active_planner_count as u64).to_le_bytes());
        hash.update(&(self.active_route_searches as u64).to_le_bytes());
        hash.update(&self.planning_budget.to_le_bytes());
        hash.update(
            &self
                .navigation_preparation_budget
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .to_le_bytes(),
        );
        hash.update(&self.fast_route_budget.to_le_bytes());
        hash.update(&(self.route_queued.len() as u64).to_le_bytes());
        for (player, far) in &self.route_queued {
            hash.update(&player.0.to_le_bytes());
            hash.update(&[u8::from(*far)]);
        }
        hash.update(&(self.fast_route_spent.len() as u64).to_le_bytes());
        for (player, spent) in &self.fast_route_spent {
            hash.update(&player.0.to_le_bytes());
            hash.update(&spent.to_le_bytes());
        }
        self.terrain.update_mutable_state_hash(&mut hash);
        hash.update(&self.next_entity_id.to_le_bytes());
        self.economy.update_hash(&mut hash);
        self.combat.update_hash(&mut hash);
        self.wildlife.update_hash(&mut hash);
        self.vision.update_hash(&mut hash);
        for unit in &self.units {
            hash.update(&unit.state.id.0.to_le_bytes());
            // Both cyclic neighbour windows carry mutable sampling order.
            hash.update(&self.followers.sampling_slot(unit.state.id).to_le_bytes());
            let wildlife_slot = match unit.wildlife_slot {
                usize::MAX => u64::MAX,
                slot => slot as u64,
            };
            hash.update(&wildlife_slot.to_le_bytes());
            hash.update(&unit.state.player.0.to_le_bytes());
            hash.update(&unit.state.position.x.to_le_bytes());
            hash.update(&unit.state.position.y.to_le_bytes());
            hash.update(&unit.state.previous_position.x.to_le_bytes());
            hash.update(&unit.state.previous_position.y.to_le_bytes());
            hash.update(&[
                unit.state.moving as u8,
                unit.state.planning as u8,
                unit.state.facing as u8,
                unit.state.kind as u8,
                unit.state.task as u8,
            ]);
            hash.update(&[u8::from(unit.state.auto_scout)]);
            hash.update(&unit.next_hit_tick.to_le_bytes());
            hash.update(&unit.state.hp.to_le_bytes());
            hash.update(&unit.state.max_hp.to_le_bytes());
            hash.update(
                &unit
                    .state
                    .attack_target
                    .map_or(u32::MAX, |id| id.0)
                    .to_le_bytes(),
            );
            super::combat::hash_hit(&mut hash, unit.state.last_hit);
            hash.update(&[unit.state.death_hunt.map_or(u8::MAX, |k| k as u8)]);
            hash.update(
                &unit
                    .state
                    .death_tick
                    .map_or(u64::MAX, |t| t.0)
                    .to_le_bytes(),
            );
            if let Some(load) = unit.state.carried {
                hash.update(&[1, load.kind as u8]);
                hash.update(&load.amount.to_le_bytes());
            } else {
                hash.update(&[0]);
            }
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
            hash.update(&[u8::from(unit.fast_route_tried)]);
            if let Some(obstruction) = unit.obstruction {
                hash.update(&[1]);
                hash.update(&obstruction.goal.x.to_le_bytes());
                hash.update(&obstruction.goal.y.to_le_bytes());
                hash.update(&obstruction.remaining.to_le_bytes());
                hash.update(&obstruction.blocked.to_le_bytes());
                hash.update(&obstruction.last_tick.to_le_bytes());
                hash.update(&obstruction.search_tick.to_le_bytes());
            } else {
                hash.update(&[0]);
            }
            match unit.unreachable_memo {
                None => hash.update(&[0]),
                Some((from, to, at)) => {
                    hash.update(&[1]);
                    for value in [from.x, from.y, to.x, to.y] {
                        hash.update(&value.to_le_bytes());
                    }
                    hash.update(&at.to_le_bytes())
                }
            };
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
        if !self.move_queues.is_empty() {
            hash.update(b"player move queues\0");
            hash.update(&(self.move_queues.len() as u64).to_le_bytes());
            for (id, queue) in &self.move_queues {
                hash.update(&id.0.to_le_bytes());
                hash.update(&(queue.len() as u64).to_le_bytes());
                for point in queue {
                    hash.update(&point.x.to_le_bytes());
                    hash.update(&point.y.to_le_bytes());
                }
            }
        }
        self.hash_formations(&mut hash);
        *hash.finalize().as_bytes()
    }

    pub fn canonical_hash_hex(&self) -> String {
        self.canonical_hash()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoe_core::CoordinateError;
    use aoe_map::EnvironmentPageError;

    #[test]
    fn movement_error_codes_stay_stable_for_replay_compatibility() {
        for (error, expected) in [
            (
                GameWorldError::InvalidConfig(CoordinateError::NegativeTile),
                1,
            ),
            (GameWorldError::InvalidConfig(CoordinateError::Overflow), 2),
            (
                GameWorldError::InvalidConfig(CoordinateError::InvalidDimensions),
                3,
            ),
            (
                GameWorldError::InvalidConfig(CoordinateError::InvalidSimulationConfig),
                4,
            ),
            (GameWorldError::InvalidPosition, 5),
            (GameWorldError::Unreachable, 6),
            (GameWorldError::PathBudgetExceeded, 7),
            (GameWorldError::UnknownEntity, 8),
            (GameWorldError::StartSearchLimit, 9),
            (GameWorldError::EntityIdExhausted, 10),
            (GameWorldError::InvalidTerrain, 11),
            (
                GameWorldError::Environment(EnvironmentPageError::Missing),
                20,
            ),
            (
                GameWorldError::Environment(EnvironmentPageError::Corrupt),
                21,
            ),
            (
                GameWorldError::Environment(EnvironmentPageError::Unavailable),
                22,
            ),
            (
                GameWorldError::Environment(EnvironmentPageError::Cancelled),
                23,
            ),
            (
                GameWorldError::Environment(EnvironmentPageError::Invalid),
                24,
            ),
        ] {
            assert_eq!(movement_error_code(error), expected, "{error}");
        }
    }
}
