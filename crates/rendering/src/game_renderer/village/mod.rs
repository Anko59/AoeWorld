//! Reviewed native villager rows and the multipart Dark Age Town Center.
use super::*;
use crate::SceneUnitKind;

pub(super) fn frames(art: &GameArt, unit: SceneUnit) -> Option<(&[GameFrame], usize, bool)> {
    if let Some(index) = unit.kind.actor_slot() {
        let frames = art.actors.get(index)?.as_slice();
        let rows = if unit.kind == SceneUnitKind::Stone {
            1
        } else if unit.kind.native_directions() {
            16
        } else {
            5
        };
        let rows = art.actor_rows.get(index).copied().unwrap_or(rows);
        let stride = frames.len() / rows.max(1);
        return (stride > 0 && frames.len() == stride * rows).then_some((frames, stride, false));
    }
    None
}

#[inline(never)]
pub(super) fn marker(
    unit: SceneUnit,
    camera: SceneCamera,
    emit: &mut dyn FnMut((Sprite, GameFrame, f64, u64)),
) {
    let side = unit.kind.building_side();
    let building = side.is_some();
    let frame = GameFrame {
        stream: 0,
        atlas: crate::AtlasAddress {
            page: 2,
            uv: [
                0.0,
                0.0,
                1.0 / GAME_ATLAS_SIDE as f32,
                1.0 / GAME_ATLAS_SIDE as f32,
            ],
        },
        size: if building {
            [
                side.unwrap_or(1.0) as f32 * 30.0,
                side.unwrap_or(1.0) as f32 * 15.0,
            ]
        } else {
            [14.0, 32.0]
        },
        anchor: if building {
            [
                side.unwrap_or(1.0) as f32 * 15.0,
                side.unwrap_or(1.0) as f32 * 7.5,
            ]
        } else {
            [7.0, 30.0]
        },
    };
    if let Some(mut sprite) = scene_sprite(frame, unit.position, unit.elevation_meters, camera) {
        sprite.color = if building {
            [0.72, 0.58, 0.35, 1.0]
        } else if matches!(
            unit.kind,
            SceneUnitKind::NeutralSheepStand
                | SceneUnitKind::NeutralSheepWalk
                | SceneUnitKind::NeutralSheepDie
                | SceneUnitKind::NeutralSheepDecay
                | SceneUnitKind::DeerStand
                | SceneUnitKind::DeerWalk
                | SceneUnitKind::DeerDie
                | SceneUnitKind::DeerDecay
                | SceneUnitKind::BoarStand
                | SceneUnitKind::BoarWalk
                | SceneUnitKind::BoarAttack
                | SceneUnitKind::BoarDie
                | SceneUnitKind::BoarDecay
                | SceneUnitKind::WolfStand
                | SceneUnitKind::WolfWalk
                | SceneUnitKind::WolfRun
                | SceneUnitKind::WolfAttack
                | SceneUnitKind::WolfDie
                | SceneUnitKind::WolfDecay
        ) {
            // Missing native art must not make neutral wildlife look player-owned.
            [0.75, 0.75, 0.75, 1.0]
        } else {
            [0.25, 0.57, 0.96, 1.0]
        };
        if let Some(colour) = unit.visual.colour() {
            sprite.color = [
                f32::from(colour[0]) / 255.0,
                f32::from(colour[1]) / 255.0,
                f32::from(colour[2]) / 255.0,
                1.0,
            ];
        }
        emit((
            sprite.with_unit_contour(unit.visual.outline_colour()),
            scaled(frame, camera.zoom as f32),
            object_depth(WorldObject::Unit(unit)),
            u64::from(unit.id.0),
        ));
    }
}

