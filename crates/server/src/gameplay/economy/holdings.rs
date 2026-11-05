use aoe_core::Tick;
use aoe_protocol::{GameplayServerMessage, HoldingSpan, MAX_HOLDING_SPANS};
use aoe_simulation::LandControl;

pub(crate) fn message(land: &LandControl, revision: u64, tick: Tick) -> GameplayServerMessage {
    let rows: Vec<_> = land.shapes.chunk_by(|a, b| a.y == b.y).collect();
    let stride = land.shapes.len().div_ceil(MAX_HOLDING_SPANS).max(1);
    let mut shapes = Vec::new();
    for row in rows.iter().step_by(stride) {
        if shapes.len() + row.len() > MAX_HOLDING_SPANS {
            continue;
        }
        shapes.extend(row.iter().map(|s| HoldingSpan {
            player: s.player,
            y: s.y,
            x0: s.x0,
            x1: s.x1,
        }));
    }
    GameplayServerMessage::Holdings {
        revision,
        tick,
        complete: shapes.len() == land.shapes.len(),
        shapes,
    }
}
