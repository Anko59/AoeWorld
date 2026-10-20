//! Visually reviewed in the private pack; no original pixels are embedded.
use super::{AssetRole, SpriteSource};

pub(super) const SOURCES: [SpriteSource; 9] = [
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
        AssetRole::VillagerChopping,
        1535,
        75,
        "male villager chopping wood: five facing rows of fifteen frames",
    ),
    village(
        AssetRole::VillagerCarryingWood,
        1536,
        75,
        "male villager carrying wood: five facing rows of fifteen frames",
    ),
    village(
        AssetRole::VillagerForaging,
        1490,
        75,
        "male villager foraging berries: five facing rows of fifteen frames",
    ),
    village(
        AssetRole::VillagerCarryingFood,
        1493,
        75,
        "male villager carrying a food bag: five facing rows of fifteen frames",
    ),
    village(
        AssetRole::VillagerMiningGold,
        1545,
        75,
        "male gold miner swinging a pick: five facing rows of fifteen frames",
    ),
    village(
        AssetRole::VillagerCarryingOre,
        1552,
        75,
        "male villager carrying ore: shared gold and stone load, five rows of fifteen frames",
    ),
    village(
        AssetRole::VillagerMiningStone,
        1560,
        75,
        "male stone miner swinging a pick: five facing rows of fifteen frames",
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
