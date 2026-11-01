//! Native animation working-set demand, independent of physical atlas placement.
use super::*;

/// Requested cells and their currently drawable substitutes (zero is cold).
/// Multiple semantic users of one pixel identity retain all their choices.
pub fn scene_native_substitutes(
    art: &GameArt,
    units: &[SceneUnit],
    resources: &[SceneResource],
    camera: SceneCamera,
    animation: usize,
) -> Vec<(u32, u32)> {
    use std::collections::BTreeSet;
    let mut result = Vec::new();
    let choose = |frame: GameFrame, frames: &[GameFrame], index, stride| {
        if frames.is_empty() {
            art.resident_frame(frame)
        } else {
            art.resident_animation(frames, index, stride)
        }
    };
    for &unit in units {
        if unit.kind.building_side().is_some() {
            let mut choices = Vec::new();
            let mut visible = BTreeSet::new();
            village::building_select(
                art,
                unit,
                camera,
                animation,
                &mut |frame, frames, index, stride| {
                    if frame.stream != 0 {
                        choices.push((
                            frame.stream,
                            choose(frame, frames, index, stride).map_or(0, |f| f.stream),
                        ));
                    }
                    frame
                },
                &mut |(_, frame, _, _)| {
                    if frame.stream != 0 {
                        visible.insert(frame.stream);
                    }
                },
            );
            for id in visible {
                let mut found = false;
                for &pair in choices.iter().filter(|(requested, _)| *requested == id) {
                    result.push(pair);
                    found = true;
                }
                if !found {
                    result.push((
                        id,
                        if art
                            .resident
                            .get(id as usize - 1)
                            .is_some_and(Option::is_some)
                        {
                            id
                        } else {
                            0
                        },
                    ));
                }
            }
            continue;
        }
        let Some([left, top, right, bottom]) = scene_unit_bounds(art, unit, camera, 0) else {
            continue;
        };
        if right < 0.0 || bottom < 0.0 || left > camera.viewport[0] || top > camera.viewport[1] {
            continue;
        }
        let Some((frames, stride, _)) = village::frames(art, unit) else {
            continue;
        };
        let phase =
            usize::from(unit.actor_frame() >> if unit.kind.native_directions() { 4 } else { 3 })
                .min(stride - 1);
        let (index, _) = unit_frame_index(unit, stride, phase, frames.len() / stride);
        let requested = frames[index];
        if requested.stream != 0 {
            result.push((
                requested.stream,
                art.resident_animation(frames, index, stride)
                    .map_or(0, |f| f.stream),
            ));
        }
    }
    for &resource in resources {
        if !environment::present(art, resource) {
            if let Some((frame, shadow)) = resource_presentation_at(art, resource, animation) {
                for (frame, resolved) in
                    std::iter::once((frame, scenery::resident(art, resource, frame)))
                        .chain(shadow.map(|f| (f, art.resident_frame(f))))
                {
                    if frame.stream != 0
                        && resource_sprite_bounds(resource, frame, camera).is_some()
                    {
                        result.push((frame.stream, resolved.map_or(0, |f| f.stream)));
                    }
                }
            }
            continue;
        }
        let mut choices = Vec::new();
        let mut visible = BTreeSet::new();
        environment::frames_select(
            art,
            resource,
            animation,
            &mut |frame, frames, index, stride| {
                if frame.stream != 0 {
                    choices.push((
                        frame.stream,
                        choose(frame, frames, index, stride).map_or(0, |f| f.stream),
                    ));
                }
                frame
            },
            &mut |frame| {
                if frame.stream != 0 && resource_sprite_bounds(resource, frame, camera).is_some() {
                    visible.insert(frame.stream);
                }
            },
        );
        result.extend(choices.into_iter().filter(|(id, _)| visible.contains(id)));
    }
    // Coverage combines semantic users and deduplicates the pinned IDs. Keep
    // each user's choice here instead of instantiating another tuple sort.
    result
}

/// Visible native cells, deduplicated independently of their current residency.
pub fn scene_native_frames(
    art: &GameArt,
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
) -> Vec<u32> {
    demand(art, units, camera, animation, 0..=0)
}

/// Two source phases of lookahead, always inside the current facing row.
pub fn scene_native_prefetch(
    art: &GameArt,
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
) -> Vec<u32> {
    demand(art, units, camera, animation, 1..=2)
}

fn demand(
    art: &GameArt,
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
    ahead: std::ops::RangeInclusive<usize>,
) -> Vec<u32> {
    let mut required = std::collections::BTreeSet::new();
    for &unit in units {
        if unit.kind.building_side().is_some() {
            for offset in ahead.clone() {
                village::building(
                    art,
                    unit,
                    camera,
                    animation + offset * 100,
                    &mut |(_, frame, _, _)| {
                        if frame.stream != 0 {
                            required.insert(frame.stream);
                        }
                    },
                );
            }
            continue;
        }
        if unit.kind.actor_slot().is_none() {
            continue;
        }
        let Some([left, top, right, bottom]) = scene_unit_bounds(art, unit, camera, 0) else {
            continue;
        };
        if right < 0.0 || bottom < 0.0 || left > camera.viewport[0] || top > camera.viewport[1] {
            continue;
        }
        let Some((frames, stride, _)) = village::frames(art, unit) else {
            continue;
        };
        let phase =
            usize::from(unit.actor_frame() >> if unit.kind.native_directions() { 4 } else { 3 })
                .min(stride - 1);
        for offset in ahead.clone() {
            let (index, _) = unit_frame_index(
                unit,
                stride,
                (phase + offset) % stride,
                frames.len() / stride,
            );
            if frames[index].stream != 0 {
                required.insert(frames[index].stream);
            }
        }
    }
    required.into_iter().collect()
}

/// Native resource cells share the same visible LRU as units and buildings.
pub fn scene_resource_native_frames(
    art: &GameArt,
    resources: &[SceneResource],
    camera: SceneCamera,
    animation: usize,
) -> Vec<u32> {
    let mut required = std::collections::BTreeSet::new();
    for &r in resources {
        if !environment::present(art, r) {
            if let Some((frame, shadow)) = resource_presentation_at(art, r, animation) {
                for frame in std::iter::once(frame).chain(shadow) {
                    if frame.stream != 0 && resource_sprite_bounds(r, frame, camera).is_some() {
                        required.insert(frame.stream);
                    }
                }
            }
            continue;
        }
        environment::frames(art, r, animation, &mut |f| {
            if f.stream != 0 && resource_sprite_bounds(r, f, camera).is_some() {
                required.insert(f.stream);
            }
        });
    }
    required.into_iter().collect()
}
