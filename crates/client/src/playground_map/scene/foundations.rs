//! Prepared ground keys include completion for terrain-painted roads.
pub(super) type FoundationKey = (
    aoe_rendering::SceneUnitKind,
    [f64; 2],
    aoe_protocol::ConstructionState,
);
pub(super) fn foundation_key(b: &aoe_protocol::GameplayBuildingState) -> FoundationKey {
    (
        super::building_kind(b),
        super::super::super::village::building_center(b),
        b.construction,
    )
}

/// Conservative bounds for every native footprint and its one-tile apron.
/// The largest marker footprint is four tiles; native metadata can exceed it.
pub(super) fn view_bounds(
    terrain: &[aoe_rendering::SceneTerrain],
    art: &aoe_rendering::GameArt,
) -> Option<[[f64; 2]; 2]> {
    if terrain.is_empty() {
        return None;
    }
    let padding = f64::from(
        art.compositions
            .iter()
            .flat_map(|building| building.footprint)
            .fold(4.0_f32, f32::max),
    ) / 2.0
        + 1.0;
    let (mut min, mut max) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for tile in terrain {
        for axis in 0..2 {
            min[axis] = min[axis].min(tile.position[axis] - padding);
            max[axis] = max[axis].max(tile.position[axis] + padding);
        }
    }
    Some([min, max])
}

pub(super) fn in_view(key: &FoundationKey, bounds: Option<[[f64; 2]; 2]>) -> bool {
    bounds.is_none_or(|[min, max]| {
        (0..2).all(|axis| key.1[axis] >= min[axis] && key.1[axis] <= max[axis])
    })
}

pub(super) fn invalidate_changed_patches(
    patches: &mut super::PatchCache,
    before: &[aoe_protocol::GameplayBuildingState],
    after: &[aoe_protocol::GameplayBuildingState],
) {
    let mut before = before.iter().peekable();
    let mut after = after.iter().peekable();
    while let Some(id) = before
        .peek()
        .map(|b| b.id)
        .into_iter()
        .chain(after.peek().map(|b| b.id))
        .min()
    {
        let previous = before.next_if(|b| b.id == id);
        let next = after.next_if(|b| b.id == id);
        if let (Some(a), Some(b)) = (previous, next)
            && a.kind == b.kind
            && a.age == b.age
            && a.footprint == b.footprint
            && a.construction == b.construction
            && (a.construction == aoe_protocol::ConstructionState::Complete
                || a.progress_permille == b.progress_permille)
        {
            continue;
        }
        let previous = previous.map(foundation_key);
        let next = next.map(foundation_key);
        if previous != next {
            for key in previous.into_iter().chain(next) {
                patches.foundations_changed(key);
            }
        }
    }
}
