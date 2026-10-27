// Rebased onto #573: its avoidance, arrival slots, settling and contact drop-off
// now combine with eco2 reservations (reservation-aware refinement, staged
// transport boarding). Regenerated the observed current hashes with the same
// scenarios via `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation`.
// Regenerated for bounded obstruction state, arrival hysteresis and unchanged-wait suppression.
// Combined bundle: Vassal economy state, water geometry and eco2 job rules
// regenerate current replay pins. Retain all scenarios and thread comparisons.
// The Vassal manager adds its fair cursor, active index and controlled jobs
// to canonical state. Regenerate current pins for this intentional schema
// change; independent runs, thread equality and emitted-change pins stay intact.
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
// Vassal home bindings add an ordered map (including its empty length) to
// canonical economy state. Regenerate the current pins; keep the scenario and
// every independent replay assertion unchanged.
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
            "c384bae1f10f4cf240fbaeb8d9018b2506bca302b5ce4f995ba637a2e354b460",
        ),
        (
            "200",
            "7db5ab596ddc481d052fe91d40ac3df85e95247db220ec4a647382d91169f5c1",
        ),
        (
            "300",
            "ae9f5b3a15095ae70f80e4edd8a7e773b239677db4e3122c08a2eb3cebc8b6db",
        ),
        (
            "400",
            "2b50998bfec973b3404f2fcb0bd41bb3557085ab435687437e696021b44abf0f",
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
