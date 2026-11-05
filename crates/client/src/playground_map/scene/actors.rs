//! Actor timelines and native headings, shared by the rendered scene.
use super::*;

/// Whether `unit` has covered under a fifth of its native stride over the last
/// few ticks (about 0.6 s, at most six samples of history).
pub(super) fn stalled(client: &Client, unit: &aoe_protocol::GameplayUnitState) -> bool {
    let hz = f64::from(client.config.tick_hz.max(1));
    let ticks = (hz * 0.6).clamp(2.0, 6.0);
    unit.hp > 0
        && client
            .presentation
            .travel_over(unit.id, ticks)
            .is_some_and(|travelled| {
                crate::unit_visuals::is_stalled(unit.kind, travelled, ticks / hz)
            })
}

pub(super) fn animation_frame(
    client: &Client,
    unit: &aoe_protocol::GameplayUnitState,
    pose: crate::unit_visuals::Pose,
    travelled: f64,
) -> u16 {
    // A walking unit's frame follows the distance it has really covered.
    let walking = unit.moving && unit.hp > 0 && unit.death_tick.is_none();
    let clock = if walking {
        crate::unit_visuals::walk_clock(unit.kind, travelled)
    } else {
        super::projectiles::pose_clock(client, unit, pose)
    };
    let mut frame = crate::unit_visuals::packed_frame(
        pose,
        unit.facing,
        clock,
        unit.death_tick,
        client
            .presentation
            .live_tick(client.last_frame)
            .unwrap_or(client.village.tick as f64),
        client.config.tick_hz,
    );
    if !matches!(
        pose,
        crate::unit_visuals::Pose::Arrow | crate::unit_visuals::Pose::Javelin
    ) && let Some(heading) = client.presentation.native_heading(*unit)
    {
        frame = (frame & !15) | u16::from(heading);
    }
    frame
}
