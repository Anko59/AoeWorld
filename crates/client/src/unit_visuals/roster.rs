//! Semantic pose groups for the complete spawned military and naval roster.
use super::*;
pub(super) fn military_poses(kind: UnitKind) -> Option<[Pose; 4]> {
    use Pose::*;
    Some(match kind {
        UnitKind::Galley => [Galley, GalleyWalk, GalleyAttack, GalleyDie],
        UnitKind::WarGalley => [WarGalley, WarGalleyWalk, WarGalleyAttack, WarGalleyDie],
        UnitKind::FireShip => [FireShip, FireShipWalk, FireShipAttack, FireShipDie],
        UnitKind::DemolitionShip => [
            DemolitionShip,
            DemolitionShipWalk,
            DemolitionShipAttack,
            DemolitionShipDie,
        ],
        UnitKind::TransportShip => [
            TransportShip,
            TransportShipWalk,
            TransportShipAttack,
            TransportShipDie,
        ],
        UnitKind::General => [GeneralStand, GeneralWalk, GeneralAttack, GeneralDie],
        UnitKind::Vassal => [KingStand, KingWalk, KingStand, KingDie],
        UnitKind::Archer => [ArcherStand, ArcherWalk, ArcherAttack, ArcherDie],
        UnitKind::Skirmisher => [
            SkirmisherStand,
            SkirmisherWalk,
            SkirmisherAttack,
            SkirmisherDie,
        ],
        UnitKind::Spearman => [SpearmanStand, SpearmanWalk, SpearmanAttack, SpearmanDie],
        UnitKind::ManAtArms => [ManAtArmsStand, ManAtArmsWalk, ManAtArmsAttack, ManAtArmsDie],
        UnitKind::Scout => [ScoutStand, ScoutWalk, ScoutAttack, ScoutDie],
        UnitKind::LongSwordsman => [
            LongSwordsmanStand,
            LongSwordsmanWalk,
            LongSwordsmanAttack,
            LongSwordsmanDie,
        ],
        UnitKind::Pikeman => [PikemanStand, PikemanWalk, PikemanAttack, PikemanDie],
        UnitKind::Crossbowman => [
            CrossbowmanStand,
            CrossbowmanWalk,
            CrossbowmanAttack,
            CrossbowmanDie,
        ],
        UnitKind::EliteSkirmisher => [
            EliteSkirmisherStand,
            EliteSkirmisherWalk,
            EliteSkirmisherAttack,
            EliteSkirmisherDie,
        ],
        UnitKind::Knight => [KnightStand, KnightWalk, KnightAttack, KnightDie],
        UnitKind::LightCavalry => [
            LightCavalryStand,
            LightCavalryWalk,
            LightCavalryAttack,
            LightCavalryDie,
        ],
        UnitKind::Monk => [MonkStand, MonkWalk, MonkAttack, MonkDie],
        UnitKind::Mangonel => [MangonelStand, MangonelWalk, MangonelAttack, MangonelDie],
        UnitKind::BatteringRam => [
            BatteringRamStand,
            BatteringRamWalk,
            BatteringRamAttack,
            BatteringRamDie,
        ],

        _ => return None,
    })
}
