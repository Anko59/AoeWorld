//! Replay pins for the wildlife and vision phases: a fixed scenario whose
//! canonical hashes include the current authoritative state schema.
//! Rally points add an explicit optional field to live and remembered buildings;
//! the pins are regenerated for that deliberate state-hash change.
//! Prepaid farm reseeds add a byte to live and remembered building hashes;
//! regenerate the current checkpoints for that deliberate schema change.
//! Cavalry auto-scout flags, its fair cursor and retained frontier searches
//! extend the authoritative schema again; regenerate the current pins.
//! The merged retained-detour planner and open TC arches change routes and
//! planner hashes too; regenerate this combined schema with the same scenario.
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
/// must still agree on every tick and match every checkpoint. Static navigation
/// adds the hashed shared preparation allowance and budgeted reverse-proof state;
/// regenerate for that deliberate schema/pacing change without changing the run.
/// TC collision now uses its NW 2x2 annex, and the obsolete unproven-approach
/// flag is removed from planner state. Near detour guidance and retained
/// searches deliberately change timing; regenerate these current checkpoints.
#[test]
fn wildlife_and_fog_replay_hashes_stay_pinned() {
    const PINS: [(&str, &str); 4] = [
        (
            "100",
            "5645a0e52b025a900eae98199f7310685f354119c7661932dda9e53371efc77d",
        ),
        (
            "200",
            "9e4280f11814fcba85379a90807f2c1675130322d555eb2fe1320a116b124179",
        ),
        (
            "300",
            "db5c72fb9b1c17f1dd227b5cf5f4f167fa5dfe2aa10cf74b29f12296b47387e0",
        ),
        (
            "400",
            "949a22460fbe9ae269298fc3e9bc9e69b84ce9e27e782ebc5cfb3961c1be3c0c",
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
