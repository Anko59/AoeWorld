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
        2589,
        75,
        "male forager picking berries (DAT unit 120 VMFOR task): five rows of fifteen frames",
    ),
    village(
        AssetRole::VillagerCarryingFood,
        2592,
        75,
        "male forager carrying berries (DAT unit 120 VMFOR carrying): five rows of fifteen frames",
    ),
    village(
        AssetRole::VillagerCarryingGold,
        2117,
        75,
        "male gold miner carrying gold (DAT unit 579 VMGLD_CN): five rows of fifteen frames",
    ),
    village(
        AssetRole::VillagerCarryingStone,
        1552,
        75,
        "male stone miner carrying stone (DAT unit 124 VMMIN_CN): five rows of fifteen frames",
    ),
    village(
        AssetRole::VillagerMiningStone,
        1560,
        75,
        "miner's pick, shared by gold (unit 579) and stone (unit 124) miners: 5x15 frames",
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
