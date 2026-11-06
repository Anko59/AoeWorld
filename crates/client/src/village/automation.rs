//! Presentation and ordinary per-unit orders for authoritative cavalry scouting.
use super::{Client, orders::send, owner};
use aoe_protocol::GameplayClientMessage as Message;

#[inline(never)]
pub(super) fn state(client: &Client) -> [f64; 2] {
    tally(client.selected.units.iter().filter_map(|id| {
        let unit = client.units.get(id)?;
        (unit.player == owner(client) && unit.hp > 0).then_some((unit.kind, unit.auto_scout))
    }))
}

/// `[eligible, scouting]` over living owned (kind, auto_scout) selections.
pub(super) fn tally(units: impl IntoIterator<Item = (aoe_protocol::UnitKind, bool)>) -> [f64; 2] {
    let mut counts = [0.0, 0.0];
    for (kind, scouting) in units {
        if kind.can_auto_scout() {
            counts[0] += 1.0;
            counts[1] += f64::from(u8::from(scouting));
        }
    }
    counts
}

pub(super) fn toggle(client: &mut Client) {
    let counts = state(client);
    let enabled = counts[1] < counts[0];
    for id in client.selected.units.clone() {
        if client.units.get(&id).is_some_and(|unit| {
            unit.player == owner(client) && unit.hp > 0 && unit.kind.can_auto_scout()
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
