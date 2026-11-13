use super::slots::total_pending;
use super::{
    GameplayService, MAX_ACK_HISTORY, MAX_PENDING_COMMANDS_GLOBAL,
    MAX_PENDING_COMMANDS_PER_CONNECTION, PendingAction, PendingCommand,
};
use aoe_core::{EntityId, WorldPosition};
use aoe_protocol::{CommandResult, GameplayServerMessage};
use std::time::Instant;

/// Order budget of an AI slot (humans keep 10 per second, burst 20).
const AI_ORDERS_PER_SECOND: f64 = 100.0;
const AI_ORDER_BURST: f64 = 200.0;
const AI_PENDING_COMMANDS: usize = 256;

#[path = "commands/apply.rs"]
mod apply;
#[path = "commands/tests/attack_move.rs"]
#[cfg(test)]
mod attack_move_tests;
#[path = "commands/tests/auto_scout.rs"]
#[cfg(test)]
mod auto_scout_tests;
#[path = "commands/tests/delete.rs"]
#[cfg(test)]
mod delete_tests;
#[path = "commands/tests/detach.rs"]
#[cfg(test)]
mod detach_tests;
#[path = "commands/formations.rs"]
mod formations;
#[path = "commands/garrison.rs"]
mod garrison;
#[path = "commands/navy.rs"]
mod navy;
#[path = "commands/queue.rs"]
mod queue;
pub(super) use apply::apply;

