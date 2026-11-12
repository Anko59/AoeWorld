//! Dead bodies lie on the ground: the die and decay poses of every actor.
use super::SceneUnitKind;

impl SceneUnitKind {
    /// A corpse is a ground decal, never an occluder of standing actors.
    pub(in crate::game_renderer) fn is_corpse(self) -> bool {
        matches!(
            self,
            Self::MilitiaDie
                | Self::VillagerDie
                | Self::SheepDie
                | Self::DeerDie
                | Self::BoarDie
                | Self::WolfDie
                | Self::ScoutDie
                | Self::MilitiaDecay
                | Self::VillagerDecay
                | Self::SheepDecay
                | Self::DeerDecay
                | Self::BoarDecay
                | Self::WolfDecay
                | Self::ScoutDecay
                | Self::HunterDie
                | Self::HunterDecay
                | Self::ShepherdDie
                | Self::ShepherdDecay
                | Self::NeutralSheepDie
                | Self::NeutralSheepDecay
                | Self::ArcherDie
                | Self::ArcherDecay
                | Self::SkirmisherDie
                | Self::SkirmisherDecay
                | Self::SpearmanDie
                | Self::SpearmanDecay
                | Self::ManAtArmsDie
                | Self::ManAtArmsDecay
                | Self::LongSwordsmanDie
                | Self::LongSwordsmanDecay
                | Self::PikemanDie
                | Self::PikemanDecay
                | Self::CrossbowmanDie
                | Self::CrossbowmanDecay
                | Self::EliteSkirmisherDie
                | Self::EliteSkirmisherDecay
                | Self::KnightDie
                | Self::KnightDecay
                | Self::LightCavalryDie
                | Self::LightCavalryDecay
                | Self::MonkDie
                | Self::MonkDecay
                | Self::MangonelDie
                | Self::MangonelDecay
                | Self::BatteringRamDie
                | Self::BatteringRamDecay
                | Self::KingDie
                | Self::KingDecay
                | Self::GeneralDie
                | Self::GeneralDecay
                | Self::FishingShipDie
                | Self::FishingShipDecay
                | Self::GalleyDie
                | Self::GalleyDecay
                | Self::WarGalleyDie
                | Self::WarGalleyDecay
                | Self::DemolitionShipDie
                | Self::DemolitionShipDecay
                | Self::TransportShipDie
                | Self::TransportShipDecay
                | Self::TwoHandedSwordsmanDie
                | Self::TwoHandedSwordsmanDecay
                | Self::ChampionDie
                | Self::ChampionDecay
                | Self::HalberdierDie
                | Self::HalberdierDecay
                | Self::ArbalesterDie
                | Self::ArbalesterDecay
                | Self::CavalierDie
                | Self::CavalierDecay
                | Self::PaladinDie
                | Self::PaladinDecay
                | Self::HussarDie
                | Self::HussarDecay
                | Self::CappedRamDie
                | Self::CappedRamDecay
                | Self::SiegeRamDie
                | Self::SiegeRamDecay
                | Self::OnagerDie
                | Self::OnagerDecay
        )
    }
}
