//! Presentation and ordinary per-unit orders for authoritative cavalry scouting.
use super::{Client, orders::send, owner};
use aoe_protocol::GameplayClientMessage as Message;

#[inline(never)]
pub(super) fn state(client: &Client) -> [f64; 2] {
    let mut counts = [0.0, 0.0];
    for id in &client.selected.units {
        if let Some(unit) = client.units.get(id)
            && unit.player == owner(client)
            && unit.hp > 0
            && unit.kind.is_cavalry()
        {
            counts[0] += 1.0;
            counts[1] += f64::from(u8::from(unit.auto_scout));
        }
    }
    counts
}

pub(super) fn toggle(client: &mut Client) {
    let counts = state(client);
    let enabled = counts[1] < counts[0];
    for id in client.selected.units.clone() {
        if client.units.get(&id).is_some_and(|unit| {
            unit.player == owner(client) && unit.hp > 0 && unit.kind.is_cavalry()
        }) {
            let sequence = client.next_sequence;
            send(
                client,
                Message::AutoScoutOrder {
                    sequence,
                    entity_id: id,
                    enabled,
                },
            );
        }
    }
}
