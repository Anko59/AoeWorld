use super::{BuildingKind, ConstructionState, GameplayBuildingState};
impl super::UnitKind {
    pub fn is_cavalry(self) -> bool {
        self == Self::Scout
    }
}
pub(super) fn valid_auto_scout(unit: &super::UnitState) -> bool {
    !unit.auto_scout || (unit.hp > 0 && unit.kind.is_cavalry())
}

pub(super) fn valid_reseeds(building: &GameplayBuildingState) -> bool {
    building.farm_reseeds <= 99
        && (building.farm_reseeds == 0
            || (building.kind == BuildingKind::Mill
                && building.construction == ConstructionState::Complete))
}
