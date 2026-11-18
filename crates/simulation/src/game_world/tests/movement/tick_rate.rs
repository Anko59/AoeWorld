//! The simulation rate changes how often the world steps, not how long
//! things take: a march lasts the same game time at 10 Hz as at 20 Hz.
use super::*;

/// Seconds of game time an unseen formation of eight takes to cross 40
/// tiles at `tick_hz`, and how far its members end from the goal.
fn march(tick_hz: u32) -> (f64, i64) {
    let config = WorldConfig {
        tick_hz,
        ..WorldConfig::new(160, 160, Seed(3)).unwrap()
    };
    let mut world = GameWorld::new(config).unwrap();
    // A human seat that sees nothing of the march puts it on the stride tier,
    // where its leader once banked nothing and trailed at a quarter speed.
    world.set_human_seats(&[PlayerId(1)]);
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let ids: Vec<_> = (0..8)
        .map(|n| {
            world
                .spawn_kind(
                    PlayerId(0),
                    crate::UnitKind::Militia,
                    at(20 + n % 4, 20 + n / 4),
                )
                .unwrap()
        })
        .collect();
    let goal = at(60, 21);
    world
        .issue_formation_move(
            PlayerId(0),
            &ids,
            aoe_core::Formation::Line,
            goal,
            false,
            false,
        )
        .unwrap();
    let mut started = false;
    for tick in 1..=120 * tick_hz {
        world.advance();
        let moving = ids.iter().any(|id| world.unit(*id).unwrap().moving);
        started |= moving;
        if started && !moving {
            let farthest = ids
                .iter()
                .map(|id| {
                    let p = world.unit(*id).unwrap().position;
                    (i64::from(p.x - goal.x).abs()).max(i64::from(p.y - goal.y).abs())
                })
                .max()
                .unwrap_or_default();
            return (f64::from(tick) / f64::from(tick_hz), farthest);
        }
    }
    panic!("the march at {tick_hz} Hz never arrived");
}

#[test]
fn a_march_lasts_the_same_game_time_at_10_and_20_hz() {
    let (fast, fast_spread) = march(20);
    let (slow, slow_spread) = march(10);
    assert!(
        (slow - fast).abs() <= fast * 0.1,
        "10 Hz took {slow} s, 20 Hz {fast} s"
    );
    // Both end in formation around the goal, not strung out along the way.
    let tiles = i64::from(aoe_core::FIXED_SUBUNITS_PER_TILE);
    assert!(
        slow_spread <= 4 * tiles && fast_spread <= 4 * tiles,
        "{slow_spread} {fast_spread}"
    );
}
