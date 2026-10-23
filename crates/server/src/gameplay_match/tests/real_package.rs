//! Village placement on the real 16,000-tile France package (local assets).
//! Skipped when the package is not present, as in CI.
use super::super::seats::place_villages_observed;
use super::super::*;
use crate::map_store::PageResidency;
use std::path::PathBuf;

const FRANCE: &str = "cf55200686bf3117e7ac33d48c3b8ae0354c8709cb33d54ba070def4fed12910";

fn france() -> Option<(MapPackage, Arc<PageResidency>)> {
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../local-assets/map-packages");
    let packages = map_store::load(Some(&directory)).ok()?;
    let package = packages.get(FRANCE)?.clone();
    let provider = PageResidency::open(&directory, &package, &|| false).ok()?;
    Some((package, provider))
}

/// 100 seats: every village is placed with its full kit, within the budget.
/// Run with `--ignored` where the local France package exists; the timing
/// bound assumes an otherwise quiet machine.
#[test]
#[ignore = "needs the local France package and a quiet machine"]
fn france_hundred_seats_place_within_twenty_seconds() {
    let Some((package, provider)) = france() else {
        eprintln!("France package not present: skipped");
        return;
    };
    let Ok(mut world) = GameWorld::from_page_provider(package, provider) else {
        panic!("the France package builds a world");
    };
    world.set_limits(MatchRules::default().limits(100));
    let started = std::time::Instant::now();
    let mut rows = Vec::new();
    let partial_before = world.partial_starting_kits();
    let placed = place_villages_observed(
        &mut world,
        100,
        None,
        &AtomicUsize::new(0),
        &|| false,
        &mut |index, ideal, tile, seconds| {
            eprintln!("village {index} ideal {ideal:?} -> {tile:?} {seconds:.2}s");
            rows.push(seconds);
        },
    );
    let Ok(scouts) = placed else {
        panic!("every seat has a start");
    };
    let total = started.elapsed().as_secs_f64();
    eprintln!("total placement {total:.1}s");
    assert_eq!(scouts.len(), 100);
    assert_eq!(world.partial_starting_kits(), partial_before);
    assert!(total < 20.0, "placing 100 villages took {total:.1}s");
    // A seat anchored at sea pays for the coast it reaches, not the sea.
    let slowest = rows.iter().copied().fold(0.0, f64::max);
    assert!(slowest < 3.0, "slowest village took {slowest:.1}s");
}

/// Two matches with different seeds: the human's village differs, and every
/// village of both stands on the one main land mass (no island starts).
#[test]
#[ignore = "needs the local France package; two 100-seat placements"]
fn france_starts_vary_by_seed_and_share_one_land_mass() {
    let Some((package, provider)) = france() else {
        eprintln!("France package not present: skipped");
        return;
    };
    let mut humans = Vec::new();
    for seed in [11_u64, 12] {
        let Ok(mut world) = GameWorld::from_page_provider(package.clone(), provider.clone()) else {
            panic!("the France package builds a world");
        };
        world.set_limits(MatchRules::default().limits(100));
        let Some(masses) = world.terrain().land_masses(world.config()) else {
            panic!("France has land");
        };
        eprintln!("France land masses: {}", masses.count());
        let mut tiles = Vec::new();
        let placed = place_villages_observed(
            &mut world,
            100,
            Some(seed),
            &AtomicUsize::new(0),
            &|| false,
            &mut |_, _, tile, _| tiles.push(tile),
        );
        let Ok(scouts) = placed else {
            panic!("every seat has a start");
        };
        assert_eq!(scouts.len(), 100);
        for (slot, tile) in tiles.iter().enumerate() {
            assert!(
                masses.is_main(*tile, aoe_map::CHUNK_TILES),
                "seed {seed} slot {slot} starts at {tile:?}, off the main continent"
            );
        }
        for unit in world
            .units()
            .filter(|unit| unit.kind == aoe_simulation::UnitKind::Scout)
        {
            let at = unit.position.tile_floor();
            assert!(
                masses.is_main(at, aoe_map::CHUNK_TILES),
                "seed {seed} scout at {at:?} is off the main continent"
            );
        }
        humans.push(tiles[0]);
    }
    assert_ne!(
        humans[0], humans[1],
        "the human started in the same place twice"
    );
}
