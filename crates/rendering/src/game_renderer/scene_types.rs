//! Backend-neutral scene data; no map transport or simulation ownership.
use aoe_core::EntityId;

#[derive(Clone, Copy)]
pub struct SceneCamera {
    pub center: [f64; 2],
    pub zoom: f64,
    pub viewport: [f64; 2],
    pub focus_elevation_meters: f64,
}

#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SceneUnitKind {
    #[default]
    Scout,
    Villager,
    VillagerChopping,
    VillagerCarryingWood,
    VillagerForaging,
    VillagerCarryingFood,
    VillagerMiningGold,
    VillagerCarryingGold,
    VillagerMiningStone,
    VillagerCarryingStone,
    TownCenter,
    VillagerBuilding,
    BuildingTownCenter,
    House,
    LumberCamp,
    MiningCamp,
    Mill,
    Farm,
    Barracks,
    PalisadeWall,
    Outpost,
    Foundation1x1,
    Foundation2x2,
    Foundation3x3,
    Foundation4x4,
    RubbleTownCenter,
    RubbleHouse,
    RubbleLumberCamp,
    RubbleMiningCamp,
    RubbleMill,
    RubbleFarm,
    RubbleBarracks,
    RubblePalisadeWall,
    RubbleOutpost,
    /// Native actor group; facing packs the direction and selected frame.
    MilitiaStand,
    MilitiaWalk,
    MilitiaAttack,
    MilitiaDie,
    VillagerAttack,
    VillagerDie,
    HunterStand,
    HunterWalk,
    HunterAttack,
    Butcher,
    HunterCarry,
    ShepherdStand,
    ShepherdWalk,
    ShepherdWork,
    SheepStand,
    SheepWalk,
    SheepDie,
    DeerStand,
    DeerWalk,
    DeerDie,
    BoarStand,
    BoarWalk,
    BoarAttack,
    BoarDie,
    WolfStand,
    WolfWalk,
    WolfRun,
    WolfAttack,
    WolfDie,
    ScoutAttack,
    ScoutDie,
    MilitiaDecay,
    VillagerDecay,
    SheepDecay,
    DeerDecay,
    BoarDecay,
    WolfDecay,
    ScoutDecay,
    HunterDie,
    HunterDecay,
    ShepherdDie,
    ShepherdDecay,
    NeutralSheepStand,
    NeutralSheepWalk,
    NeutralSheepDie,
    NeutralSheepDecay,
    FishingShip,
    FishingShipMoving,
    FishingShipWorking,
    VillagerFishing,
    Dock,
    RubbleDock,
    BuilderAtlas,
    ArcheryRange,
    Stable,
    Blacksmith,
    Market,
    WatchTower,
    PalisadeGate,
    RubbleArcheryRange,
    RubbleStable,
    RubbleBlacksmith,
    RubbleMarket,
    RubbleWatchTower,
    RubblePalisadeGate,
    FeudalTownCenter,
    FeudalHouse,
    FeudalMill,
    FeudalBarracks,
    OpenPalisadeGate,
    GateFoundation,
    FeudalDock,
    ConstructionTownCenter,
    ConstructionHouse,
    ConstructionLumberCamp,
    ConstructionMiningCamp,
    ConstructionMill,
    ConstructionFarm,
    ConstructionBarracks,
    ConstructionPalisadeWall,
    ConstructionOutpost,
    ConstructionRange,
    ConstructionStable,
    ConstructionBlacksmith,
    ConstructionMarket,
    ConstructionTower,
    ConstructionGate,
    ConstructionFeudalTownCenter,
    ConstructionFeudalHouse,
    ConstructionFeudalMill,
    ConstructionFeudalLumberCamp,
    ConstructionFeudalMiningCamp,
    ConstructionFeudalBarracks,
    ConstructionFeudalDock,
    FeudalLumberCamp,
    FeudalMiningCamp,
    FoundationDock,
    ArcherStand,
    ArcherWalk,
    ArcherAttack,
    ArcherDie,
    ArcherDecay,
    SkirmisherStand,
    SkirmisherWalk,
    SkirmisherAttack,
    SkirmisherDie,
    SkirmisherDecay,
    SpearmanStand,
    SpearmanWalk,
    SpearmanAttack,
    SpearmanDie,
    SpearmanDecay,
    ManAtArmsStand,
    ManAtArmsWalk,
    ManAtArmsAttack,
    ManAtArmsDie,
    ManAtArmsDecay,
    ScoutStand,
    ScoutWalk,
    Arrow,
    Javelin,
}

