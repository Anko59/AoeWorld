//! Run-twice determinism for the wildlife and vision phases: a fixed scenario
//! whose two independent runs must agree on the canonical hash at every tick.
use super::*;

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

/// Fixed wildlife-and-fog scenario: two villages, scouts crossing wildlife
/// country, an unseen Town Center demolition.
fn replay_world() -> GameWorld {
    let mut w = world();
    w.initialize_village(PlayerId(0), TileCoord::new(64, 64))
        .unwrap();
    w.initialize_village(PlayerId(1), TileCoord::new(180, 180))
        .unwrap();
    let scouts = [(PlayerId(0), at(180, 180)), (PlayerId(1), at(64, 64))];
    for (player, destination) in scouts {
        let scout = w
            .units()
            .find(|u| u.player == player && u.kind == UnitKind::Scout)
            .unwrap()
            .id;
        w.issue_move(scout, destination).unwrap();
    }
    w
}

#[test]
fn wildlife_and_fog_replay_is_deterministic() {
    let (mut a, mut b) = (replay_world(), replay_world());
    for tick in 1..=400 {
        for world in [&mut a, &mut b] {
            if tick == 130 {
                for player in [PlayerId(0), PlayerId(1)] {
                    let scout = world
                        .units()
                        .find(|u| u.player == player && u.kind == UnitKind::Scout)
                        .unwrap()
                        .id;
                    let destination = if player == PlayerId(0) {
                        at(90, 90)
                    } else {
                        at(150, 150)
                    };
                    world.issue_move(scout, destination).unwrap();
                }
            }
            if tick == 200 {
                let town_center = world
                    .buildings()
                    .find(|b| b.player == PlayerId(1) && b.kind == BuildingKind::TownCenter)
                    .unwrap()
                    .id;
                world.remove_building(town_center);
            }
            world.advance();
        }
        assert_eq!(a.canonical_hash(), b.canonical_hash(), "tick {tick}");
    }
}
