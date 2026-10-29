//! Compact, reviewed mappings from local trial assets to gameplay roles.
//!
//! Original pixels and inspection captures remain in ignored `local-assets/`.
//! Entries here identify only frames verified in that local pack.

/// Increment when the reviewed semantic mappings change.
pub const VERSION: u8 = 21;

pub mod buildings;
pub mod candidates;
pub mod interface;
pub mod packing;
pub mod runtime;
mod topology;
pub mod trees;
pub mod units;
mod village;
pub use topology::TerrainFrameTopology;

/// Reviewed selection ceiling, not an increase to the fixed five-page atlas.
pub const MAX_REVIEWED_FRAMES: u32 = 2_371;

/// Buildings represented by the current gameplay protocol. Other DAT recipes
/// remain available to offline tools without occupying the live match atlas.
pub const GAMEPLAY_BUILDING_IDS: [i32; 39] = [
    109, 70, 68, 562, 584, 50, 12, 72, 598, 45, 87, 101, 103, 84, 79, 792, 71, 463, 129, 563, 585,
    498, 133, 82, 104, 209, 49, 141, 464, 564, 586, 130, 132, 47, 10, 86, 18, 116, 31,
];

/// Paving and gravel are not mountain ground.
pub const NATURAL_ROCK_FALLBACK: &str =
    "native rock ground 15033; never substitute paved terrain 15018";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum AssetRole {
    CavalryWalking,
    CavalryStanding,
    TemperateGrass,
    DryGrass,
    Dirt,
    Sand,
    Rock,
    Water,
    ForestFloor,
    WoodTree,
    WoodTreeShadow,
    ForageBush,
    GoldDeposit,
    StoneDeposit,
    TreeConifer,
    TreePalm,
    WaterMedium,
    WaterDeep,
    Shallows,
    GrassDirt,
    WornGrass,
    LushGrass,
    Gravel,
    CliffFace,
    Snow,
    RockPile,
    Ice,
    Shrub,
    DeadwoodStump,
    DeadwoodLog,
    DeadwoodTree,
    VillagerStanding,
    VillagerWalking,
    VillagerChopping,
    VillagerCarryingWood,
    VillagerForaging,
    VillagerCarryingFood,
    VillagerCarryingGold,
    VillagerCarryingStone,
    VillagerMiningStone,
    RallyFlag,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpriteSource {
    pub role: AssetRole,
    pub archive: &'static str,
    pub id: u32,
    /// The exact, contiguous frame range the renderer may load.
    pub frames: u32,
    /// How the selected frames are interpreted by the renderer.
    pub interpretation: &'static str,
    /// Explicit sheet semantics; frame count alone never establishes periodicity.
    pub terrain_topology: Option<TerrainFrameTopology>,
}

impl SpriteSource {
    pub fn manifest_source(self) -> String {
        format!("{}:[32, 112, 108, 115]:{}", self.archive, self.id)
    }
}

/// Sources required for a local-pack game to start. They are deliberately
/// ordered by render role, not by incidental archive order.
pub const REQUIRED_RENDER_SOURCES: [SpriteSource; 7] = [
    SpriteSource {
        role: AssetRole::CavalryWalking,
        archive: "graphics.drs",
        id: 3008,
        frames: 50,
        interpretation: "five facing rows of ten walking animation frames",
        terrain_topology: None,
    },
    SpriteSource {
        role: AssetRole::CavalryStanding,
        archive: "graphics.drs",
        id: 3004,
        frames: 50,
        interpretation: "five facing rows of ten standing frames",
        terrain_topology: None,
    },
    SpriteSource {
        role: AssetRole::TemperateGrass,
        archive: "terrain.drs",
        id: 15008,
        frames: 100,
        interpretation: "10x10 periodic grass texture atlas in x-major, reversed-y order",
        terrain_topology: Some(TerrainFrameTopology::PeriodicXMajorReversedY {
            columns: 10,
            rows: 10,
        }),
    },
    SpriteSource {
        role: AssetRole::DryGrass,
        archive: "terrain.drs",
        id: 15007,
        frames: 100,
        interpretation: "10x10 periodic dry-grass texture atlas in x-major, reversed-y order",
        terrain_topology: Some(TerrainFrameTopology::PeriodicXMajorReversedY {
            columns: 10,
            rows: 10,
        }),
    },
    SpriteSource {
        role: AssetRole::Dirt,
        archive: "terrain.drs",
        id: 15000,
        frames: 100,
        interpretation: "10x10 periodic dirt texture atlas in x-major, reversed-y order",
        terrain_topology: Some(TerrainFrameTopology::PeriodicXMajorReversedY {
            columns: 10,
            rows: 10,
        }),
    },
    SpriteSource {
        role: AssetRole::Sand,
        archive: "terrain.drs",
        id: 15010,
        frames: 100,
        interpretation: "10x10 periodic sand texture atlas in x-major, reversed-y order",
        terrain_topology: Some(TerrainFrameTopology::PeriodicXMajorReversedY {
            columns: 10,
            rows: 10,
        }),
    },
    SpriteSource {
        role: AssetRole::Water,
        archive: "terrain.drs",
        id: 15002,
        frames: 100,
        interpretation: "10x10 periodic light shallow-water atlas in x-major, reversed-y order",
        terrain_topology: Some(TerrainFrameTopology::PeriodicXMajorReversedY {
            columns: 10,
            rows: 10,
        }),
    },
];

/// Reviewed resource art that a renderer loads when its local pack provides
/// it. Its absence never prevents synthetic fixtures or partial local packs
/// from starting.
pub const OPTIONAL_RESOURCE_SOURCES: [SpriteSource; 23] = [
    // Visually reviewed in the local trial viewer: fourteen distinct standing
    // broadleaf-tree variants with their original hotspots intact.
    SpriteSource {
        role: AssetRole::WoodTree,
        archive: "graphics.drs",
        id: 4652,
        frames: 14,
        interpretation: "individual broadleaf tree variants",
        terrain_topology: None,
    },
    // This shadow-only sequence is paired frame-for-frame with broadleaf art
    // 4652 and retains its original mask alpha and hotspot.
    SpriteSource {
        role: AssetRole::WoodTreeShadow,
        archive: "graphics.drs",
        id: 2296,
        frames: 14,
        interpretation: "paired broadleaf tree shadows for frames 0 through 13",
        terrain_topology: None,
    },
    SpriteSource {
        role: AssetRole::ForageBush,
        archive: "graphics.drs",
        id: 2560,
        frames: 4,
        interpretation: "four leafy berry-bush variants",
        terrain_topology: None,
    },
    SpriteSource {
        role: AssetRole::GoldDeposit,
        archive: "graphics.drs",
        id: 4479,
        frames: 7,
        interpretation: "seven gold-ore deposit variants",
        terrain_topology: None,
    },
    SpriteSource {
        role: AssetRole::StoneDeposit,
        archive: "graphics.drs",
        id: 4482,
        frames: 7,
        interpretation: "seven stone-ore deposit variants",
        terrain_topology: None,
    },
    SpriteSource {
        role: AssetRole::TreeConifer,
        archive: "graphics.drs",
        id: 4654,
        frames: 9,
        interpretation: "healthy resource subset 1,2,3,4,7,8 only; alpha silhouette, no approved paired shadow",
        terrain_topology: None,
    },
    SpriteSource {
        role: AssetRole::TreePalm,
        archive: "graphics.drs",
        id: 4653,
        frames: 13,
        interpretation: "single resource subset 0,1,2,3,5,6,8,10,11,12 only; alpha silhouette, no approved paired shadow",
        terrain_topology: None,
    },
    // Visually reviewed against a three-by-three tile grid: eleven rock faces
    // that fall toward the camera, hotspot at the centre of the footprint.
    // The other fourteen slots are one-pixel placeholders and are never drawn.
    SpriteSource {
        role: AssetRole::CliffFace,
        archive: "graphics.drs",
        id: 226,
        frames: 25,
        interpretation: "camera-facing cliff faces 1,2,3,4,5,15,16,17,18,19,24 on a 3x3 tile footprint; other slots are placeholders",
        terrain_topology: None,
    },
    // Seven plain grey rock piles: the stone-deposit shapes without ore
    // glints. Drawn reduced as loose rocks, never as a resource.
    SpriteSource {
        role: AssetRole::RockPile,
        archive: "graphics.drs",
        id: 1034,
        frames: 7,
        interpretation: "seven plain grey rock piles for visual rocks",
        terrain_topology: None,
    },
    // Full native prefixes retain source slots and signed hotspots. Selection
    // excludes the three tall cactus forms; only low succulent scrub is dressing.
    SpriteSource {
        role: AssetRole::Shrub,
        archive: "graphics.drs",
        id: 4474,
        frames: 6,
        interpretation: "low succulent scrub frames 3,4,5 only; no tall cacti or forage bushes",
        terrain_topology: None,
    },
    SpriteSource {
        role: AssetRole::DeadwoodStump,
        archive: "graphics.drs",
        id: 1035,
        frames: 3,
        interpretation: "three weathered stumps; preserve signed and outside hotspots",
        terrain_topology: None,
    },
    SpriteSource {
        role: AssetRole::DeadwoodLog,
        archive: "graphics.drs",
        id: 1252,
        frames: 2,
        interpretation: "two low mossy fallen logs; nonblocking deadwood dressing",
        terrain_topology: None,
    },
    SpriteSource {
        role: AssetRole::DeadwoodTree,
        archive: "graphics.drs",
        id: 1262,
        frames: 1,
        interpretation: "occasional bare snag; nonblocking deadwood, no wood resource",
        terrain_topology: None,
    },
    village::SOURCES[0],
    village::SOURCES[1],
    village::SOURCES[2],
    village::SOURCES[3],
    village::SOURCES[4],
    village::SOURCES[5],
    village::SOURCES[6],
    village::SOURCES[7],
    village::SOURCES[8],
    SpriteSource {
        role: AssetRole::RallyFlag,
        archive: "graphics.drs",
        id: 4480,
        frames: 10,
        interpretation: "DAT unit 720 FLAGM, graphic 4351 FLAGM_NN: gather-point flag",
        terrain_topology: None,
    },
];

/// Native Forest/g_for (terrain record 10) maps to SLP 15011. Load only
/// frames 0..10 as coordinate-stable accents, not a complete periodic grid.
/// Optional so partial packs keep their explicit dirt fallback.
///
/// Water depth: required water 15002 is the light shallow sheet. Medium
/// 15016, deep 15015 and marsh shallows 15014 are periodic sheets of the same
/// layout; a pack without one draws the shallow sheet in its place, never a
/// flat tint. AoK DAT terrain 4 (Shallows / g_sha, SLP 15014) marks
/// generated Ford crossings that both land units and ships can use.
/// The four ground-variety sheets were reviewed as 10×10 periodic montages in
/// the same x-major, reversed-y order as the required sheets: grass and dirt
/// mix 15006, grass with brown patches 15009, darker grass 15001 and pebbly
/// beach 15017. A pack without one draws its base sheet instead.
pub const OPTIONAL_TERRAIN_SOURCES: [SpriteSource; 11] = [
    SpriteSource {
        role: AssetRole::ForestFloor,
        archive: "terrain.drs",
        id: 15011,
        frames: 10,
        interpretation: "ten coordinate-stable forest-floor accents; seamless full sheet unapproved",
        terrain_topology: Some(TerrainFrameTopology::CoordinateStableAccents),
    },
    SpriteSource {
        role: AssetRole::WaterMedium,
        archive: "terrain.drs",
        id: 15016,
        frames: 100,
        interpretation: "10x10 periodic medium-water atlas in x-major, reversed-y order",
        terrain_topology: Some(TerrainFrameTopology::PeriodicXMajorReversedY {
            columns: 10,
            rows: 10,
        }),
    },
    SpriteSource {
        role: AssetRole::WaterDeep,
        archive: "terrain.drs",
        id: 15015,
        frames: 100,
        interpretation: "10x10 periodic deep-water atlas in x-major, reversed-y order",
        terrain_topology: Some(TerrainFrameTopology::PeriodicXMajorReversedY {
            columns: 10,
            rows: 10,
        }),
    },
    SpriteSource {
        role: AssetRole::Shallows,
        archive: "terrain.drs",
        id: 15014,
        frames: 100,
        interpretation: "10x10 periodic marsh-shallows atlas; no generated fords",
        terrain_topology: Some(TerrainFrameTopology::PeriodicXMajorReversedY {
            columns: 10,
            rows: 10,
        }),
    },
    variety(
        AssetRole::GrassDirt,
        15006,
        "10x10 periodic grass and dirt mix",
    ),
    variety(
        AssetRole::WornGrass,
        15009,
        "10x10 periodic grass with brown patches",
    ),
    variety(AssetRole::LushGrass, 15001, "10x10 periodic darker grass"),
    variety(AssetRole::Gravel, 15017, "10x10 periodic pebbly beach"),
    variety(AssetRole::Rock, 15033, "10x10 periodic natural rock ground"),
    SpriteSource {
        role: AssetRole::Ice,
        archive: "terrain.drs",
        id: 15024,
        frames: 64,
        interpretation: "8x8 periodic cracked ice; frozen water only",
        terrain_topology: Some(TerrainFrameTopology::PeriodicXMajorReversedY {
            columns: 8,
            rows: 8,
        }),
    },
    // The base trial and CI fixtures lack this sheet; keep startup independent
    // of the expanded mountain pack, just as for optional native rock.
    variety(AssetRole::Snow, 15026, "10x10 periodic soft snow ground"),
];

const fn variety(role: AssetRole, id: u32, interpretation: &'static str) -> SpriteSource {
    SpriteSource {
        role,
        archive: "terrain.drs",
        id,
        frames: 100,
        interpretation,
        terrain_topology: Some(TerrainFrameTopology::PeriodicXMajorReversedY {
            columns: 10,
            rows: 10,
        }),
    }
}

/// Object roles without reviewed source art. Keep this empty while every
/// supported resource role has an approved source mapping.
pub const UNAVAILABLE_RESOURCE_ART: [&str; 0] = [];

#[path = "catalog/tests/private_pack.rs"]
#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod private_pack;

#[path = "catalog/tests/contract.rs"]
#[cfg(test)]
mod tests;
