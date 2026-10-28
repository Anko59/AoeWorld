//! S7: two players' units meet head-on on a bridge one tile wide. Records
//! what today's hard-circle bodies do; PR 8 of the group-movement rewrite
//! (docs/movement/group-movement-rewrite.md, section 6) turns it into a pass.
use super::*;

fn tile(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

#[test]
fn two_players_meeting_on_a_one_tile_bridge() {
    let (mut world, worker) = setup();
    let bridge = world
        .issue_build_bridge(
            PlayerId(0),
            &[worker],
            TileCoord::new(29, 20),
            TileCoord::new(34, 20),
            false,
        )
        .unwrap();
    advance(&mut world, 20 * 100);
    assert!(world.building(bridge).unwrap().is_complete());
    let west = (0..3)
        .map(|n| world.spawn_kind(PlayerId(0), UnitKind::Militia, tile(27 - n, 20)))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let east = (0..3)
        .map(|n| world.spawn_kind(PlayerId(1), UnitKind::Militia, tile(36 + n, 20)))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for &id in &west {
        world.issue_move(id, tile(38, 20)).unwrap();
    }
    for &id in &east {
        world.issue_move(id, tile(25, 20)).unwrap();
    }
    let all = [west.clone(), east.clone()].concat();
    let (mut first_cross, mut all_across, mut last_move) = (None, None, 0);
    let mut previous = all
        .iter()
        .map(|&id| world.unit(id).unwrap().position)
        .collect::<Vec<_>>();
    for tick in 1..=2000_u32 {
        world.advance();
        for (n, &id) in all.iter().enumerate() {
            let at = world.unit(id).unwrap().position;
            if at != previous[n] {
                last_move = tick;
            }
            previous[n] = at;
        }
        let crossed = |ids: &[EntityId], past: bool, x: i32| {
            ids.iter()
                .filter(|&&id| (world.unit(id).unwrap().position.tile_floor().x >= x) == past)
                .count()
        };
        if first_cross.is_none() && (crossed(&west, true, 35) > 0 || crossed(&east, false, 29) > 0)
        {
            first_cross = Some(tick);
        }
        if all_across.is_none() && crossed(&west, true, 35) == 3 && crossed(&east, false, 29) == 3 {
            all_across = Some(tick);
        }
    }
    let at = |id: EntityId| world.unit(id).unwrap().position.tile_floor();
    let west_across = west.iter().filter(|&&id| at(id).x >= 35).count();
    let east_across = east.iter().filter(|&&id| at(id).x <= 28).count();
    let on_deck = all
        .iter()
        .filter(|&&id| (30..34).contains(&at(id).x))
        .count();
    let moving = all
        .iter()
        .filter(|&&id| world.unit(id).unwrap().moving)
        .count();
    eprintln!(
        "group-baseline bridge-head-on: first_cross={first_cross:?} all_across={all_across:?} last_position_change={last_move}"
    );
    // The design suspected a deadlock; there is none. Three units each side
    // PR 7 graded squeezing preserves the rebased bridge timing: 181/251/380.
    // PR 8 keeps this passing and makes the pass faster than 3 s (60 ticks)
    // from the first meeting.
    assert_eq!((west_across, east_across, on_deck, moving), (3, 3, 0, 0));
    assert_eq!(
        (first_cross, all_across, last_move),
        (Some(181), Some(251), 380)
    );
}
