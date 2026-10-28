use super::*;
use crate::unit_visuals::Pose;
use aoe_protocol::ProjectileKind;
use aoe_rendering::{SceneUnit, SceneUnitKind};

/// Metres of arc height per tile of flight, capped, as drawn below.
const ARC_PER_DISTANCE: f64 = 0.25;
const ARC_LIMIT_METERS: f64 = 8.0;

/// The arc height at `fraction` of a flight over `distance` tiles. It keeps
/// the launch and impact heights unchanged.
fn arc(fraction: f64, distance: f64) -> f64 {
    4.0 * fraction * (1.0 - fraction) * (distance * ARC_PER_DISTANCE).min(ARC_LIMIT_METERS)
}

/// The packed sprite cell along the flight `from -> to`: the direction of
/// its ground path as drawn on the isometric screen.
pub(super) fn flight_facing(from: [f64; 2], to: [f64; 2]) -> u8 {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    // The sprite directions are drawn for a 2:1 projection, so a tile step
    // (dx, dy) reads (dx - dy, dx + dy) in unsquished screen axes.
    aoe_assets::catalog::units::projectile_facing(dx - dy, dx + dy)
}

/// The animation clock of `unit` in `pose`; see `unit_visuals::pose_clock`.
pub(super) fn pose_clock(
    client: &Client,
    unit: &aoe_protocol::GameplayUnitState,
    pose: Pose,
) -> f64 {
    let live = client
        .presentation
        .live_tick(client.last_frame)
        .unwrap_or(client.village.tick as f64);
    crate::unit_visuals::pose_clock(
        pose,
        unit.kind,
        (
            client.village.last_shot.get(&unit.id).copied(),
            live,
            f64::from(client.config.tick_hz.max(1)),
        ),
        client.last_frame,
    )
}

pub(super) fn append(client: &Client, scene: &PreparedScene, units: &mut Vec<SceneUnit>) {
    // The newest tick advanced by the time since it arrived: the clock that
    // phases shooters' attack animations, so a shot leaves on their release
    // frame. A shot first seen in fog keeps the tick it was seen.
    let now = client
        .presentation
        .live_tick(client.last_frame)
        .unwrap_or(client.village.tick as f64)
        .max(client.village.tick as f64);
    // Synthetic scene ids descend as shot ids rise; reverse iteration keeps
    // the entity-id ordering required by the last-presented-frame cache.
    for shot in client.village.projectiles.iter().rev() {
        if now >= shot.impact.0 as f64 {
            continue;
        }
        let elapsed = (now - shot.launched.0 as f64).max(0.0);
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
        let distance = (to[0] - from[0]).hypot(to[1] - from[1]);
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
            facing: if pose == Pose::Stone {
                0
            } else {
                flight_facing(from, to)
            },
            elevation_meters: scene
                .height(position)
                .unwrap_or_else(|| elevation_at_world(client, position))
                + 1.0
                + arc(fraction, distance),
        });
    }
}

#[cfg(test)]
#[path = "tests/projectiles.rs"]
mod tests;
