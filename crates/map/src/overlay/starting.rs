use crate::{ObjectKind, ResourceKind, ResourceNode};
use aoe_core::TileCoord;

impl ResourceNode {
    /// The current resource identity reserves bit 37 for starting resources,
    /// bits 38..39 for kind, and retains the map's tile coordinate fields.
    pub fn starting(tile: TileCoord, kind: ResourceKind) -> Option<Self> {
        if tile.x < 0 || tile.y < 0 || tile.x >= 1 << 18 || tile.y >= 1 << 18 {
            return None;
        }
        Self::starting_from_id(
            (1 << 37) | ((kind as u64) << 38) | ((tile.y as u64) << 19) | ((tile.x as u64) << 1),
        )
    }

    pub fn starting_from_id(id: u64) -> Option<Self> {
        if id & (1 << 37) == 0 || id & 1 != 0 || id >> 40 != 0 {
            return None;
        }
        let (kind, object, initial_amount) = match id >> 38 {
            0 => (ResourceKind::Food, ObjectKind::ForageBush, 125),
            1 => (ResourceKind::Wood, ObjectKind::Tree, 100),
            2 => (ResourceKind::Gold, ObjectKind::GoldDeposit, 800),
            3 => (ResourceKind::Stone, ObjectKind::StoneDeposit, 350),
            _ => return None,
        };
        Some(Self {
            id,
            tile: TileCoord::new(
                ((id >> 1) & 0x3_ffff) as i32,
                ((id >> 19) & 0x3_ffff) as i32,
            ),
            kind,
            object,
            initial_amount,
            visual_variant: 0,
        })
    }
}
