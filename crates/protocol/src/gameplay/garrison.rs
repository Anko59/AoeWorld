use super::{BuildingKind, ConstructionState, GameplayBuildingState, UnitKind};
use aoe_core::EntityId;
use serde::{Deserialize, Deserializer, Serialize};

/// Owner-visible occupants. They have no map position or targetable unit state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GarrisonUnitState {
    pub id: EntityId,
    pub kind: UnitKind,
    pub hp: u16,
    pub max_hp: u16,
}

impl BuildingKind {
    pub const fn garrison_capacity(self) -> u8 {
        match self {
            Self::TownCenter => 15,
            Self::Castle => 20,
            Self::WatchTower => 5,
            _ => 0,
        }
    }
    pub const fn admits(self, kind: UnitKind) -> bool {
        !kind.is_naval()
            && self.garrison_capacity() > 0
            && (matches!(self, Self::Castle)
                || matches!(
                    kind,
                    UnitKind::Villager
                        | UnitKind::Militia
                        | UnitKind::ManAtArms
                        | UnitKind::Spearman
                        | UnitKind::Archer
                        | UnitKind::Skirmisher
                        | UnitKind::LongSwordsman
                        | UnitKind::Pikeman
                        | UnitKind::Crossbowman
                        | UnitKind::EliteSkirmisher
                ))
    }
}

pub(super) fn bounded_garrison<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<GarrisonUnitState>, D::Error> {
    super::economy::bounded::<D, _, 20>(d)
}

pub(super) fn valid(b: &GameplayBuildingState) -> bool {
    b.garrison.len() <= usize::from(b.kind.garrison_capacity())
        && (b.garrison.is_empty() || b.construction == ConstructionState::Complete)
        && b.garrison.windows(2).all(|pair| pair[0].id < pair[1].id)
        && b.garrison
            .iter()
            .all(|u| u.id != b.id && b.kind.admits(u.kind) && u.hp > 0 && u.hp <= u.max_hp)
}
