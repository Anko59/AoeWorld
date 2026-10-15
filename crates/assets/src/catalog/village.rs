//! Visually reviewed in the private pack; no original pixels are embedded.
use super::{AssetRole, SpriteSource};

pub(super) const SOURCES: [SpriteSource; 12] = [
    village(
        AssetRole::VillagerStanding,
        1479,
        75,
        "male villager idle: five facing rows of fifteen frames",
    ),
    village(
        AssetRole::VillagerWalking,
        1484,
        75,
        "male villager walking: five facing rows of fifteen frames",
    ),
    village(
        AssetRole::VillagerWorking,
        1535,
        75,
        "male villager chopping wood: five facing rows of fifteen frames",
    ),
    village(
        AssetRole::VillagerCarrying,
        1536,
        75,
        "male villager carrying wood: five facing rows of fifteen frames",
    ),
    village(
        AssetRole::TownCenterMain,
        891,
        1,
        "Dark Age town center main structure",
    ),
    village(
        AssetRole::TownCenterRoofLeft,
        3594,
        1,
        "Dark Age town center left roof; preserve native signed anchor",
    ),
    village(
        AssetRole::TownCenterSupportLeft,
        3595,
        1,
        "Dark Age town center left front support",
    ),
    village(
        AssetRole::TownCenterSupportsLeft,
        3596,
        1,
        "Dark Age town center left supports",
    ),
    village(
        AssetRole::TownCenterRoofRight,
        4610,
        1,
        "Dark Age town center right roof; preserve native signed anchor",
    ),
    village(
        AssetRole::TownCenterSupportsRight,
        4611,
        1,
        "Dark Age town center right supports",
    ),
    village(
        AssetRole::TownCenterSupportRight,
        4612,
        1,
        "Dark Age town center far right support",
    ),
    village(
        AssetRole::TownCenterShadow,
        889,
        1,
        "Dark Age town center paired ground shadow",
    ),
];

const fn village(
    role: AssetRole,
    id: u32,
    frames: u32,
    interpretation: &'static str,
) -> SpriteSource {
    SpriteSource {
        role,
        archive: "graphics.drs",
        id,
        frames,
        interpretation,
        terrain_topology: None,
    }
}
