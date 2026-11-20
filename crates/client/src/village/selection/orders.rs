use super::{Client, orders::can_form};
use aoe_core::{EntityId, WorldPosition};
use aoe_protocol::GameplayClientMessage as Message;

pub(super) fn movement(
    client: &Client,
    mut unit_ids: Vec<EntityId>,
    destination: WorldPosition,
    queued: bool,
) -> Option<Message> {
    unit_ids.sort_unstable();
    unit_ids.dedup();
    let sequence = client.next_sequence;
    let formation = client.village.formation;
    let attack_move = client.village.attack_move;
    Some(match unit_ids.len() {
        0 => return None,
        1 => {
            let entity_id = unit_ids[0];
            let unit = client.units.get(&entity_id);
            let attack_move = attack_move && unit.is_some();
            if queued && unit.is_some() {
                Message::QueueMoveOrder {
                    sequence,
                    entity_id,
                    destination,
                    attack_move,
                }
            } else {
                Message::MoveOrder {
                    sequence,
                    entity_id,
                    destination,
                    attack_move,
                }
            }
        }
        _ if unit_ids
            .iter()
            .all(|id| client.units.get(id).is_some_and(|u| can_form(u.kind))) =>
        {
            Message::FormationOrder {
                sequence,
                unit_ids,
                formation,
                destination: Some(destination),
                queued,
                attack_move,
            }
        }
        _ => Message::MoveSelectionOrder {
            sequence,
            unit_ids,
            formation,
            destination,
            queued,
            attack_move,
        },
    })
}
