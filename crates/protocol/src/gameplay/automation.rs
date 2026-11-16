use super::{BuildingKind, ConstructionState, GameplayBuildingState};
impl super::UnitKind {
    pub const fn is_naval(self) -> bool {
        matches!(
            self,
            Self::FishingShip
                | Self::Galley
                | Self::FireShip
                | Self::DemolitionShip
                | Self::TransportShip
                | Self::WarGalley
                | Self::TradeCog
                | Self::FireGalley
                | Self::FastFireShip
                | Self::DemolitionRaft
                | Self::HeavyDemolitionShip
                | Self::Galleon
                | Self::CannonGalleon
                | Self::EliteCannonGalleon
                | Self::Hulk
                | Self::WarHulk
                | Self::Carrack
                | Self::CatapultGalleon
        )
    }
    pub const fn is_trader(self) -> bool {
        matches!(self, Self::TradeCart | Self::TradeCog)
    }
    /// Units the player may put on automatic exploration: cavalry on land,
    /// warships and transports on water.
    pub fn can_auto_scout(self) -> bool {
        self.is_cavalry()
            || matches!(
                self,
                Self::Galley
                    | Self::WarGalley
                    | Self::Galleon
                    | Self::FireGalley
                    | Self::FireShip
                    | Self::FastFireShip
                    | Self::TransportShip
            )
    }
    pub fn is_cavalry(self) -> bool {
        matches!(
            self,
            Self::Scout
                | Self::Knight
                | Self::LightCavalry
                | Self::Cavalier
                | Self::Paladin
                | Self::Hussar
                | Self::CamelRider
                | Self::HeavyCamelRider
                | Self::CavalryArcher
                | Self::HeavyCavalryArcher
        )
    }
}
pub(super) fn valid_auto_scout(unit: &super::UnitState) -> bool {
    !unit.auto_scout || (unit.hp > 0 && unit.kind.can_auto_scout())
}

pub(super) fn valid_reseeds(building: &GameplayBuildingState) -> bool {
    building.farm_reseeds <= 99
        && (building.farm_reseeds == 0
            || (building.kind == BuildingKind::Mill
                && building.construction == ConstructionState::Complete))
}
