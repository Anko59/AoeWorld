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
    FarmerWork,
    LongSwordsmanStand,
    LongSwordsmanWalk,
    LongSwordsmanAttack,
    LongSwordsmanDie,
    LongSwordsmanDecay,
    PikemanStand,
    PikemanWalk,
    PikemanAttack,
    PikemanDie,
    PikemanDecay,
    CrossbowmanStand,
    CrossbowmanWalk,
    CrossbowmanAttack,
    CrossbowmanDie,
    CrossbowmanDecay,
    EliteSkirmisherStand,
    EliteSkirmisherWalk,
    EliteSkirmisherAttack,
    EliteSkirmisherDie,
    EliteSkirmisherDecay,
    KnightStand,
    KnightWalk,
    KnightAttack,
    KnightDie,
    KnightDecay,
    LightCavalryStand,
    LightCavalryWalk,
    LightCavalryAttack,
    LightCavalryDie,
    LightCavalryDecay,
    MonkStand,
    MonkWalk,
    MonkAttack,
    MonkDie,
    MonkDecay,
    MangonelStand,
    MangonelWalk,
    MangonelAttack,
    MangonelDie,
    MangonelDecay,
    BatteringRamStand,
    BatteringRamWalk,
    BatteringRamAttack,
    BatteringRamDie,
    BatteringRamDecay,
    Stone,
    Galley,
    WarGalley,
    FireShip,
    DemolitionShip,
    TransportShip,
    Castle,
    Monastery,
    University,
    SiegeWorkshop,
    RubbleCastle,
    RubbleMonastery,
    RubbleUniversity,
    RubbleSiegeWorkshop,
    ConstructionCastle,
    ConstructionMonastery,
    ConstructionUniversity,
    ConstructionSiegeWorkshop,
    CastleTownCenter,
    CastleHouse,
    CastleLumberCamp,
    CastleMiningCamp,
    CastleMill,
    CastleBarracks,
    CastleDock,
    CastleArcheryRange,
    CastleStable,
    CastleBlacksmith,
    CastleMarket,
    CastleMonastery,
    BridgeATop,
    BridgeAMiddle,
    BridgeABottom,
    BridgeBTop,
    BridgeBMiddle,
    BridgeBBottom,
    KingStand,
    KingWalk,
    KingDie,
    KingDecay,
    GeneralStand,
    GeneralWalk,
    GeneralAttack,
    GeneralDie,
    GeneralDecay,
}

#[path = "scene_actors.rs"]
mod actors;

#[path = "scene_types/buildings.rs"]
mod buildings;

#[derive(Clone, Copy, PartialEq)]
pub struct SceneUnit {
    pub kind: SceneUnitKind,
    pub id: EntityId,
    pub position: [f64; 2],
    pub visual: SceneUnitVisual,
    /// Unit direction, or construction stage/wall variant for a building.
    pub facing: u8,
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
    /// 0 cursor only, 1 soft outline, 2 inner glow. Render-only state.
    pub highlight: u8,
    pub id: u64,
    pub position: [f64; 2],
    pub kind: u8,
    pub visual_variant: u8,
    /// Semantic family, never automatic artwork approval. Zero is generic.
    pub visual_family: u8,
    pub elevation_meters: f64,
}

impl SceneResource {
    /// Render-only stages; never gatherable protocol resource kinds.
    pub const CHOPPED_TREE: u8 = 4;
    pub const TREE_STUMP: u8 = 5;
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
    pub const RALLY_FLAG: u8 = 21;
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

pub use crate::player::SceneUnitVisual;

impl SceneUnit {
    /// Actors use the building-only damage-stage bits for the high animation
    /// bits. The scene stays 32 bytes; colour, selection and ghost bits remain
    /// independent. Apply after owner/selection flags have been constructed.
    pub fn with_actor_frame(mut self, frame: u16) -> Self {
        assert!(frame < 2048, "actor frame exceeds packed scene bound");
        self.facing = frame as u8;
        self.visual = self.visual.with_damage_stage((frame >> 8) as u8);
        self
    }
    pub(crate) fn actor_frame(self) -> u16 {
        u16::from(self.facing) | (u16::from(self.visual.damage_stage()) << 8)
    }
}