// Drawing and visibility share one emitter body rather than cloning the
// multipart renderer for each callback in the browser WASM.
#[inline(never)]
pub(super) fn building(
    art: &GameArt,
    unit: SceneUnit,
    camera: SceneCamera,
    animation: usize,
    emit: &mut dyn FnMut((Sprite, GameFrame, f64, u64)),
) {
    if unit.visual.ghost() && unit.kind == SceneUnitKind::Farm {
        // Farms are terrain, not scaffolding. Draw their native crop diamonds
        // as translucent sprites without mutating the cached ground mesh.
        for y in -1..=1 {
            for x in -1..=1 {
                let position = [
                    unit.position[0] + f64::from(x),
                    unit.position[1] + f64::from(y),
                ];
                let tile = position.map(|v| v.floor() as i32);
                let native =
                    crate::surface_mesh::terrain_texture_frame(art, 17, tile).or_else(|| {
                        let mut frame = *art.buildings[5].first()?;
                        frame.atlas.uv[0] += frame.atlas.uv[2] * (x + 1) as f32 / 3.0;
                        frame.atlas.uv[1] += frame.atlas.uv[3] * (y + 1) as f32 / 3.0;
                        frame.atlas.uv[2] /= 3.0;
                        frame.atlas.uv[3] /= 3.0;
                        Some(frame)
                    });
                if let Some(mut frame) = native {
                    frame.size = [
                        aoe_core::ISO_TILE_WIDTH as f32,
                        aoe_core::ISO_TILE_HEIGHT as f32,
                    ];
                    frame.anchor = frame.size.map(|v| v / 2.0);
                    if let Some(mut sprite) =
                        scene_sprite(frame, position, unit.elevation_meters, camera)
                    {
                        sprite.pages[3] |= 128; // Crop the native terrain diamond on every backend.
                        emit((
                            sprite,
                            scaled(frame, camera.zoom as f32),
                            object_depth(WorldObject::Unit(unit)),
                            u64::from(unit.id.0),
                        ));
                    }
                }
            }
        }
        return;
    }
    let Some(building) = art
        .compositions
        .iter()
        .find(|b| Some(b.unit_id) == unit.kind.building_id())
    else {
        sprite_building(art, unit, camera, animation, emit);
        return;
    };
    let destroyed = unit.visual.moving() || unit.kind.rubble();
    let elapsed = if unit.visual.moving() {
        art.building_clocks
            .iter()
            .find(|(id, _)| *id == unit.id.0)
            .map_or(0.0, |(_, millis)| *millis)
    } else {
        f64::INFINITY
    };
    let destruction = building
        .states
        .iter()
        .find(|(role, _)| *role == 0)
        .map(|(_, p)| p);
    let destruction_ms = destruction
        .into_iter()
        .flatten()
        .map(|p| {
            p.frame_rate as f64
                * 1000.0
                * (p.animation.len().max(1) / usize::from(p.angles).max(1)) as f64
        })
        .fold(0.0, f64::max);
    let state = if destroyed {
        Some(if elapsed < destruction_ms { 0 } else { 1 })
    } else if matches!(
        unit.kind,
        SceneUnitKind::OpenPalisadeGate | SceneUnitKind::OpenStoneGate
    ) {
        Some(5)
    } else {
        None
    };
    let damage = if destroyed || unit.kind.is_foundation() || unit.visual.ghost() {
        None
    } else {
        usize::from(unit.visual.damage_stage())
            .checked_sub(1)
            .and_then(|i| building.damage.get(i))
    };
    let parts = if let Some(role) = state {
        building
            .states
            .iter()
            .find(|(r, _)| *r == role)
            .map_or(&building.parts, |(_, p)| p)
    } else if unit.kind.is_foundation() {
        &building.construction
    } else if let Some(replacement) = damage.filter(|d| d.apply_mode == 2) {
        &replacement.parts
    } else {
        &building.parts
    };
    if parts.is_empty() && damage.is_none_or(|d| d.parts.is_empty()) {
        if destroyed {
            return;
        }
        if unit.kind == SceneUnitKind::Farm
            && building.foundation_material == Some(17)
            && !art.foundation.0.is_empty()
        {
            return; // The native farm terrain is the complete visual building.
        }
        sprite_building(art, unit, camera, animation, emit);
        return;
    }
    // Import order is shadow layer, then isometric annex depth, preserving
    // stand/delta order within each unit. No rotation or mirroring is applied.
    let overlay = damage
        .filter(|d| d.apply_mode != 2)
        .into_iter()
        .flat_map(|d| &d.parts);
    for (part, is_overlay) in parts
        .iter()
        .map(|part| (part, false))
        .chain(overlay.map(|part| (part, true)))
    {
        let mut native = part.frame;
        if !part.animation.is_empty() {
            let angles = usize::from(part.angles).max(1);
            let count = part.animation.len() / angles;
            let angle = if unit.kind.is_foundation() || unit.kind == SceneUnitKind::PalisadeWall {
                usize::from(unit.facing).min(angles - 1)
            } else {
                unit.id.0 as usize % angles
            };
            let index = if unit.kind.is_foundation()
                || (unit.kind == SceneUnitKind::PalisadeWall && !is_overlay)
            {
                0
            } else if part.frame_rate > 0.0 {
                {
                    let clock = if destroyed { elapsed } else { animation as f64 };
                    let step = f64::from(part.frame_rate) * 1000.0;
                    let phase = (clock + step * 1e-5)
                        % (step * count as f64 + f64::from(part.replay_delay) * 1000.0);
                    let index = (phase / step + 1e-5) as usize;
                    if destroyed {
                        (clock / step) as usize
                    } else {
                        index
                    }
                    .min(count - 1)
                }
            } else {
                unit.id.0 as usize % count
            };
            native = part.animation[angle * count + index];
        }
        native.anchor[0] -= part.offset[0];
        native.anchor[1] -= part.offset[1];
        let frame = world_frame(native);
        if let Some(sprite) = scene_sprite(frame, unit.position, unit.elevation_meters, camera) {
            emit((
                sprite.with_owner(unit.visual),
                scaled(frame, camera.zoom as f32),
                object_depth(WorldObject::Unit(unit)),
                u64::from(unit.id.0),
            ));
        }
    }
}

