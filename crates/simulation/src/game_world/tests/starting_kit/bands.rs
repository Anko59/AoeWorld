//! Fallback bands for constrained starts: wood in the wider band, partial herds.
use super::*;

/// Blocks every tile matching `wall` around `center` with a one-tile footprint.
fn wall_off(world: &mut GameWorld, center: TileCoord, wall: impl Fn(i32, i32) -> bool) {
    let Terrain::Map { overlay, .. } = &mut world.terrain else {
        unreachable!()
    };
    for y in -40_i32..=40 {
        for x in -40_i32..=40 {
            if wall(x, y) {
                overlay
                    .add_footprint(aoe_core::TileRect::from_xywh(
                        center.x + x,
                        center.y + y,
                        1,
                        1,
                    ))
                    .unwrap();
            }
        }
    }
}

fn kit_count(world: &GameWorld, kind: ResourceKind) -> usize {
    world
        .starting_resources()
        .iter()
        .filter(|n| n.kind == kind)
        .count()
}

#[test]
fn a_village_without_land_for_its_herds_is_reported_partial() {
    let mut world = super::barren();
    let center = TileCoord::new(2051, 2051);
    // Land ends 8 tiles from the Town Center: the full resource kit fits, but
    // the 9-15 tile boar and deer band is unreachable.
    wall_off(&mut world, center, |x, y| x * x + y * y >= 81);
    world
        .initialize_village(PlayerId(0), TileCoord::new(2048, 2048))
        .unwrap();
    assert_eq!(kit_count(&world, ResourceKind::Gold), 7);
    assert_eq!(kit_count(&world, ResourceKind::Stone), 5);
    assert!(kit_count(&world, ResourceKind::Wood) >= 12);
    assert!(
        !world
            .animals()
            .any(|a| matches!(a.kind, crate::AnimalKind::Boar | crate::AnimalKind::Deer))
    );
    assert_eq!(
        world.partial_starting_kits(),
        1,
        "missing herds are partial"
    );
}

#[test]
fn final_wood_slots_fall_back_to_the_wider_woodline_band() {
    let mut world = super::barren();
    let center = TileCoord::new(2051, 2051);
    // The 3-6 tile band is solid except the Town Center's exit ring (which
    // wood must not block) and one corridor; the wider band beyond it has
    // plenty of access-safe tiles.
    wall_off(&mut world, center, |x, y| {
        let ring = (-3..=2).contains(&x) && (-3..=2).contains(&y);
        (9..=36).contains(&(x * x + y * y)) && !ring && !(y == 0 && x > 0)
    });
    world
        .initialize_village(PlayerId(0), TileCoord::new(2048, 2048))
        .unwrap();
    let wood = kit_count(&world, ResourceKind::Wood);
    assert!((12..=13).contains(&wood), "wood {wood}");
    assert_eq!(world.partial_starting_kits(), 0);
}
