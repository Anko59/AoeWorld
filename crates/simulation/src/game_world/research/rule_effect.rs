//! Effects whose state is read at the point of use, such as combat and trade.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleEffect {
    MeleeAttack,
    Armor(ArmorClass),
    Arrows { galleys: bool },
    ProjectileAttack,
    LeadTargets,
    RemoveMinimumRange,
    BuildingDurability(BuildingGroup),
    BonusDamage(AttackBonus),
    ArcheryReload,
    TowerAttack,
    ConversionResistance,
    GarrisonHealing,
    SharedConversion,
    ConversionRecovery,
    MonkRange,
    Convert(ConversionTarget),
    DeathOnConversion,
    TrainingRate,
    Unimplemented,
    MarketCommission,
    TributeFee,
    ConstructionRate,
    FishingYield,
    ShipProduction,
    ShipSpeed,
    FireShipAttack,
    DeathBlast,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArmorClass {
    Infantry,
    Cavalry,
    Archers,
    Militia,
    MountedArchers,
    Ships,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildingGroup {
    Standard { bonus_resistance: bool },
    StoneWall,
    Castle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttackBonus {
    InfantryBuildings,
    MountedArchersSpears,
    SiegeBuildings,
    FortificationsShips,
    VillagersBuildingsAndRams,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversionTarget {
    Monks,
    SiegeAndBuildings,
}
