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
