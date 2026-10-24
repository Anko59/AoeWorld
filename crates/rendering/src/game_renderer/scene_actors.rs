//! Native actor tags shared by scene selection and rendering.
use super::SceneUnitKind;

impl SceneUnitKind {
    /// Flat tags keep the scene DTO at 32 bytes, including actor animations.
    pub fn actor(index: u8) -> Self {
        const ACTORS: [SceneUnitKind; 75] = [
            SceneUnitKind::MilitiaStand,
            SceneUnitKind::MilitiaWalk,
            SceneUnitKind::MilitiaAttack,
            SceneUnitKind::MilitiaDie,
            SceneUnitKind::VillagerAttack,
            SceneUnitKind::VillagerDie,
            SceneUnitKind::HunterStand,
            SceneUnitKind::HunterWalk,
            SceneUnitKind::HunterAttack,
            SceneUnitKind::Butcher,
            SceneUnitKind::HunterCarry,
            SceneUnitKind::ShepherdStand,
            SceneUnitKind::ShepherdWalk,
            SceneUnitKind::ShepherdWork,
            SceneUnitKind::SheepStand,
            SceneUnitKind::SheepWalk,
            SceneUnitKind::SheepDie,
            SceneUnitKind::DeerStand,
            SceneUnitKind::DeerWalk,
            SceneUnitKind::DeerDie,
            SceneUnitKind::BoarStand,
            SceneUnitKind::BoarWalk,
            SceneUnitKind::BoarAttack,
            SceneUnitKind::BoarDie,
            SceneUnitKind::WolfStand,
            SceneUnitKind::WolfWalk,
            SceneUnitKind::WolfRun,
            SceneUnitKind::WolfAttack,
            SceneUnitKind::WolfDie,
            SceneUnitKind::ScoutAttack,
            SceneUnitKind::ScoutDie,
            SceneUnitKind::MilitiaDecay,
            SceneUnitKind::VillagerDecay,
            SceneUnitKind::SheepDecay,
            SceneUnitKind::DeerDecay,
            SceneUnitKind::BoarDecay,
            SceneUnitKind::WolfDecay,
            SceneUnitKind::ScoutDecay,
            SceneUnitKind::HunterDie,
            SceneUnitKind::HunterDecay,
            SceneUnitKind::ShepherdDie,
            SceneUnitKind::ShepherdDecay,
            SceneUnitKind::NeutralSheepStand,
            SceneUnitKind::NeutralSheepWalk,
            SceneUnitKind::NeutralSheepDie,
            SceneUnitKind::NeutralSheepDecay,
            SceneUnitKind::FishingShip,
            SceneUnitKind::FishingShipMoving,
            SceneUnitKind::FishingShipWorking,
            SceneUnitKind::VillagerFishing,
            SceneUnitKind::ArcherStand,
            SceneUnitKind::ArcherWalk,
            SceneUnitKind::ArcherAttack,
            SceneUnitKind::ArcherDie,
            SceneUnitKind::ArcherDecay,
            SceneUnitKind::SkirmisherStand,
            SceneUnitKind::SkirmisherWalk,
            SceneUnitKind::SkirmisherAttack,
            SceneUnitKind::SkirmisherDie,
            SceneUnitKind::SkirmisherDecay,
            SceneUnitKind::SpearmanStand,
            SceneUnitKind::SpearmanWalk,
            SceneUnitKind::SpearmanAttack,
            SceneUnitKind::SpearmanDie,
            SceneUnitKind::SpearmanDecay,
            SceneUnitKind::ManAtArmsStand,
            SceneUnitKind::ManAtArmsWalk,
            SceneUnitKind::ManAtArmsAttack,
            SceneUnitKind::ManAtArmsDie,
            SceneUnitKind::ManAtArmsDecay,
            SceneUnitKind::ScoutStand,
            SceneUnitKind::ScoutWalk,
            SceneUnitKind::Arrow,
            SceneUnitKind::Javelin,
            SceneUnitKind::FarmerWork,
        ];
        ACTORS
            .get(usize::from(index))
            .copied()
            .unwrap_or(Self::Scout)
    }
    pub(in crate::game_renderer) fn actor_slot(self) -> Option<usize> {
        if (Self::ArcherStand as u8..=Self::FarmerWork as u8).contains(&(self as u8)) {
            return Some(50 + (self as usize - Self::ArcherStand as usize));
        }
        (self as u8)
            .checked_sub(Self::MilitiaStand as u8)
            .filter(|index| *index < 50)
            .map(usize::from)
    }
}
