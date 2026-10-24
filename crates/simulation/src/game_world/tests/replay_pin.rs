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

/// Current pins include route guidance, certified endpoints and unreachable
/// memos, resumable reverse-flood frontiers, player-first scheduling, bounded
/// idle rotation and active-follower sampling slots. Exact explored visibility
/// adds per-player fog state. These intentional authoritative-state and timing
/// changes regenerate the combined tree's checkpoints; both independent runs
/// must still agree on every tick and match every checkpoint.
#[test]
fn wildlife_and_fog_replay_hashes_stay_pinned() {
    const PINS: [(&str, &str); 4] = [
        (
            "100",
            "cf81ff8cb84ba1ae148304945637ddb6391a37815ceb47b5a127543fbaf987b1",
        ),
        (
            "200",
            "7938800d7a9d15131de7ea5130677720fb8a726ee9f1b18e112c31ec2092077a",
        ),
        (
            "300",
            "3e20809bb3e3d4d10797479da2342fa6ddfdb6da9cc65c33a2c08083566c91ed",
        ),
        (
            "400",
            "24a4502c6526191ce82a65184db41440da11ea83a234ff41c7289af801b80cfb",
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
