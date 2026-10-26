use super::land_masses::LandMasses;
use aoe_core::TileCoord;

/// A 20 x 20 map: a continent in the west, a smaller island across a channel,
/// and an islet touching the continent only at a corner.
fn grid(x: i32, y: i32) -> bool {
    (x < 10 && y < 18) || (14..18).contains(&x) && (4..8).contains(&y) || (x == 10 && y == 18)
}

#[test]
fn an_island_is_not_the_main_land_mass() {
    let Some(masses) = LandMasses::from_cells(20, grid) else {
        panic!("the grid has land");
    };
    assert_eq!(masses.count(), 3, "continent, island and diagonal islet");
    let tile = |x: i32, y: i32| TileCoord::new(x * 32 + 5, y * 32 + 5);
    assert!(masses.is_main(tile(3, 3), 32));
    assert!(
        !masses.is_main(tile(15, 5), 32),
        "island across the channel"
    );
    assert!(
        !masses.is_main(tile(10, 18), 32),
        "corner contact is no bridge"
    );
    assert!(!masses.is_main(tile(12, 3), 32), "channel water");
    assert!(!masses.is_main(TileCoord::new(-1, 5), 32));
    assert!(!masses.is_main(TileCoord::new(20 * 32, 5), 32));
}

#[test]
fn a_water_world_has_no_masses() {
    assert!(LandMasses::from_cells(8, |_, _| false).is_none());
}
