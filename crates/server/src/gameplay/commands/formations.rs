use super::*;

impl GameplayService {
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
