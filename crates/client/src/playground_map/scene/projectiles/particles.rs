//! One-shot muzzle and impact animations of observed native projectiles.
use super::*;

pub(super) fn scene_id(shot: u64, particle: bool) -> aoe_core::EntityId {
    aoe_core::EntityId(u32::MAX - ((shot as u32 & 0x3fff_ffff) * 2 + u32::from(particle)))
}

pub(super) fn append(client: &Client, scene: &PreparedScene, out: &mut Vec<SceneUnit>, now: f64) {
    let hz = f64::from(client.config.tick_hz.max(1));
    for shot in client.village.projectile_particles.values() {
        let ProjectileKind::Native(id) = shot.kind else {
            continue;
        };
        if aoe_assets::catalog::units::projectile_pose(id) == Some(Pose::FireStream) {
            out.extend(fire_stream(client, scene, shot, id, now, hz));
        }
        let (muzzle, impact) = aoe_assets::catalog::units::projectile_effects(id);
        let (pose, at, elapsed) = if now >= shot.impact.0 as f64 {
            (impact, shot.to, now - shot.impact.0 as f64)
        } else if shot.attacker.is_some() {
            (muzzle, shot.from, now - shot.launched.0 as f64)
        } else {
            continue;
        };
        let Some(pose) = pose else { continue };
        let animation = aoe_assets::catalog::units::SPRITES[pose as usize];
        let frame = elapsed * 1000.0 / (hz * animation.frame_millis());
        let tile = at.tile_floor();
        if frame < 0.0
            || frame >= f64::from(animation.stride)
            || client.fog.state([tile.x, tile.y]) != 2
            || client
                .art
                .actors
                .get(pose as usize)
                .is_none_or(Vec::is_empty)
        {
            continue;
        }
        let position = at.as_tiles();
        let (from, to) = (shot.from.as_tiles(), shot.to.as_tiles());
        out.push(
            SceneUnit {
                id: scene_id(shot.id, true),
                kind: SceneUnitKind::actor(pose as u16),
                position,
                visual: aoe_rendering::SceneUnitVisual::new(true),
                facing: 0,
                elevation_meters: scene
                    .height(position)
                    .unwrap_or_else(|| elevation_at_world(client, position))
                    as f32,
            }
            .with_actor_frame(native_heading(from, to) | ((frame as u16) << 4)),
        );
    }
}

fn fire_stream(
    client: &Client,
    scene: &PreparedScene,
    shot: &aoe_protocol::ProjectileState,
    id: u16,
    now: f64,
    hz: f64,
) -> Option<SceneUnit> {
    let pose = Pose::FireStream;
    let source = aoe_assets::catalog::units::SPRITES[pose as usize];
    let age = now - shot.launched.0 as f64;
    let cell_ticks = source.frame_millis() * hz / 1000.0;
    let life = f64::from(source.stride) * cell_ticks;
    if !(0.0..life).contains(&age) {
        return None;
    }
    // Every shot emits a puff that travels for the installed emitter's lifetime.
    // Short-range hits can finish while successive puffs still fill the gap.
    let fraction = age / life.max(shot.impact.0.saturating_sub(shot.launched.0) as f64);
    let (from, to) = (shot.from.as_tiles(), shot.to.as_tiles());
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let position = [from[0] + dx * fraction, from[1] + dy * fraction];
    if client
        .fog
        .state([position[0].floor() as i32, position[1].floor() as i32])
        != 2
    {
        return None;
    }
    let rise = 4.0
        * fraction
        * (1.0 - fraction)
        * dx.hypot(dy)
        * aoe_assets::catalog::units::projectile_arc(id);
    Some(
        SceneUnit {
            id: scene_id(shot.id, false),
            kind: SceneUnitKind::actor(pose as u16),
            position,
            visual: aoe_rendering::SceneUnitVisual::new(true),
            facing: 0,
            elevation_meters: (scene
                .height(position)
                .unwrap_or_else(|| elevation_at_world(client, position))
                + 1.0
                + rise) as f32,
        }
        .with_actor_frame(((age / cell_ticks) as u16) << 4),
    )
}
