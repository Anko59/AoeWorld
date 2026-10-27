// Rebased onto #573: its avoidance, arrival slots, settling and contact drop-off
// now combine with eco2 reservations (reservation-aware refinement, staged
// transport boarding). Regenerated the observed current hashes with the same
// scenarios via `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation`.
// Regenerated for bounded obstruction state, arrival hysteresis and unchanged-wait suppression.
// Combined bundle: eco2 reservations, farm production and DAT rates combine
// with Vassal state, water and static navigation. Regenerated from the focused
// replay output; the scenario and independent replay assertions are unchanged.
// The Vassal manager adds its fair cursor, active index and controlled jobs
// to canonical state. Regenerate current pins for this intentional schema
// change; independent runs, thread equality and emitted-change pins stay intact.
// Vassal home bindings add an ordered map to canonical economy state,
// including the empty map length. Regenerate current hashes for this deliberate
// schema change; preserve the scenarios, emitted changes and replay comparisons.
// Combined Castle bundle: u64 research, healer/garrison indexes, queued builders
// and cargo/embarked maps extend current authoritative state. Re-pin the same
// workload; retain checkpoints, change digests and all independent replay checks.
// Castle Age widens researched bits to u64 and hashes the ordered healer index;
// Gather credits now use millionths for exact stacked mining upgrades.
// Regenerate current checkpoints for these deliberate state-schema changes.
// Gather jobs now hash their bounded failed-approach counter. Replacement also
// uses only locally certified work tiles, so unreachable approaches retarget.
// Regenerate current pins; keep this workload and every replay assertion.
use super::*;

/// Replay pin: villagers gather every resource through several drop-off
/// building kinds while the scout walks, camps rise beside their resources
/// and a wood node is depleted into a replacement, so any index change that
/// alters authoritative state breaks the current pinned hashes.
#[test]
fn gathering_and_dropoffs_replay_to_pinned_hashes() {
    let (package, pages, water) = fixture::flat_fixture(false);
    let generator = package
        .generator_with_environment(pages.clone(), water.clone(), Vec::new(), Vec::new())
        .unwrap();
    let mut world =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    // A search before the village exists warms any per-chunk resource cache;
    // the starting nodes inserted by the village must still be found after.
    let _ = world.nearest_resource(TileCoord::new(64, 64), ResourceKind::Wood, 10, None);
    (0..40i32)
        .find_map(|step| {
            let spot = TileCoord::new(64 + (step % 7) * 6 - 18, 64 + (step / 7) * 6);
            world
                .initialize_village(PlayerId(0), spot)
                .ok()
                .map(|_| spot)
        })
        .expect("a village start site");
    buildings::set_stock(&mut world, 0, 600, 0, 0);

    // One gatherer per resource, each spawned next to its node.
    let mut jobs = Vec::new();
    for kind in [
        ResourceKind::Wood,
        ResourceKind::Food,
        ResourceKind::Gold,
        ResourceKind::Stone,
    ] {
        let node = accessible_node(&generator, &world, kind);
        let id = gatherer_at(&mut world, node);
        world.issue_gather(PlayerId(0), id, node.id).unwrap();
        jobs.push((kind, node, id));
    }

    // A completed drop-off of each camp kind rises beside its resource, so
    // returns are attempted toward several building kinds. Candidate spots
    // skip node tiles, and the builder spawns on a checked neighbour tile.
    for (kind, base) in [
        (BuildingKind::LumberCamp, jobs[0].1.tile),
        (BuildingKind::MiningCamp, jobs[2].1.tile),
        (BuildingKind::Mill, jobs[1].1.tile),
    ] {
        let side = kind.rules().side;
        let ring = (-1..=side)
            .flat_map(|dy| (-1..=side).map(move |dx| (dx, dy)))
            .filter(|(dx, dy)| *dx == -1 || *dx == side || *dy == -1 || *dy == side);
        let placed = (0..25i32)
            .find_map(|attempt| {
                let spot = TileCoord::new(
                    base.x + 3 + (attempt % 5) * 2,
                    base.y + 3 + (attempt / 5) * 2,
                );
                let clear = (0..side).all(|dy| {
                    (0..side).all(|dx| {
                        world
                            .terrain()
                            .passable(TileCoord::new(spot.x + dx, spot.y + dy), world.config())
                    })
                });
                if !clear {
                    return None;
                }
                let (sx, sy) = ring.clone().find(|(dx, dy)| {
                    world
                        .terrain()
                        .passable(TileCoord::new(spot.x + dx, spot.y + dy), world.config())
                })?;
                let worker = world
                    .spawn_villager(
                        PlayerId(0),
                        WorldPosition::from_tile_center(TileCoord::new(spot.x + sx, spot.y + sy))
                            .unwrap(),
                    )
                    .ok()?;
                world
                    .issue_build(PlayerId(0), &[worker], kind, spot, false)
                    .ok()
            })
            .expect("room for the camp");
        buildings::finish(&mut world, placed, 1_600);
    }

    // The scout walks for the whole run, so movement state changes too.
    let scout = world
        .units()
        .find(|unit| unit.kind == crate::UnitKind::Scout)
        .map(|unit| unit.id)
        .expect("village scout");
    (0..40i32)
        .find_map(|step| {
            Some(world.issue_move(
                scout,
                WorldPosition::from_tile_center(TileCoord::new(60 + step, 90)).ok()?,
            ))
            .and_then(|result| result.ok())
        })
        .expect("a scout destination");

    let mut pins = Vec::new();
    for block in 0..6 {
        if block == 1 {
            // Exhausting the wood node forces a replacement search mid-run.
            let node = jobs[0].1;
            world
                .deplete_resource(node.id, node.initial_amount)
                .unwrap();
        }
        for _ in 0..100 {
            world.advance();
        }
        pins.push(world.canonical_hash_hex());
    }
    assert_eq!(
        pins.as_slice(),
        GATHER_REPLAY_PINS,
        "tick {}",
        world.tick().0
    );
}