impl SceneUnitKind {
    /// Flat tags keep the scene DTO at 32 bytes, including actor animations.
    pub fn actor(index: u8) -> Self {
        const ACTORS: [SceneUnitKind; 74] = [
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
        ];
        ACTORS
            .get(usize::from(index))
            .copied()
            .unwrap_or(Self::Scout)
    }
    pub(super) fn actor_slot(self) -> Option<usize> {
        if (Self::ArcherStand as u8..=Self::Javelin as u8).contains(&(self as u8)) {
            return Some(50 + (self as usize - Self::ArcherStand as usize));
        }
        (self as u8)
            .checked_sub(Self::MilitiaStand as u8)
            .filter(|index| *index < 50)
            .map(usize::from)
    }

    /// Map building roles to the atlas's semantic order.
    pub fn building(slot: u8) -> Self {
        use SceneUnitKind::*;
        [
            BuildingTownCenter,
            House,
            LumberCamp,
            MiningCamp,
            Mill,
            Farm,
            Barracks,
            PalisadeWall,
            Outpost,
            Foundation1x1,
            Foundation2x2,
            Foundation3x3,
            Foundation4x4,
            RubbleTownCenter,
            RubbleHouse,
            RubbleLumberCamp,
            RubbleMiningCamp,
            RubbleMill,
            RubbleFarm,
            RubbleBarracks,
            RubblePalisadeWall,
            RubbleOutpost,
            BuilderAtlas,
            Dock,
            FoundationDock,
            RubbleDock,
            ArcheryRange,
            Stable,
            Blacksmith,
            Market,
            WatchTower,
            PalisadeGate,
            RubbleArcheryRange,
            RubbleStable,
            RubbleBlacksmith,
            RubbleMarket,
            RubbleWatchTower,
            RubblePalisadeGate,
            FeudalTownCenter,
            FeudalHouse,
            FeudalMill,
            FeudalBarracks,
            OpenPalisadeGate,
            GateFoundation,
            FeudalDock,
        ][usize::from(slot.min(44))]
    }
    pub(super) fn building_slot(self) -> Option<u8> {
        match self {
            Self::Dock => return Some(23),
            Self::FoundationDock => return Some(24),
            Self::RubbleDock => return Some(25),
            Self::BuilderAtlas => return Some(22),
            Self::FeudalLumberCamp => return Some(2),
            Self::FeudalMiningCamp => return Some(3),
            _ => {}
        }
        if self.is_foundation() {
            return Some(match self.building_id()? {
                109 | 71 | 84 => 12,
                50 | 12 | 87 | 101 | 103 | 498 => 11,
                72 | 598 | 79 => 9,
                792 => 43,
                133 => 24,
                _ => 10,
            });
        }
        (self as u8)
            .checked_sub(Self::BuildingTownCenter as u8)
            .filter(|slot| *slot <= 21)
            .or_else(|| {
                (self as u8)
                    .checked_sub(Self::ArcheryRange as u8)
                    .filter(|slot| *slot <= 18)
                    .map(|slot| slot + 26)
            })
    }

