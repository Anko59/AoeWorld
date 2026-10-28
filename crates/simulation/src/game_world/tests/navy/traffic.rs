//! Water traffic uses ship radii and must continue in both directions.
use super::*;

#[test]
fn four_fishing_ships_and_two_galleys_pass_in_a_three_tile_channel() {
    let mut w = coast();
    w.set_fog_disabled(true);
    let (_, water) = bank(&w);
    let base = TileCoord::new(water.x + 2, water.y + 4);
    for x in base.x - 2..base.x + 16 {
        for y in 0..w.config().height_tiles {
            if !(base.y..base.y + 3).contains(&y) {
                w.economy.blocked_tiles.insert(TileCoord::new(x, y));
            }
        }
    }
    let mut actors = Vec::new();
    for n in 0..6 {
        let east = n < 3;
        let start = TileCoord::new(base.x + if east { n } else { 12 - n % 3 }, base.y + n % 3);
        let goal = at(TileCoord::new(
            base.x + if east { 12 - n } else { n % 3 },
            base.y + n % 3,
        ));
        let kind = if n % 3 == 2 {
            UnitKind::Galley
        } else {
            UnitKind::FishingShip
        };
        let id = ship(&mut w, kind, start, 0);
        w.issue_move(id, goal).unwrap();
        actors.push((id, goal, w.unit(id).unwrap().position, 0));
    }
    for tick in 0..1000 {
        w.advance();
        for (n, a) in actors.iter().enumerate() {
            for b in &actors[n + 1..] {
                let a = w.unit(a.0).unwrap().position;
                let b = w.unit(b.0).unwrap().position;
                assert!(
                    i64::from(a.x - b.x).pow(2) + i64::from(a.y - b.y).pow(2) >= 1024_i64.pow(2),
                    "hulls overlap at tick {tick}"
                );
            }
        }
        for (id, goal, previous, stalled) in &mut actors {
            let unit = w.unit(*id).unwrap();
            if unit.position != *previous || unit.position == *goal {
                *stalled = 0;
            } else {
                *stalled += 1;
            }
            assert!(
                *stalled <= 160,
                "ship {id:?} stalled at tick {tick}: {unit:?}"
            );
            assert!((base.y..base.y + 3).contains(&unit.position.tile_floor().y));
            *previous = unit.position;
        }
    }
    for (id, goal, _, _) in actors {
        assert_eq!(w.unit(id).unwrap().position, goal, "{id:?}");
    }
}

#[test]
fn a_demolition_ship_can_close_to_hull_contact_from_out_of_range() {
    let mut w = coast();
    w.set_fog_disabled(true);
    let (_, water) = bank(&w);
    let demo = ship(&mut w, UnitKind::DemolitionShip, water, 0);
    let target = ship(
        &mut w,
        UnitKind::Galley,
        TileCoord::new(water.x + 10, water.y),
        1,
    );
    w.issue_attack(PlayerId(0), demo, target).unwrap();
    for _ in 0..500 {
        w.advance();
        if w.unit(demo).is_none_or(|u| u.hp == 0) {
            return;
        }
    }
    panic!("demolition ship never reached its one-tile hull contact range");
}