/// Current gathering/drop-off replay includes route guidance, certified
/// endpoints, resumable reverse-flood state and exact explored fog. Bounded
/// idle rotation and active follower sampling intentionally change hashed
/// state and timing. Regenerate the combined tree's six checkpoints while
/// preserving every assertion and the independent replay tests. Static
/// navigation adds a hashed shared preparation allowance and budgeted proof
/// state; these pins cover that intentional schema/pacing change as well.
/// Drop-off-ranked replacements and the retained last work tile intentionally
/// change gather jobs and their hashes; regenerate these current checkpoints.
/// The Mill's prepaid reseed count extends live and remembered building hashes.
/// Cavalry auto-scout flags and bounded frontier scheduling extend the current
/// authoritative schema, so these checkpoints are regenerated with that state.
/// The merged open TC arches and retained detour planner change routes and
/// planner hashes too; regenerate the same six checkpoints for their combination.
/// Transport cargo and embarked-unit maps extend canonical economy state,
/// including empty maps; regenerate these six pins without changing the run.
/// Returning workers now deposit on contact with any footprint perimeter while
/// their old movement order is still active. Cancelling that route and resuming
/// work earlier deliberately changes task/planner timing and stockpiles. These
/// six current checkpoints also include swept avoidance, nearest-free work
/// arrivals and bounded settling/retries, which change routes and hashed state.
/// Regenerate the combination with the same workload and replay assertions.
/// Regenerate with `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation
/// AOE_NATIVE_TEST_FILTER='test(gathering_and_dropoffs_replay_to_pinned_hashes)'`
/// and use the six observed canonical hashes without changing the workload.
const GATHER_REPLAY_PINS: [&str; 6] = [
    "79f159a2a0839799fd54ec86d5fa50807aaa6ce47df82f8997f6c09742906884",
    "4ed7ca843f1b2edbeaea463bc855c579feac9b3756f3241e0dfd25a6cf2597a3",
    "c1ee687f8821ad4fffb637aa92e47a01b330aac886b2b446022d3ef21bbef4fd",
    "d7e40fb7aacfd7ba0eaf947bf98144966c80535d807032c683dadd7b5bff2af8",
    "d870a57c719f2989b73fc3bc2893ee79249d0c922dc36db265a7c53ef6b6497c",
    "1c628751dd5779b45266f1dcd792660c7b7d08574defede9bad64ed30cc53bd3",
];
