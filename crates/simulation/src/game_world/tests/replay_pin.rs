// Regenerated for bounded obstruction state, arrival hysteresis and unchanged-wait suppression.
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
// Castle Age widens researched bits to u64 and hashes the ordered healer index;
// Gather credits now use millionths for exact stacked mining upgrades.
// Regenerate current checkpoints for these deliberate state-schema changes.
// Combined Castle bundle: u64 research, healers, garrisons, queued builders and
// cargo/embarked maps extend current state. Regenerate the same scenario; all
// independent replay, thread equality and emitted-change assertions remain.
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
/// Unit obstruction and local refinement now route around stationary workers
/// and preserve reachable arrival slots. Regenerate for that intentional route
/// and timing change; both independent runs still agree at every tick.
#[test]
fn wildlife_and_fog_replay_hashes_stay_pinned() {
    const PINS: [(&str, &str); 4] = [
        (
            "100",
            "06938adf18fe315aeefbbbcb7a0f7853f095913448613beac31d1e78b0e4d649",
        ),
        (
            "200",
            "d301e1fed80c38c67fe70f8bcca0ca11d28a92a1f57006e74ab2aa7a97110c3a",
        ),
        (
            "300",
            "0bf354e3b9791bd16b22bb923ae208a5697cbacbc9dfff7874c10037324b5937",
        ),
        (
            "400",
            "f2dc34995fbeb368d338babc9f79ec8d81933ec86d7e91968286854c96ff5995",
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
