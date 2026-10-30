//! Actor timelines and native headings, shared by the rendered scene.
use super::*;

pub(super) fn animation_frame(
    client: &Client,
    unit: &aoe_protocol::GameplayUnitState,
    pose: crate::unit_visuals::Pose,
) -> u16 {
    let mut frame = crate::unit_visuals::packed_frame(
        pose,
        unit.facing,
        super::projectiles::pose_clock(client, unit, pose),
        unit.death_tick,
        client
            .presentation
            .live_tick(client.last_frame)
            .unwrap_or(client.village.tick as f64),
        client.config.tick_hz,
    );
    if unit.kind == aoe_protocol::UnitKind::General
        && let Some(heading) = client.presentation.native_heading(*unit)
    {
        frame = (frame & !15) | u16::from(heading);
    }
    frame
}
