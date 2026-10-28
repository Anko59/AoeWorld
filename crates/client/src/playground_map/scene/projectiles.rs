use super::*;
use crate::unit_visuals::Pose;
use aoe_protocol::ProjectileKind;
use aoe_rendering::{SceneUnit, SceneUnitKind};

fn facing(dx: f64, dy: f64) -> u8 {
    let sx = dx - dy;
    let sy = dx + dy;
    if sx.abs() > 2.0 * sy.abs() {
        if sx > 0.0 { 2 } else { 6 }
    } else if sy.abs() > 2.0 * sx.abs() {
        if sy > 0.0 { 0 } else { 4 }
    } else if sx >= 0.0 {
        if sy >= 0.0 { 1 } else { 3 }
    } else if sy >= 0.0 {
        7
    } else {
        5
    }
}

pub(super) fn append(client: &Client, scene: &PreparedScene, units: &mut Vec<SceneUnit>) {
    // Synthetic scene ids descend as shot ids rise; reverse iteration keeps
    // the entity-id ordering required by the last-presented-frame cache.
    for shot in client.village.projectiles.iter().rev() {
        if client.village.tick >= shot.impact.0 {
            continue;
        }
        let elapsed = client.village.tick.saturating_sub(shot.launched.0) as f64;
        let duration = shot.impact.0.saturating_sub(shot.launched.0).max(1) as f64;
        let fraction = (elapsed / duration).clamp(0.0, 1.0);
        let from = shot.from.as_tiles();
        let to = shot.to.as_tiles();
        let position = [
            from[0] + (to[0] - from[0]) * fraction,
            from[1] + (to[1] - from[1]) * fraction,
        ];
        if client
            .fog
            .state([position[0].floor() as i32, position[1].floor() as i32])
            != 2
        {
            continue;
        }
        // A bounded parabola keeps the launch and impact heights unchanged.
        let distance = (to[0] - from[0]).hypot(to[1] - from[1]);
        let arc = 4.0 * fraction * (1.0 - fraction) * (distance * 0.25).min(8.0);
        let pose = if shot.kind == ProjectileKind::Stone {
            Pose::Stone
        } else if shot.kind == ProjectileKind::Arrow {
            Pose::Arrow
        } else {
            Pose::Javelin
        };
        units.push(SceneUnit {
            id: aoe_core::EntityId(u32::MAX.saturating_sub(shot.id as u32)),
            kind: SceneUnitKind::actor(pose as u8),
            position,
            visual: aoe_rendering::SceneUnitVisual::new(true).with_selected(false),
            facing: facing(to[0] - from[0], to[1] - from[1]),
            elevation_meters: scene
                .height(position)
                .unwrap_or_else(|| elevation_at_world(client, position))
                + 1.0
                + arc,
        });
    }
}
