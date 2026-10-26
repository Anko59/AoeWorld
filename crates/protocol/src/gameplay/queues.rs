use super::{Error, MAX_SUBSCRIBED_UNITS, ServerMessage, economy};
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

pub(super) fn validate(message: &ServerMessage) -> Result<(), Error> {
    if let ServerMessage::MoveQueues {
        revision, queues, ..
    } = message
        && (*revision == 0
            || queues.len() > MAX_SUBSCRIBED_UNITS
            || !queues.windows(2).all(|q| q[0].entity_id < q[1].entity_id)
            || queues.iter().any(|q| {
                q.destinations.is_empty()
                    || q.destinations.len() > MAX_MOVE_DESTINATIONS
                    || q.destinations.iter().any(|p| p.x < 0 || p.y < 0)
            }))
    {
        return Err(Error::InvalidEconomyState);
    }
    Ok(())
}