#[cfg(test)]
mod tests;

/// Native anchors and player/shadow planes are shared by all backends.
#[inline(never)]
pub(super) fn sprite_building(
    art: &GameArt,
    unit: SceneUnit,
    camera: SceneCamera,
    animation: usize,
    emit: &mut dyn FnMut((Sprite, GameFrame, f64, u64)),
) {
    let animation = animation / 100;
    let Some(art_slot) = unit.kind.building_slot() else {
        marker(unit, camera, emit);
        return;
    };
    if art_slot == 5 {
        return;
    } // Farm is ground.
    let frames = art
        .buildings
        .get(usize::from(art_slot))
        .map_or(&[][..], |frames| frames.as_slice());
    if frames.is_empty() || (art_slot == 4 && frames.len() != 44) {
        marker(unit, camera, emit);
        return;
    }
    let index = if (9..=12).contains(&art_slot) || matches!(art_slot, 24 | 43) {
        usize::from(unit.facing).min(frames.len() - 1)
    } else if art_slot == 40 {
        animation % frames.len()
    } else if art_slot == 7 || art_slot == 71 {
        usize::from(unit.facing) % frames.len()
    } else {
        unit.id.0 as usize % frames.len()
    };
    let (indices, count) = if art_slot == 4 || (art_slot == 40 && frames.len() == 9) {
        ([0, 1 + animation % (frames.len() - 1)], 2)
    } else {
        ([index, 0], 1)
    };
    for index in indices.into_iter().take(count) {
        let frame = world_frame(frames[index]);
        if let Some(sprite) = scene_sprite(frame, unit.position, unit.elevation_meters, camera) {
            emit((
                if (65..=70).contains(&art_slot) {
                    sprite.with_unit_contour(unit.visual.outline_colour())
                } else {
                    sprite.with_owner(unit.visual)
                },
                scaled(frame, camera.zoom as f32),
                object_depth(WorldObject::Unit(unit)),
                u64::from(unit.id.0),
            ));
        }
    }
}
#[cfg(test)]
#[path = "tests/composition.rs"]
mod composition_tests;
#[cfg(test)]
#[path = "tests/foundations.rs"]
mod foundation_tests;

#[cfg(test)]
#[path = "tests/hero.rs"]
mod hero_tests;
