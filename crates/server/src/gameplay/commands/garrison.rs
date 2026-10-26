use super::*;

impl GameplayService {
    pub async fn queue_garrison(
        &self,
        session_id: u64,
        sequence: u64,
        entity_id: EntityId,
        building_id: EntityId,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Garrison {
                entity_id,
                building_id,
            },
        )
        .await;
    }
    pub async fn queue_ungarrison(
        &self,
        session_id: u64,
        sequence: u64,
        building_id: EntityId,
        entity_id: Option<EntityId>,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Ungarrison {
                building_id,
                entity_id,
            },
        )
        .await;
    }
}
