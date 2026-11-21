//! Packages store inputs only, so a recipe-4 package written before rock
//! footprints blocked walking still loads under the same identity and now
//! blocks walking under its rocks.
use super::*;
use crate::{DecorationFamily, WaterKind};

/// Content hash of the default request at recipe 4, computed before rock
/// footprints blocked walking. It must never change with generation behavior.
const RECIPE_4_DEFAULT_HASH: &str =
    "789e3f349c7cb50c288d806de5463e294693d6c0f3d1a15195f15cc959cf59cf";

#[test]
fn recipe_four_package_keeps_its_identity_and_loads() {
    assert_eq!(crate::GENERATION_RECIPE_VERSION, 4);
    let package = MapPackage::new(1, MapRequest::default(), vec![]).expect("package");
    assert_eq!(package.generation_recipe_version, 4);
    assert_eq!(package.content_hash_hex(), RECIPE_4_DEFAULT_HASH);
    package.validate().expect("canonical");
    let stored = serde_json::to_string(&package).expect("serialize");
    let loaded: MapPackage = serde_json::from_str(&stored).expect("strict reader loads recipe 4");
    assert_eq!(loaded, package);
    loaded.validate().expect("validates");
}

#[test]
fn rocks_of_a_recipe_four_package_block_walking_without_regeneration() {
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
