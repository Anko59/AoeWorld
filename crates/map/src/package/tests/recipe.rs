//! Current package identity and derived rock walkability.
use super::*;
use crate::{DecorationFamily, WaterKind};

/// Content hash of the default request under the current recipe.
const DEFAULT_PACKAGE_HASH: &str =
    "339c8def0dc963800bcd3ed43ab06c59debcae605f8cd88908d99da9c5485182";

#[test]
fn current_recipe_package_has_its_canonical_identity_and_loads() {
    assert_eq!(crate::GENERATION_RECIPE_VERSION, 5);
    let package = MapPackage::new(1, MapRequest::default(), vec![]).expect("package");
    assert_eq!(package.generation_recipe_version, 5);
    assert_eq!(package.content_hash_hex(), DEFAULT_PACKAGE_HASH);
    package.validate().expect("canonical");
    let stored = serde_json::to_string(&package).expect("serialize");
    let loaded: MapPackage =
        serde_json::from_str(&stored).expect("strict reader loads the current recipe");
    assert_eq!(loaded, package);
    loaded.validate().expect("validates");
}

#[test]
fn rocks_of_a_current_package_block_walking() {
    let package = MapPackage::new(1, MapRequest::default(), vec![]).expect("package");
    let generator = package.generator();
    let mut piles = 0;
    for y in 0..8 {
        for x in 0..8 {
            let chunk = generator
                .landscape_chunk_with_cancel(x, y, &|| false)
                .expect("chunk");
            for rock in chunk
                .decorations
                .iter()
                .filter(|d| d.family == DecorationFamily::Stone)
            {
                piles += 1;
                for (dx, dy) in [(-1, -1), (0, 0), (1, 1), (-1, 1), (1, -1)] {
                    let at = aoe_core::TileCoord::new(rock.tile.x + dx, rock.tile.y + dy);
                    let (walk, _) = generator
                        .tile_and_node_with_cancel(at, &|| false)
                        .expect("tile")
                        .expect("in bounds");
                    assert_eq!(walk.water, WaterKind::None);
                    assert!(
                        !walk.passable,
                        "rock at {:?} leaves {at:?} walkable",
                        rock.tile
                    );
                }
            }
        }
    }
    assert!(piles > 0, "the default map grows loose rocks");
}
