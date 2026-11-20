use super::economy;
use aoe_core::{EntityId, MAX_MOVE_DESTINATIONS, WorldPosition};
use serde::{Deserialize, Deserializer, Serialize};

const _: () = assert!(MAX_MOVE_DESTINATIONS <= u16::BITS as usize);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MoveQueue {
    pub entity_id: EntityId,
    #[serde(deserialize_with = "destinations")]
    pub destinations: Vec<WorldPosition>,
    /// Bit i describes the leg ending at destinations[i].
    pub attack_move_mask: u16,
    /// Equal nonzero IDs refer to the same player order. Zero is unit-local.
    #[serde(deserialize_with = "order_ids")]
    pub order_ids: Vec<u32>,
}

impl MoveQueue {
    pub fn is_attack_move(&self, index: usize) -> bool {
        index < u16::BITS as usize && self.attack_move_mask & (1 << index) != 0
    }
}

fn destinations<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<WorldPosition>, D::Error> {
    economy::bounded::<D, _, MAX_MOVE_DESTINATIONS>(d)
}

fn order_ids<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u32>, D::Error> {
    economy::bounded::<D, _, MAX_MOVE_DESTINATIONS>(d)
}

pub(super) fn valid_queue(q: &MoveQueue) -> bool {
    !q.destinations.is_empty()
        && q.destinations.len() <= MAX_MOVE_DESTINATIONS
        && q.order_ids.len() == q.destinations.len()
        && (u32::from(q.attack_move_mask) >> q.destinations.len()) == 0
        && q.destinations.iter().all(|p| p.x >= 0 && p.y >= 0)
}
