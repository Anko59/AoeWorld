use super::economy;
use aoe_core::{EntityId, MAX_MOVE_DESTINATIONS, WorldPosition};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MoveQueue {
    pub entity_id: EntityId,
    #[serde(deserialize_with = "destinations")]
    pub destinations: Vec<WorldPosition>,
}

fn destinations<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<WorldPosition>, D::Error> {
    economy::bounded::<D, _, MAX_MOVE_DESTINATIONS>(d)
}

pub(super) fn valid_queue(q: &MoveQueue) -> bool {
    !q.destinations.is_empty()
        && q.destinations.len() <= MAX_MOVE_DESTINATIONS
        && q.destinations.iter().all(|p| p.x >= 0 && p.y >= 0)
}
