//! The protocol's enumerations as Python enums (`UnitKind.VILLAGER`, ...).
use pyo3::prelude::*;

macro_rules! mirror {
    ($py:ident, $rs:path, [$($variant:ident),+ $(,)?]) => {
        #[pyclass(eq, eq_int, frozen, hash, module = "aoeworld", rename_all = "SCREAMING_SNAKE_CASE")]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $py {
            $($variant),+
        }
        impl From<$rs> for $py {
            fn from(value: $rs) -> Self {
                match value {
                    $(<$rs>::$variant => Self::$variant),+
                }
            }
        }
        impl From<$py> for $rs {
            fn from(value: $py) -> Self {
                match value {
                    $($py::$variant => <$rs>::$variant),+
                }
            }
        }
    };
}

mirror!(Formation, aoe_sdk::Formation, [Line, Box, Staggered, Flank]);
mirror!(
    CombatStance,
    aoe_sdk::CombatStance,
    [Aggressive, Defensive, StandGround, NoAttack]
);

mirror!(
    UnitKind,
    aoe_sdk::UnitKind,
    [
        Villager,
        Scout,
        Militia,
        FishingShip,
        Archer,
        Skirmisher,
        Spearman,
        ManAtArms,
        LongSwordsman,
        Pikeman,
        Crossbowman,
        EliteSkirmisher,
        Knight,
        LightCavalry,
        Monk,
        Mangonel,
        BatteringRam,
        Galley,
        FireShip,
        DemolitionShip,
        TransportShip,
        WarGalley,
        Vassal,
        General,
    ]
);
mirror!(
    BuildingKind,
    aoe_sdk::BuildingKind,
    [
        TownCenter,
        House,
        LumberCamp,
        MiningCamp,
        Mill,
        Farm,
        Barracks,
        PalisadeWall,
        Outpost,
        Dock,
        ArcheryRange,
        Stable,
        Blacksmith,
        Market,
        WatchTower,
        PalisadeGate,
        Castle,
        Monastery,
        University,
        SiegeWorkshop,
        Bridge,
        StoneWall,
        StoneGate,
    ]
);
mirror!(
    ResourceKind,
    aoe_sdk::ResourceKind,
    [Food, Wood, Gold, Stone]
);
mirror!(TradeResource, aoe_sdk::TradeResource, [Wood, Food, Stone]);
mirror!(
    Tech,
    aoe_sdk::Tech,
    [
        Loom,
        TownWatch,
        FeudalAge,
        DoubleBitAxe,
        HorseCollar,
        GoldMining,
        StoneMining,
        Wheelbarrow,
        ManAtArms,
        Forging,
        ScaleMailArmor,
        ScaleBardingArmor,
        Fletching,
        PaddedArcherArmor,
        CastleAge,
        LongSwordsman,
        Pikeman,
        Crossbowman,
        EliteSkirmisher,
        LightCavalry,
        HandCart,
        TownPatrol,
        IronCasting,
        ChainMailArmor,
        ChainBardingArmor,
        BodkinArrow,
        LeatherArcherArmor,
        HeavyPlow,
        BowSaw,
        GoldShaftMining,
        Masonry,
        Ballistics,
        MurderHoles,
        WarGalley,
    ]
);
mirror!(Age, aoe_sdk::Age, [Dark, Feudal, Castle]);
mirror!(
    UnitTask,
    aoe_sdk::UnitTask,
    [
        Idle,
        Moving,
        Gathering,
        Carrying,
        Returning,
        Attacking,
        Dying,
        Building,
        Farming,
        Hunting,
        Shepherding,
        Fishing
    ]
);
mirror!(AnimalKind, aoe_sdk::AnimalKind, [Sheep, Deer, Boar, Wolf]);
mirror!(
    AnimalTask,
    aoe_sdk::AnimalTask,
    [Idle, Fleeing, Attacking, Dead, Carcass]
);
mirror!(
    Visibility,
    aoe_sdk::Visibility,
    [Unexplored, Explored, Visible]
);
mirror!(
    CommandResult,
    aoe_sdk::CommandResult,
    [
        Accepted,
        RejectedDisconnected,
        RejectedUnknownEntity,
        RejectedSequence,
        RejectedRateLimited,
        RejectedQueueFull,
        RejectedInvalidDestination,
        RejectedUnreachable,
        RejectedPathBudgetExceeded,
        RejectedNotOwner,
        RejectedWrongUnitKind,
        RejectedInvalidResource,
        RejectedInsufficientFood,
        RejectedTrainingQueueFull,
        RejectedResourceUnavailable,
        RejectedInvalidTarget,
        RejectedInsufficientResources,
        RejectedNotBuildable,
        RejectedRequirementMissing,
        RejectedInvalidPlacement,
        RejectedUnderConstruction,
        RejectedAlreadyComplete,
        RejectedFarmUnavailable,
        RejectedNotResearchable,
        RejectedAlreadyResearched,
        RejectedResearchBusy,
        RejectedTechRequirement,
        RejectedRequiresFeudalAge,
        RejectedNoMarket,
        RejectedWorldLimit,
        RejectedOrderTooLarge,
        RejectedUnexploredPlacement,
        RejectedRequiresCastleAge,
        RejectedGarrisonFull,
        RejectedVassalExists,
        RejectedVassalLeash,
        RejectedVassalHome
    ]
);

mirror!(GeneralState, aoe_sdk::GeneralState, [Idle, Marching, Fighting, Retreating]);

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<CombatStance>()?;
    module.add_class::<GeneralState>()?;
    module.add_class::<Formation>()?;
    module.add_class::<UnitKind>()?;
    module.add_class::<BuildingKind>()?;
    module.add_class::<ResourceKind>()?;
    module.add_class::<TradeResource>()?;
    module.add_class::<Tech>()?;
    module.add_class::<Age>()?;
    module.add_class::<UnitTask>()?;
    module.add_class::<AnimalKind>()?;
    module.add_class::<AnimalTask>()?;
    module.add_class::<Visibility>()?;
    module.add_class::<CommandResult>()?;
    Ok(())
}
