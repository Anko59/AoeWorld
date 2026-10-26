use super::super::seats::{FarWalk, SPREAD, match_ideals};
use aoe_core::TileCoord;
use aoe_simulation::{LandMasses, Terrain};

/// An ideal point 600 tiles out at sea: the walk reaches land without
/// ever running the costly test on open water.
#[test]
fn sea_anchor_does_not_probe_open_water() {
    let ideal = TileCoord::new(100, 4000);
    let mut walk = FarWalk::new(ideal, 16_000);
    let mut costly = 0;
    let mut found = Vec::new();
    while found.len() < 10 {
        let tile = walk.next(&mut |tile| tile.x < 700, &mut |_| {
            costly += 1;
            true
        });
        found.push(tile.expect("land lies east of the sea"));
    }
    assert!(found.iter().all(|tile| tile.x >= 700));
    // Only dry tiles reach the costly test: ten spread tiles need 10 x 24.
    assert_eq!(costly, 10 * SPREAD - SPREAD + 1);
    // Rings 0..=600 at stride 16 hold well under 100,000 tiles.
    assert!(walk.examined < 100_000, "examined {}", walk.examined);
}

#[test]
fn walk_is_bounded_without_land() {
    let mut walk = FarWalk::new(TileCoord::new(50, 50), 200);
    assert_eq!(walk.next(&mut |_| true, &mut |_| true), None);
    assert!(walk.examined < 20_000, "examined {}", walk.examined);
}

#[test]
fn a_seed_decides_who_starts_where() {
    let canonical = match_ideals(16_000, 100, None);
    let layout = |seed| match_ideals(16_000, 100, Some(seed));
    assert_eq!(layout(7), layout(7), "the same seed gives the same layout");
    assert_eq!(layout(7).len(), 100);
    let humans: std::collections::BTreeSet<_> = (1..=30).map(|seed| layout(seed)[0]).collect();
    assert!(
        humans.len() >= 25,
        "slot 0 started in {} places",
        humans.len()
    );
    assert_ne!(layout(1), canonical);
    // Fair: a shuffle only moves who sits where, and each cell keeps its
    // own neighbourhood (no two ideals closer than half a cell).
    let cell = (16_000 - 2 * 1_600) / 10;
    for seed in [1, 2, 3] {
        let ideals = layout(seed);
        for (at, a) in ideals.iter().enumerate() {
            for b in &ideals[at + 1..] {
                assert!((a.x - b.x).abs().max((a.y - b.y).abs()) >= cell / 2);
            }
        }
    }
    // A small ring turns: two players do not always sit on the same diagonal.
    let rings: std::collections::BTreeSet<_> = (1..=20)
        .map(|seed| match_ideals(4_000, 2, Some(seed))[0])
        .collect();
    assert!(rings.len() >= 10);
}

#[test]
fn villages_are_confined_to_the_main_continent() {
    let Some(masses) = LandMasses::from_cells(20, |x, y| {
        (x < 10 && y < 18) || ((14..18).contains(&x) && (4..8).contains(&y))
    }) else {
        panic!("land");
    };
    assert!(Terrain::on_main_land(
        Some(&masses),
        TileCoord::new(100, 100)
    ));
    assert!(!Terrain::on_main_land(
        Some(&masses),
        TileCoord::new(15 * 32, 5 * 32)
    ));
    assert!(Terrain::on_main_land(None, TileCoord::new(15 * 32, 5 * 32)));
}
