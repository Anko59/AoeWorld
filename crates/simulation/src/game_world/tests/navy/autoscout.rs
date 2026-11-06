//! Ship auto-scout explores one connected water body and ignores sealed water.
use super::*;
use crate::TileVisibility;

const HALF: i32 = 12;

/// A centre whose whole `(2 * HALF + 1)` square is open water.
fn open_water(world: &GameWorld) -> TileCoord {
    let wet = |x, y| {
        world
            .unit_tile_passable(UnitKind::Galley, TileCoord::new(x, y), &|| false)
            .unwrap()
    };
    for y in HALF + 2..120 {
        for x in HALF + 2..world_width(world) - HALF - 2 {
            if (-HALF..=HALF).all(|dy| (-HALF..=HALF).all(|dx| wet(x + dx, y + dy))) {
                return TileCoord::new(x, y);
            }
        }
    }
    panic!("prepared coast needs an open water square");
}

fn world_width(world: &GameWorld) -> i32 {
    world.config().width_tiles
}

/// Seal the square's border so water beyond it is not reachable.
fn seal(world: &mut GameWorld, c: TileCoord) {
    for d in -HALF - 1..=HALF + 1 {
        for (x, y) in [
            (c.x + d, c.y - HALF - 1),
            (c.x + d, c.y + HALF + 1),
            (c.x - HALF - 1, c.y + d),
            (c.x + HALF + 1, c.y + d),
        ] {
            world.economy.blocked_tiles.insert(TileCoord::new(x, y));
        }
    }
}

fn unexplored(world: &GameWorld, x: i32, y: i32) -> bool {
    world.visibility(PlayerId(0), TileCoord::new(x, y)) == TileVisibility::Unexplored
}

#[test]
fn warships_and_transports_explore_their_water_body_and_ignore_sealed_water() {
    for kind in [
        UnitKind::Galley,
        UnitKind::WarGalley,
        UnitKind::TransportShip,
    ] {
        let mut w = coast();
        let c = open_water(&w);
        seal(&mut w, c);
        let id = ship(&mut w, kind, c, 0);
        w.issue_auto_scout(PlayerId(0), id, true).unwrap();
        assert!(w.unit(id).unwrap().auto_scout);
        for _ in 0..6000 {
            w.advance();
            let p = w.unit(id).unwrap().position.tile_floor();
            assert!(
                (p.x - c.x).abs() <= HALF && (p.y - c.y).abs() <= HALF,
                "{kind:?} left the sealed water at {p:?}"
            );
        }
        let seen = (-HALF..=HALF)
            .flat_map(|dy| (-HALF..=HALF).map(move |dx| (dx, dy)))
            .filter(|&(dx, dy)| !unexplored(&w, c.x + dx, c.y + dy))
            .count();
        assert_eq!(
            seen,
            ((2 * HALF + 1) as usize).pow(2),
            "{kind:?} left water inside the body unexplored"
        );
        assert!(
            unexplored(&w, c.x + 3 * HALF, c.y),
            "{kind:?} saw sealed-off water"
        );
        assert!(
            w.movement_order(id).is_none(),
            "{kind:?} keeps sailing with nothing left to explore"
        );
        assert!(w.unit(id).unwrap().auto_scout);
    }
}

#[test]
fn only_warships_and_transports_may_auto_scout_and_manual_orders_cancel_it() {
    let mut w = coast();
    let c = open_water(&w);
    let fisher = ship(&mut w, UnitKind::FishingShip, c, 0);
    assert_eq!(
        w.issue_auto_scout(PlayerId(0), fisher, true),
        Err(EconomyError::WrongUnitKind)
    );
    let galley = ship(&mut w, UnitKind::Galley, c, 0);
    assert_eq!(
        w.issue_auto_scout(PlayerId(1), galley, true),
        Err(EconomyError::NotOwner)
    );
    w.issue_auto_scout(PlayerId(0), galley, true).unwrap();
    w.issue_stop(PlayerId(0), galley).unwrap();
    assert!(!w.unit(galley).unwrap().auto_scout);
}
