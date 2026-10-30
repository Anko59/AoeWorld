use super::*;

fn frames(art: &GameArt, resource: SceneResource) -> (&[GameFrame], usize) {
    let slot = if resource.visual_family == 5 {
        0
    } else {
        1 + usize::from(resource.visual_variant % 5)
    };
    (&art.fishing[slot], if slot == 0 { 3 } else { 9 })
}

fn index(resource: SceneResource, animation: usize, period: usize, count: usize) -> usize {
    let animation = animation / 100;
    (animation / period + resource.id as usize) % count.max(1)
}

pub(super) fn frame(art: &GameArt, resource: SceneResource, animation: usize) -> Option<GameFrame> {
    let (frames, period) = frames(art, resource);
    frames
        .get(index(resource, animation, period, frames.len()))
        .copied()
}

/// Displayed frame index only for animated fish whose current art is visible.
pub fn scene_resource_animation_frame(
    art: &GameArt,
    resource: SceneResource,
    animation: usize,
    camera: SceneCamera,
) -> Option<usize> {
    if resource.kind != 0 || !matches!(resource.visual_family, 5 | 6) {
        return None;
    }
    let (frames, period) = frames(art, resource);
    if frames.len() <= 1 {
        return None;
    }
    let index = index(resource, animation, period, frames.len());
    resource_sprite_bounds(resource, frames[index], camera).map(|_| index)
}
