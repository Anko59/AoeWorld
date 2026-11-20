use super::*;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct SelectionMove {
    pub unit_ids: Vec<EntityId>,
    pub formation: aoe_core::Formation,
    pub destination: WorldPosition,
    pub queued: bool,
    pub attack_move: bool,
}

impl GameplayService {
    pub(crate) async fn queue_move_selection(
        &self,
        session: u64,
        sequence: u64,
        order: SelectionMove,
    ) {
        self.queue_command(session, sequence, PendingAction::MoveSelection(order))
            .await;
    }

    pub(crate) async fn queue_attack_selection(
        &self,
        session: u64,
        sequence: u64,
        entity_ids: Vec<EntityId>,
        target_id: EntityId,
    ) {
        self.queue_command(
            session,
            sequence,
            PendingAction::AttackSelection {
                entity_ids,
                target_id,
            },
        )
        .await;
    }

    pub(crate) async fn queue_train_batch(
        &self,
        session: u64,
        sequence: u64,
        building_ids: Vec<EntityId>,
        kind: aoe_protocol::UnitKind,
    ) {
        self.queue_command(
            session,
            sequence,
            PendingAction::TrainBatch { building_ids, kind },
        )
        .await;
    }

    pub async fn queue_patrol(
        &self,
        session_id: u64,
        sequence: u64,
        entity_id: EntityId,
        destination: WorldPosition,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Patrol(entity_id, destination),
        )
        .await;
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn queue_formation(
        &self,
        session_id: u64,
        sequence: u64,
        mut unit_ids: Vec<EntityId>,
        formation: aoe_core::Formation,
        destination: Option<WorldPosition>,
        queued: bool,
        attack_move: bool,
    ) {
        unit_ids.truncate(aoe_core::MAX_FORMATION_MEMBERS + 1);
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Formation {
                unit_ids,
                formation,
                destination,
                queued,
                attack_move,
            },
        )
        .await;
    }
}
