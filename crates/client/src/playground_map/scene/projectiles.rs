use super::*;
use crate::unit_visuals::Pose;
use aoe_protocol::ProjectileKind;
use aoe_rendering::{SceneUnit, SceneUnitKind};
#[path = "projectiles/particles.rs"]
mod particles;

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

fn native_heading(from: [f64; 2], to: [f64; 2]) -> u16 {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let angle = (dx + dy).atan2(dx - dy);
    (angle * 8.0 / std::f64::consts::PI)
        .round()
        .rem_euclid(16.0) as u16
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
    let hz = f64::from(client.config.tick_hz.max(1));
    if let Some(aoe_protocol::NativeAction::AttackStarted(started)) = unit.native_action
        && aoe_assets::catalog::units::release_frame(pose).is_some()
    {
        return crate::unit_visuals::native_swing_clock(pose, live - started.0 as f64, hz);
    }
    crate::unit_visuals::pose_clock(
        pose,
        unit.kind,
        (client.village.last_shot.get(&unit.id).copied(), live, hz),
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
    let start = units.len();
    for shot in &client.village.projectiles {
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
        let pose = match shot.kind {
            ProjectileKind::Stone => Pose::Stone,
            ProjectileKind::Arrow => Pose::Arrow,
            ProjectileKind::Bolt => Pose::Bolt,
            ProjectileKind::Javelin => Pose::Javelin,
            ProjectileKind::Native(id) => {
                let Some(pose) = aoe_assets::catalog::units::projectile_pose(id) else {
                    continue;
                };
                pose
            }
        };
        if pose == Pose::FireStream {
            continue;
        }
        let frame = if matches!(pose, Pose::Arrow | Pose::Javelin | Pose::Bolt) {
            u16::from(flight_facing(from, to))
        } else if matches!(
            pose,
            Pose::GalleyArrow
                | Pose::NativeArrow
                | Pose::ScorpionBolt
                | Pose::HeavyScorpionBolt
                | Pose::BoardingHook
        ) {
            let source = aoe_assets::catalog::units::SPRITES[pose as usize];
            // The DAT direction rows for the arrow and hook contain pitch
            // cells with level flight in the center. Bolts use one cell.
            native_heading(from, to) | (u16::from(source.stride / 2) << 4)
        } else {
            crate::unit_visuals::packed_frame(
                pose,
                0,
                elapsed * 1000.0 / f64::from(client.config.tick_hz.max(1)),
                None,
                now,
                client.config.tick_hz,
            )
        };
        let rise = match shot.kind {
            ProjectileKind::Native(id) => {
                4.0 * fraction
                    * (1.0 - fraction)
                    * distance
                    * aoe_assets::catalog::units::projectile_arc(id)
            }
            _ => arc(fraction, distance),
        };
        units.push(
            SceneUnit {
                id: particles::scene_id(shot.id, false),
                kind: SceneUnitKind::actor(pose as u16),
                position,
                visual: aoe_rendering::SceneUnitVisual::new(true).with_selected(false),
                facing: 0,
                elevation_meters: (scene
                    .height(position)
                    .unwrap_or_else(|| elevation_at_world(client, position))
                    + 1.0
                    + rise) as f32,
            }
            .with_actor_frame(frame),
        );
    }
    particles::append(client, scene, units, now);
    units[start..].sort_unstable_by_key(|u| u.id);
}

#[cfg(test)]
#[path = "tests/projectiles.rs"]
mod tests;
