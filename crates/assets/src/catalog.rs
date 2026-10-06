//! Compact, reviewed mappings from local trial assets to gameplay roles.
//!
//! Original pixels and inspection captures remain in ignored `local-assets/`.
//! Entries here identify only frames verified in that local pack.

/// Increment when the reviewed semantic mappings change.
pub const VERSION: u8 = 4;

pub mod candidates;
pub mod runtime;
mod topology;
pub use topology::TerrainFrameTopology;

/// Stage 1 remains within the existing single-atlas selection budget.
pub const MAX_REVIEWED_FRAMES: u32 = 656;

/// Paving 15018 is not natural rock. No imported rock sheet is approved.
pub const NATURAL_ROCK_FALLBACK: &str =
    "procedural neutral natural-rock ground; never substitute paved terrain 15018";

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
        interpretation: "10x10 periodic water texture atlas in x-major, reversed-y order",
        terrain_topology: Some(TerrainFrameTopology::PeriodicXMajorReversedY {
            columns: 10,
            rows: 10,
        }),
    },
];

/// Reviewed resource art that a renderer loads when its local pack provides
/// it. Its absence never prevents synthetic fixtures or partial local packs
/// from starting.
pub const OPTIONAL_RESOURCE_SOURCES: [SpriteSource; 5] = [
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
];

/// Native Forest/g_for (terrain record 10) maps to SLP 15011. Load only
/// frames 0..10 as coordinate-stable accents, not a complete periodic grid.
/// Optional so older partial packs keep their explicit dirt fallback.
pub const OPTIONAL_TERRAIN_SOURCES: [SpriteSource; 1] = [SpriteSource {
    role: AssetRole::ForestFloor,
    archive: "terrain.drs",
    id: 15011,
    frames: 10,
    interpretation: "ten coordinate-stable forest-floor accents; seamless full sheet unapproved",
    terrain_topology: Some(TerrainFrameTopology::CoordinateStableAccents),
}];

/// Object roles without reviewed source art. Keep this empty while every
/// supported resource role has an approved source mapping.
pub const UNAVAILABLE_RESOURCE_ART: [&str; 0] = [];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_unique_roles_and_nonempty_frame_ranges() {
        let mut roles = REQUIRED_RENDER_SOURCES
            .iter()
            .chain(OPTIONAL_RESOURCE_SOURCES.iter())
            .chain(OPTIONAL_TERRAIN_SOURCES.iter())
            .map(|source| source.role)
            .collect::<Vec<_>>();
        roles.sort_unstable();
        roles.dedup();
        assert_eq!(
            roles.len(),
            REQUIRED_RENDER_SOURCES.len()
                + OPTIONAL_RESOURCE_SOURCES.len()
                + OPTIONAL_TERRAIN_SOURCES.len()
        );
        assert!(
            REQUIRED_RENDER_SOURCES
                .iter()
                .chain(OPTIONAL_RESOURCE_SOURCES.iter())
                .chain(OPTIONAL_TERRAIN_SOURCES.iter())
                .all(|source| source.frames > 0)
        );
        assert_eq!(VERSION, 4);
        assert_eq!(OPTIONAL_TERRAIN_SOURCES[0].id, 15011);
        assert_eq!(OPTIONAL_TERRAIN_SOURCES[0].frames, 10);
        assert_eq!(
            REQUIRED_RENDER_SOURCES
                .iter()
                .chain(OPTIONAL_RESOURCE_SOURCES.iter())
                .chain(OPTIONAL_TERRAIN_SOURCES.iter())
                .map(|source| source.frames as usize)
                .sum::<usize>(),
            MAX_REVIEWED_FRAMES as usize
        );
    }

    #[test]
    fn paving_is_not_approved_as_natural_rock() {
        assert!(
            REQUIRED_RENDER_SOURCES
                .iter()
                .chain(OPTIONAL_RESOURCE_SOURCES.iter())
                .chain(OPTIONAL_TERRAIN_SOURCES.iter())
                .all(|source| source.role != AssetRole::Rock && source.id != 15018)
        );
        assert!(NATURAL_ROCK_FALLBACK.contains("procedural"));
        assert_eq!(
            OPTIONAL_TERRAIN_SOURCES[0].terrain_topology,
            Some(TerrainFrameTopology::CoordinateStableAccents)
        );
    }

    #[test]
    fn manifest_reference_preserves_the_drs_type_tag() {
        assert_eq!(
            OPTIONAL_RESOURCE_SOURCES[0].manifest_source(),
            "graphics.drs:[32, 112, 108, 115]:4652"
        );
    }

    #[test]
    fn reviewed_resource_sources_preserve_semantic_order_and_frame_counts() {
        assert_eq!(
            OPTIONAL_RESOURCE_SOURCES
                .iter()
                .map(|source| (source.role, source.id, source.frames))
                .collect::<Vec<_>>(),
            vec![
                (AssetRole::WoodTree, 4652, 14),
                (AssetRole::WoodTreeShadow, 2296, 14),
                (AssetRole::ForageBush, 2560, 4),
                (AssetRole::GoldDeposit, 4479, 7),
                (AssetRole::StoneDeposit, 4482, 7),
            ]
        );
    }

    #[test]
    fn every_reviewed_terrain_source_loads_its_complete_periodic_texture_grid() {
        assert!(
            REQUIRED_RENDER_SOURCES
                .iter()
                .filter(|source| matches!(
                    source.role,
                    AssetRole::TemperateGrass
                        | AssetRole::DryGrass
                        | AssetRole::Dirt
                        | AssetRole::Sand
                        | AssetRole::Water
                ))
                .all(|source| source.terrain_topology
                    == Some(TerrainFrameTopology::PeriodicXMajorReversedY {
                        columns: 10,
                        rows: 10
                    })
                    && source
                        .terrain_topology
                        .unwrap()
                        .supports_frames(source.frames))
        );
    }
}
