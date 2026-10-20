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
    FoundationDock,
    RubbleDock,
    ConstructionTownCenter,
    ConstructionHouse,
    ConstructionLumberCamp,
    ConstructionMiningCamp,
    ConstructionMill,
    ConstructionFarm,
    ConstructionBarracks,
    ConstructionPalisadeWall,
    ConstructionOutpost,
}

impl SceneUnitKind {
    /// Flat tags keep the scene DTO at 32 bytes, including actor animations.
    pub fn actor(index: u8) -> Self {
        const ACTORS: [SceneUnitKind; 50] = [
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
        ];
        ACTORS
            .get(usize::from(index))
            .copied()
            .unwrap_or(Self::Scout)
    }
    pub(super) fn actor_slot(self) -> Option<usize> {
        (self as u8)
            .checked_sub(Self::MilitiaStand as u8)
            .filter(|index| *index < 50)
            .map(usize::from)
    }

    /// The typed building roles are contiguous in the atlas's semantic order.
    pub fn building(slot: u8) -> Self {
        if slot >= 23 {
            return [Self::Dock, Self::FoundationDock, Self::RubbleDock]
                [usize::from(slot - 23).min(2)];
        }
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
        ][usize::from(slot.min(21))]
    }
    pub(super) fn building_slot(self) -> Option<u8> {
        if self.is_foundation() {
            return Some(match self {
                Self::ConstructionTownCenter => 12,
                Self::ConstructionFarm | Self::ConstructionBarracks => 11,
                Self::ConstructionPalisadeWall | Self::ConstructionOutpost => 9,
                _ => 10,
            });
        }
        (self as u8)
            .checked_sub(Self::BuildingTownCenter as u8)
            .filter(|s| *s <= 21)
    }
    pub fn foundation(id: i16) -> Self {
        use SceneUnitKind::*;
        match id {
            109 => ConstructionTownCenter,
            70 => ConstructionHouse,
            562 => ConstructionLumberCamp,
            584 => ConstructionMiningCamp,
            68 => ConstructionMill,
            50 => ConstructionFarm,
            12 => ConstructionBarracks,
            72 => ConstructionPalisadeWall,
            _ => ConstructionOutpost,
        }
    }
    pub(super) fn is_foundation(self) -> bool {
        self as u8 >= Self::ConstructionTownCenter as u8
    }
    pub fn building_id(self) -> Option<i32> {
        if self == Self::TownCenter {
            return Some(109);
        }
        let slot = if self.is_foundation() {
            self as u8 - Self::ConstructionTownCenter as u8
        } else {
            self.building_slot()?
        };
        [109, 70, 562, 584, 68, 50, 12, 72, 598]
            .get(usize::from(slot))
            .copied()
    }
    pub(super) fn building_side(self) -> Option<f64> {
        if self == Self::TownCenter {
            return Some(4.0);
        }
        let slot = self.building_slot()?;
        if slot >= 23 {
            return Some(3.0);
        }
        Some(if (9..=12).contains(&slot) {
            f64::from(slot - 8)
        } else {
            f64::from(
                [4, 2, 2, 2, 2, 3, 3, 1, 1]
                    [usize::from(if slot >= 13 { slot - 13 } else { slot }).min(8)],
            )
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
