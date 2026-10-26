//! Naval orders use the same bounded authenticated queue as other commands.
use super::*;
impl GameplayService {
    pub async fn queue_load(
        &self,
        session_id: u64,
        sequence: u64,
        mut unit_ids: Vec<EntityId>,
        transport_id: EntityId,
    ) {
        unit_ids.truncate(aoe_protocol::MAX_BUILD_GROUP + 1);
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Load {
                unit_ids,
                transport_id,
            },
        )
        .await;
    }
    pub async fn queue_unload(
        &self,
        session_id: u64,
        sequence: u64,
        transport_id: EntityId,
        destination: aoe_core::TileCoord,
    ) {
        self.queue_command(
            session_id,
            sequence,
            PendingAction::Unload {
                transport_id,
                destination,
            },
        )
        .await;
    }
}
