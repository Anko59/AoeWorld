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
        ManAtArms
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
        PalisadeGate
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
        PaddedArcherArmor
    ]
);
mirror!(Age, aoe_sdk::Age, [Dark, Feudal]);
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
        RejectedNotController,
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
        RejectedPopulationCap,
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
        RejectedNoMarket
    ]
);

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
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