    pub fn foundation(id: i16) -> Self {
        use SceneUnitKind::*;
        match id {
            45 => FoundationDock,
            109 => ConstructionTownCenter,
            70 => ConstructionHouse,
            562 => ConstructionLumberCamp,
            584 => ConstructionMiningCamp,
            68 => ConstructionMill,
            50 => ConstructionFarm,
            12 => ConstructionBarracks,
            72 => ConstructionPalisadeWall,
            87 => ConstructionRange,
            101 => ConstructionStable,
            103 => ConstructionBlacksmith,
            84 => ConstructionMarket,
            79 => ConstructionTower,
            792 => ConstructionGate,
            71 => ConstructionFeudalTownCenter,
            463 => ConstructionFeudalHouse,
            129 => ConstructionFeudalMill,
            563 => ConstructionFeudalLumberCamp,
            585 => ConstructionFeudalMiningCamp,
            498 => ConstructionFeudalBarracks,
            133 => ConstructionFeudalDock,
            _ => ConstructionOutpost,
        }
    }
    pub(super) fn is_foundation(self) -> bool {
        self == Self::FoundationDock
            || (Self::ConstructionTownCenter as u8..=Self::ConstructionFeudalDock as u8)
                .contains(&(self as u8))
    }
    pub fn building_id(self) -> Option<i32> {
        use SceneUnitKind::*;
        if self.is_foundation() && self != FoundationDock {
            return [
                109, 70, 562, 584, 68, 50, 12, 72, 598, 87, 101, 103, 84, 79, 792, 71, 463, 129,
                563, 585, 498, 133,
            ]
            .get((self as u8 - Self::ConstructionTownCenter as u8) as usize)
            .copied();
        }
        Some(match self {
            TownCenter | BuildingTownCenter => 109,
            House => 70,
            LumberCamp => 562,
            MiningCamp => 584,
            Mill => 68,
            Farm => 50,
            Barracks => 12,
            PalisadeWall => 72,
            Outpost => 598,
            Dock | FoundationDock => 45,
            ArcheryRange => 87,
            Stable => 101,
            Blacksmith => 103,
            Market => 84,
            WatchTower => 79,
            PalisadeGate | OpenPalisadeGate | GateFoundation => 792,
            FeudalTownCenter => 71,
            FeudalHouse => 463,
            FeudalMill => 129,
            FeudalLumberCamp => 563,
            FeudalMiningCamp => 585,
            FeudalBarracks => 498,
            FeudalDock => 133,
            _ => return None,
        })
    }
    pub(super) fn building_side(self) -> Option<f64> {
        if self == Self::TownCenter {
            return Some(4.0);
        }
        let slot = self.building_slot()?;
        Some(match slot {
            0 | 12 | 13 | 29 | 31 | 35 | 37 | 38 | 42 | 43 => 4.0,
            1..=4 | 10 | 14..=17 | 39 | 40 => 2.0,
            5 | 6 | 11 | 18 | 19 | 23..=28 | 32..=34 | 41 | 44 => 3.0,
            _ => 1.0,
        })
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct SceneUnit {
    pub kind: SceneUnitKind,
    pub id: EntityId,
    pub position: [f64; 2],
    pub moving: bool,
    /// Unit direction, or construction stage/wall variant for a building.
    pub facing: u8,
    pub selected: bool,
    pub elevation_meters: f64,
}

#[derive(Clone, Copy)]
pub struct SceneTerrain {
    pub position: [f64; 2],
    /// Display material: 0 grass, 1 dry grass, 2 dirt, 3 sand, 4 procedural
    /// rock, 5 water, 6 forest, 7 procedural snow, 8 interim procedural ice,
    /// 9 procedural mud, 10 procedural shallow/shore water. Not recipe identity.
    pub material: u8,
    pub elevation_meters: f64,
    pub surface: SceneTerrainSurface,
    /// None for terrain without landscape metadata (synthetic, non-map scenes);
    /// map chunks always supply an appearance.
    pub appearance: Option<SceneTerrainAppearance>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SceneTerrainAppearance {
    pub canopy_strength: u16,
    pub floor_strength: u16,
    pub palette: u8,
    pub exposure: u8,
    pub height_band: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SceneTerrainSurface {
    pub corner_game_height_levels: [i16; 4],
    pub kind: u8,
    pub triangulation: u8,
    pub water: u8,
}

impl SceneTerrainSurface {
    pub const PLATEAU: u8 = 0;
    pub const RAMP: u8 = 1;
    pub const CLIFF: u8 = 2;
    pub const CLIFF_LEDGE: u8 = 3;

    pub const fn is_cliff(self) -> bool {
        matches!(self.kind, Self::CLIFF | Self::CLIFF_LEDGE)
    }

    pub const fn flat(elevation_meters: f64) -> Self {
        Self {
            corner_game_height_levels: [elevation_meters as i16; 4],
            kind: Self::PLATEAU,
            triangulation: 0,
            water: 0,
        }
    }
}

/// A resource node, or one piece of scenery when `kind` is a scenery kind.
#[derive(Clone, Copy, PartialEq)]
pub struct SceneResource {
    pub id: u64,
    pub position: [f64; 2],
    pub kind: u8,
    pub visual_variant: u8,
    /// Semantic family, never automatic artwork approval. Zero is generic.
    pub visual_family: u8,
    pub elevation_meters: f64,
}

impl SceneResource {
    /// Cliff anchor marker. Faces are terrain geometry, never object billboards.
    pub const CLIFF: u8 = 16;
    /// A loose rock; visual_variant selects its pile and size.
    pub const ROCK: u8 = 17;
    /// Low scrub dressing; never enters resource/depletion or movement state.
    pub const SHRUB: u8 = 18;
    /// Low plant patches standing on the tile's projected ground.
    pub const GRASS: u8 = 19;
    /// Weathered stumps and fallen wood, purely visual.
    pub const DEADWOOD: u8 = 20;
}

/// Pure visual dressing: never a resource, blocker, gatherable or economy actor.
#[derive(Clone, Copy, PartialEq)]
pub struct SceneDecoration {
    pub position: [f64; 2],
    pub family: u8,
    pub visual_variant: u8,
    pub orientation: u8,
    pub elevation_meters: f64,
}
