//! Replay pins for the wildlife and vision phases: a fixed scenario whose
//! canonical hashes include the current authoritative state schema.
//! Rally points add an explicit optional field to live and remembered buildings;
//! the pins are regenerated for that deliberate state-hash change.
use super::*;

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

/// Fixed wildlife-and-fog scenario: two villages, scouts crossing wildlife
/// country, an unseen Town Center demolition. Two independent runs must agree
/// with each other and with the pinned checkpoints.
fn pinned_replay_world() -> GameWorld {
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

/// Re-pinned for the route-throughput fix: every order now gets a fast
/// attempt from its own per-tick budget, so a scout's route starts on the
/// order's tick, and the fast budget and per-player spend, each unit's
/// fast-attempt flag and each planner's sealed-destination flag are hashed. Rally points also add optional state to
/// buildings, which is hashed. Review re-pin adds resumable reverse-flood
/// frontier/cursor state and player-first budget allocation. Only checkpoints
/// holding changed planner state move; the paired replay assertion stays. Two runs still agree.
#[test]
fn wildlife_and_fog_replay_hashes_stay_pinned() {
    const PINS: [(&str, &str); 4] = [
        (
            "100",
            "b8ed5e0e37a4cdf45982d7fec13332a3afb19136cc073b4fa036f5f3b7956ae6",
        ),
        (
            "200",
            "2440d186157253ebf1042fbcb626dcdd3a84ad6663757878503428b8868095dd",
        ),
        (
            "300",
            "2b310225bc9d6f0135ce9f4e506195afcf651f25ee515797bf8a9bcfd93ddf04",
        ),
        (
            "400",
            "8c3cb0cc43edfc2d47a420561811878dcef7fcaf4481e033e06fac6dd302092a",
        ),
    ];
    let mut checkpoints = Vec::new();
    let (mut a, mut b) = (pinned_replay_world(), pinned_replay_world());
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
        if tick % 100 == 0 {
            let pin = PINS[tick / 100 - 1];
            assert_eq!(pin.0, tick.to_string(), "checkpoint order");
            checkpoints.push(a.canonical_hash_hex());
        }
    }
    assert_eq!(
        checkpoints,
        PINS.map(|(_, hash)| hash),
        "replay checkpoints"
    );
}