impl GameplayService {
    pub async fn queue_vassal_development(
        &self,
        session_id: u64,
        sequence: u64,
        vassal_id: EntityId,
        enabled: bool,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::VassalDevelopment { vassal_id, enabled },
        )
        .await;
    }
    pub async fn queue_stance(
        &self,
        session_id: u64,
        sequence: u64,
        mut unit_ids: Vec<EntityId>,
        stance: aoe_core::CombatStance,
    ) {
        unit_ids.truncate(aoe_core::MAX_FORMATION_MEMBERS + 1);
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Stance { unit_ids, stance },
        )
        .await;
    }
    pub async fn queue_auto_scout(
        &self,
        session_id: u64,
        sequence: u64,
        entity_id: EntityId,
        enabled: bool,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::AutoScout { entity_id, enabled },
        )
        .await;
    }
    pub async fn queue_steward_dismiss(
        &self,
        session_id: u64,
        sequence: u64,
        steward_id: EntityId,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::StewardDismiss { steward_id },
        )
        .await;
    }
    pub async fn queue_steward_settings(
        &self,
        session_id: u64,
        sequence: u64,
        steward_id: EntityId,
        weights: [u8; 4],
        cap: u8,
        training: bool,
    ) {
        let settings = aoe_simulation::StewardSettings {
            weights,
            cap,
            training,
        };
        self.queue_command(
            session_id,
            sequence,
            PendingAction::StewardSettings {
                steward_id,
                settings,
            },
        )
        .await;
    }
    pub async fn queue_farm_reseed(
        &self,
        session_id: u64,
        sequence: u64,
        building_id: EntityId,
        add: bool,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::FarmReseed { building_id, add },
        )
        .await;
    }
    pub async fn queue_set_rally(
        &self,
        session_id: u64,
        sequence: u64,
        building_id: EntityId,
        target: aoe_protocol::RallyTarget,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::SetRally {
                building_id,
                target,
            },
        )
        .await;
    }

    pub async fn queue_delete(&self, session_id: u64, sequence: u64, entity_ids: Vec<EntityId>) {
        self.queue_command(session_id, sequence, PendingAction::Delete { entity_ids })
            .await;
    }

    pub async fn queue_stop(&self, session_id: u64, sequence: u64, entity_id: EntityId) {
        self.queue_command(session_id, sequence, PendingAction::Stop { entity_id })
            .await;
    }

    pub async fn queue_general_move_alone(
        &self,
        session_id: u64,
        sequence: u64,
        entity_id: EntityId,
        destination: WorldPosition,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::GeneralMoveAlone {
                entity_id,
                destination,
            },
        )
        .await;
    }

    pub async fn queue_general_training(
        &self,
        session_id: u64,
        sequence: u64,
        entity_id: EntityId,
        enabled: bool,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::GeneralTraining { entity_id, enabled },
        )
        .await;
    }

    pub async fn queue_general_mix(
        &self,
        session_id: u64,
        sequence: u64,
        entity_id: EntityId,
        mix: [u8; 5],
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::GeneralMix { entity_id, mix },
        )
        .await;
    }

    pub async fn queue_hunt(
        &self,
        session_id: u64,
        sequence: u64,
        mut unit_ids: Vec<EntityId>,
        animal_id: EntityId,
    ) {
        // Keep one excess id so the simulation rejects oversized typed calls,
        // while preserving the shared authority, sequence and queue checks.
        unit_ids.truncate(aoe_protocol::MAX_BUILD_GROUP + 1);
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Hunt {
                unit_ids,
                animal_id,
            },
        )
        .await;
    }
    pub async fn queue_move(
        &self,
        session_id: u64,
        sequence: u64,
        entity_id: EntityId,
        destination: WorldPosition,
    ) {
        self.queue_move_order(session_id, sequence, entity_id, destination, false)
            .await;
    }
    /// A replacing move; `attack_move` fights enemies met on the way.
    pub async fn queue_move_order(
        &self,
        session_id: u64,
        sequence: u64,
        entity_id: EntityId,
        destination: WorldPosition,
        attack_move: bool,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Move {
                entity_id,
                destination,
                queued: false,
                attack_move,
            },
        )
        .await;
    }
    pub async fn queue_waypoint(
        &self,
        session_id: u64,
        sequence: u64,
        entity_id: EntityId,
        destination: WorldPosition,
        attack_move: bool,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Move {
                entity_id,
                destination,
                queued: true,
                attack_move,
            },
        )
        .await;
    }
    pub async fn queue_gather(
        &self,
        session_id: u64,
        sequence: u64,
        entity_id: EntityId,
        resource_id: u64,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Gather {
                entity_id,
                resource_id,
            },
        )
        .await;
    }
    pub async fn queue_attack(
        &self,
        session_id: u64,
        sequence: u64,
        entity_id: EntityId,
        target_id: EntityId,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Attack {
                entity_id,
                target_id,
            },
        )
        .await;
    }
    pub async fn queue_train(
        &self,
        session_id: u64,
        sequence: u64,
        building_id: EntityId,
        kind: aoe_protocol::UnitKind,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Train { building_id, kind },
        )
        .await;
    }
    pub async fn queue_cancel_training(
        &self,
        session_id: u64,
        sequence: u64,
        building_id: EntityId,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::CancelTraining { building_id },
        )
        .await;
    }
    pub async fn queue_research(
        &self,
        session_id: u64,
        sequence: u64,
        building_id: EntityId,
        tech: aoe_protocol::Tech,
    ) {
        let action = PendingAction::Research { building_id, tech };
        self.queue_command(session_id, sequence, action).await;
    }
    pub async fn queue_cancel_research(
        &self,
        session_id: u64,
        sequence: u64,
        building_id: EntityId,
    ) {
        let action = PendingAction::CancelResearch { building_id };
        self.queue_command(session_id, sequence, action).await;
    }
    pub async fn queue_build(
        &self,
        session_id: u64,
        sequence: u64,
        builder_ids: Vec<EntityId>,
        kind: aoe_protocol::BuildingKind,
        origin: aoe_core::TileCoord,
        queued: bool,
    ) {
        let action = PendingAction::Build {
            builder_ids,
            kind,
            origin,
            queued,
        };
        self.queue_command(session_id, sequence, action).await;
    }
    pub async fn queue_build_bridge(
        &self,
        session_id: u64,
        sequence: u64,
        builder_ids: Vec<EntityId>,
        from: aoe_core::TileCoord,
        to: aoe_core::TileCoord,
        queued: bool,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::BuildBridge {
                builder_ids,
                from,
                to,
                queued,
            },
        )
        .await;
    }
    pub async fn queue_build_wall(
        &self,
        session_id: u64,
        sequence: u64,
        builder_ids: Vec<EntityId>,
        kind: aoe_protocol::BuildingKind,
        (from, to): (aoe_core::TileCoord, aoe_core::TileCoord),
        queued: bool,
    ) {
        let action = PendingAction::BuildWall {
            builder_ids,
            kind,
            from,
            to,
            queued,
        };
        self.queue_command(session_id, sequence, action).await;
    }
    pub async fn queue_construct(
        &self,
        session_id: u64,
        sequence: u64,
        builder_ids: Vec<EntityId>,
        building_id: EntityId,
    ) {
        let action = PendingAction::Construct {
            builder_ids,
            building_id,
        };
        self.queue_command(session_id, sequence, action).await;
    }
    pub async fn queue_cancel_construction(
        &self,
        session_id: u64,
        sequence: u64,
        building_id: EntityId,
    ) {
        let action = PendingAction::CancelConstruction { building_id };
        self.queue_command(session_id, sequence, action).await;
    }
    pub async fn queue_farm(
        &self,
        session_id: u64,
        sequence: u64,
        entity_id: EntityId,
        farm_id: EntityId,
    ) {
        let action = PendingAction::Farm { entity_id, farm_id };
        self.queue_command(session_id, sequence, action).await;
    }
    pub async fn queue_market_buy(
        &self,
        session_id: u64,
        sequence: u64,
        building_id: EntityId,
        resource: aoe_protocol::TradeResource,
    ) {
        self.queue_market_trade(session_id, sequence, building_id, resource, true, 1)
            .await;
    }
    pub async fn queue_market_sell(
        &self,
        session_id: u64,
        sequence: u64,
        building_id: EntityId,
        resource: aoe_protocol::TradeResource,
    ) {
        self.queue_market_trade(session_id, sequence, building_id, resource, false, 1)
            .await;
    }
    pub async fn queue_market_trade(
        &self,
        session_id: u64,
        sequence: u64,
        building_id: EntityId,
        resource: aoe_protocol::TradeResource,
        buy: bool,
        lots: u32,
    ) {
        let action = PendingAction::MarketTrade {
            building_id,
            resource,
            buy,
            lots,
        };
        self.queue_command(session_id, sequence, action).await;
    }
}

#[path = "commands/tests.rs"]
#[cfg(test)]
mod tests;
