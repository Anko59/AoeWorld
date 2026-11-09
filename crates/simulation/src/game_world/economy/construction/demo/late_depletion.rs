//! The late-game start's worked-out land: the owner's two-hour world had cut
//! the woods and emptied the mines and bushes around its villages, so its
//! gatherers walked far for every load. `seed_depletion` exhausts every land
//! resource within a radius of the village (fish stay), and `far_resources`
//! lists what stands just beyond it, nearest first, for the seeded workers'
//! first orders (the ordinary replacement search only looks a few tiles).
use crate::{GameWorld, Terrain};
use aoe_core::TileCoord;
use aoe_map::{ObjectKind, ResourceKind, ResourceNode};

/// Tiles beyond the depleted disc searched for the workers' first resources.
const FAR_BAND: i32 = 48;

fn land(node: &ResourceNode) -> bool {
    !matches!(node.object, ObjectKind::ShoreFish | ObjectKind::DeepFish)
}

impl GameWorld {
    /// Every resource node on the disc of `radius` tiles around `center`, in
    /// row order (generator objects, else the overlay's starting nodes).
    fn disc_nodes(&self, center: TileCoord, radius: i32) -> Vec<(i64, ResourceNode)> {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return Vec::new();
        };
        let min = TileCoord::new(center.x - radius, center.y - radius);
        let max = TileCoord::new(center.x + radius, center.y + radius);
        let mut nodes: Vec<ResourceNode> = overlay.starting_nodes_in(min, max).collect();
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                if let Some(node) = generator.object_at(TileCoord::new(x, y)) {
                    nodes.push(node);
                }
            }
        }
        let squared = i64::from(radius).pow(2);
        let mut out: Vec<_> = nodes
            .into_iter()
            .filter(land)
            .map(|n| {
                let (dx, dy) = (n.tile.x - center.x, n.tile.y - center.y);
                (i64::from(dx).pow(2) + i64::from(dy).pow(2), n)
            })
            .filter(|(d2, _)| *d2 <= squared)
            .collect();
        out.sort_by_key(|(d2, n)| (*d2, n.id));
        out.dedup_by_key(|(_, n)| n.id);
        out
    }

    /// Exhausts the land resources within `radius` tiles of `center`;
    /// returns how many nodes it emptied.
    pub(super) fn seed_depletion(&mut self, center: TileCoord, radius: i32) -> u64 {
        if radius <= 0 {
            return 0;
        }
        let mut emptied = 0;
        for (_, node) in self.disc_nodes(center, radius) {
            let Terrain::Map { generator, overlay } = &self.terrain else {
                break;
            };
            let Some(left) = overlay.remaining(generator, node.id).filter(|n| *n > 0) else {
                continue;
            };
            if self.deplete_resource(node.id, left).is_ok() {
                emptied += 1;
            }
        }
        emptied
    }

    /// The available land nodes of `kind` within `FAR_BAND` tiles beyond
    /// `radius`, nearest to `center` first.
    pub(super) fn far_resources(
        &self,
        center: TileCoord,
        radius: i32,
        kind: ResourceKind,
    ) -> Vec<ResourceNode> {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return Vec::new();
        };
        self.disc_nodes(center, radius + FAR_BAND)
            .into_iter()
            .map(|(_, n)| n)
            .filter(|n| n.kind == kind && overlay.remaining(generator, n.id).unwrap_or(0) > 0)
            .collect()
    }
}
