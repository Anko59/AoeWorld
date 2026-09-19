//! Compact, reviewed mappings from local trial assets to gameplay roles.
//!
//! Original pixels and inspection captures remain in ignored `local-assets/`.
//! Entries here identify only frames verified in that local pack.

/// Increment when the reviewed semantic mappings change.
pub const VERSION: u8 = 1;

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
    WoodTree,
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
}

impl SpriteSource {
    pub fn manifest_source(self) -> String {
        format!("{}:[32, 112, 108, 115]:{}", self.archive, self.id)
    }
}

/// Sources required for a local-pack game to start. They are deliberately
/// ordered by render role, not by incidental archive order.
pub const REQUIRED_RENDER_SOURCES: [SpriteSource; 8] = [
    SpriteSource {
        role: AssetRole::CavalryWalking,
        archive: "graphics.drs",
        id: 3008,
        frames: 50,
        interpretation: "five facing rows of ten walking animation frames",
    },
    SpriteSource {
        role: AssetRole::CavalryStanding,
        archive: "graphics.drs",
        id: 3004,
        frames: 50,
        interpretation: "five facing rows of ten standing frames",
    },
    SpriteSource {
        role: AssetRole::TemperateGrass,
        archive: "terrain.drs",
        id: 15008,
        frames: 10,
        interpretation: "terrain shape variants",
    },
    SpriteSource {
        role: AssetRole::DryGrass,
        archive: "terrain.drs",
        id: 15007,
        frames: 10,
        interpretation: "terrain shape variants",
    },
    SpriteSource {
        role: AssetRole::Dirt,
        archive: "terrain.drs",
        id: 15000,
        frames: 10,
        interpretation: "terrain shape variants",
    },
    SpriteSource {
        role: AssetRole::Sand,
        archive: "terrain.drs",
        id: 15010,
        frames: 10,
        interpretation: "terrain shape variants",
    },
    SpriteSource {
        role: AssetRole::Rock,
        archive: "terrain.drs",
        id: 15018,
        frames: 10,
        interpretation: "terrain shape variants",
    },
    SpriteSource {
        role: AssetRole::Water,
        archive: "terrain.drs",
        id: 15002,
        frames: 10,
        interpretation: "terrain shape variants",
    },
];

/// Reviewed resource art that a renderer loads when its local pack provides
/// it. Its absence never prevents synthetic fixtures or partial local packs
/// from starting.
pub const OPTIONAL_RESOURCE_SOURCES: [SpriteSource; 1] = [
    // Visually reviewed in the local trial viewer: fourteen distinct standing
    // broadleaf-tree variants with their original hotspots intact.
    SpriteSource {
        role: AssetRole::WoodTree,
        archive: "graphics.drs",
        id: 4652,
        frames: 14,
        interpretation: "individual broadleaf tree variants",
    },
];

/// Roles lacking reviewed source art. They stay absent from the render list so
/// public synthetic imagery and unrelated trial frames are never misreported
/// as evidence of real resource-art coverage.
pub const UNAVAILABLE_RESOURCE_ART: [&str; 3] =
    ["food forage bush", "gold deposit", "stone deposit"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_unique_roles_and_nonempty_frame_ranges() {
        let mut roles = REQUIRED_RENDER_SOURCES
            .iter()
            .chain(OPTIONAL_RESOURCE_SOURCES.iter())
            .map(|source| source.role)
            .collect::<Vec<_>>();
        roles.sort_unstable();
        roles.dedup();
        assert_eq!(
            roles.len(),
            REQUIRED_RENDER_SOURCES.len() + OPTIONAL_RESOURCE_SOURCES.len()
        );
        assert!(
            REQUIRED_RENDER_SOURCES
                .iter()
                .chain(OPTIONAL_RESOURCE_SOURCES.iter())
                .all(|source| source.frames > 0)
        );
        assert_eq!(VERSION, 1);
    }

    #[test]
    fn manifest_reference_preserves_the_drs_type_tag() {
        assert_eq!(
            OPTIONAL_RESOURCE_SOURCES[0].manifest_source(),
            "graphics.drs:[32, 112, 108, 115]:4652"
        );
    }
}
