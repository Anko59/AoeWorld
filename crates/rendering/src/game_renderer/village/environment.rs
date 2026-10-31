//! One DAT composition path for resource bodies, trees and their depletion stages.
use super::*;
fn identity(r: SceneResource) -> Option<(i32, Option<u8>)> {
    let tree = match r.visual_family {
        2 => 350,
        4 => 351,
        _ => 349,
    };
    Some(match r.kind {
        0 => (
            match r.visual_family {
                5 => 69,
                6 => 455 + i32::from(r.visual_variant % 5),
                _ => 59,
            },
            None,
        ),
        1 => (tree, None),
        2 => (66, None),
        3 => (102, None),
        SceneResource::CHOPPED_TREE => (tree, Some(0)),
        SceneResource::TREE_STUMP => (415, None),
        SceneResource::ROCK => (623, None),
        SceneResource::DEADWOOD if r.visual_variant % 5 < 3 => (415, None),
        SceneResource::DEADWOOD => (349, Some(0)),
        _ => return None,
    })
}
pub(in crate::game_renderer) fn present(art: &GameArt, r: SceneResource) -> bool {
    !art.environment.is_empty()
        && identity(r).is_some_and(|(id, _)| art.environment.iter().any(|c| c.unit_id == id))
}
#[inline(never)]
pub(in crate::game_renderer) fn frames(
    art: &GameArt,
    r: SceneResource,
    animation: usize,
    emit: &mut dyn FnMut(GameFrame),
) {
    frames_select(art, r, animation, &mut |frame, _, _, _| frame, emit);
}

#[inline(never)]
pub(in crate::game_renderer) fn frames_select(
    art: &GameArt,
    r: SceneResource,
    animation: usize,
    select: &mut dyn FnMut(GameFrame, &[GameFrame], usize, usize) -> GameFrame,
    emit: &mut dyn FnMut(GameFrame),
) {
    let Some((id, state)) = identity(r) else {
        return;
    };
    let Some(c) = art.environment.iter().find(|c| c.unit_id == id) else {
        return;
    };
    let parts = if let Some(state) = state {
        c.states
            .iter()
            .find(|(s, _)| *s == state)
            .map_or(&[][..], |(_, p)| p)
    } else {
        &c.parts
    };
    for p in parts {
        let mut f = p.frame;
        let (mut selected, mut stride) = (0, 1);
        if !p.animation.is_empty() {
            let angles = usize::from(p.angles).max(1);
            let count = p.animation.len() / angles;
            let row = usize::from(r.visual_variant) % angles;
            let phase = if p.frame_rate > 0.0 {
                let step = f64::from(p.frame_rate) * 1000.0;
                let clock =
                    animation as f64 % (step * count as f64 + f64::from(p.replay_delay) * 1000.0);
                ((clock / step) as usize).min(count - 1)
            } else {
                usize::from(r.visual_variant) % count
            };
            selected = row * count + phase;
            stride = count;
            f = p.animation[selected];
        }
        f = select(
            f,
            if p.frame_rate > 0.0 {
                &p.animation
            } else {
                &[]
            },
            selected,
            stride,
        );
        f.anchor = [0, 1].map(|i| f.anchor[i] - p.offset[i]);
        emit(f);
    }
}
pub(super) fn primary(art: &GameArt, r: SceneResource, animation: usize) -> Option<GameFrame> {
    let mut first = None;
    frames(art, r, animation, &mut |f| {
        if first.is_none() {
            first = Some(f);
        }
    });
    first
}
pub(super) fn emit(
    art: &GameArt,
    r: SceneResource,
    camera: SceneCamera,
    animation: usize,
    out: &mut dyn FnMut((Sprite, GameFrame, f64, u64)),
) {
    frames_select(
        art,
        r,
        animation,
        &mut |frame, frames, index, stride| {
            let resolved = if frames.is_empty() {
                art.resident_frame(frame)
            } else {
                art.resident_animation(frames, index, stride)
            };
            resolved.unwrap_or(frame)
        },
        &mut |native| {
            let logical = world_frame(native);
            if resource_sprite_bounds(r, native, camera).is_none() {
                return;
            }
            let Some(mut frame) = art.resident_frame(native) else {
                return;
            };
            frame = world_frame(frame);
            if let Some(mut sprite) = scene_sprite(frame, r.position, r.elevation_meters, camera) {
                if r.kind <= SceneResource::CHOPPED_TREE
                    && !(r.kind == 0 && matches!(r.visual_family, 5 | 6))
                {
                    sprite.pages[3] |= Sprite::OCCLUDER;
                }
                sprite.pages[3] |= u32::from(r.highlight & 3) << 4;
                out((
                    sprite,
                    scaled(frame, camera.zoom as f32),
                    object_depth(WorldObject::Resource(r, logical, None)),
                    r.id,
                ));
            }
        },
    );
}

pub(super) fn animation_frame(
    art: &GameArt,
    r: SceneResource,
    animation: usize,
    camera: SceneCamera,
) -> Option<usize> {
    let mut key = None;
    frames(art, r, animation, &mut |f| {
        if resource_sprite_bounds(r, f, camera).is_some() {
            key = Some(
                key.unwrap_or(0_usize)
                    .wrapping_mul(31)
                    .wrapping_add(f.stream as usize),
            );
        }
    });
    key
}
