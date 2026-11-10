//! Reproducible river planning with late-game building counts.
use super::*;
use crate::game_world::economy::bridge::work;
use std::time::Instant;

#[test]
fn vassal_bridge_planning_with_ten_thousand_buildings() {
    let mut w = super::super::bridges::river_fixture::river_sized(true, 30..31, 100, None, 15_360);
    w.set_fog_disabled(true);
    w.place_town_center(PlayerId(0), TileCoord::new(140, 92))
        .unwrap();
    w.economy.players.get_mut(&PlayerId(0)).unwrap().age = Age::Castle;
    let castle = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            TileRect::from_xywh(100, 90, 4, 4),
            true,
        )
        .unwrap();
    let v = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::Vassal,
            WorldPosition::from_tile_center(TileCoord::new(99, 92)).unwrap(),
        )
        .unwrap();
    w.issue_garrison(PlayerId(0), v, castle).unwrap();
    w.prepare_navigation(&|| false).unwrap();
    advance(&mut w, 20);
    while w.vassal_planning_pending(v) {
        w.tick.0 += 1;
        w.advance_vassals(&mut Vec::new());
    }
    // One-tile foundations exercise occupancy without adding observers or
    // changing the castle's work points. Keep them away from the domain.
    let width = w.config.width_tiles;
    let height = w.config.height_tiles;
    assert!(width >= 250 && height >= 250, "{width}x{height}");
    for y in 150..250 {
        for x in 0..100 {
            w.insert_building(
                PlayerId(1),
                BuildingKind::Road,
                TileRect::from_xywh(x, y, 1, 1),
                false,
            )
            .unwrap();
        }
    }
    let mut worst = 0.0_f64;
    let mut total = 0.0;
    let mut worst_creation = 0.0_f64;
    let mut worst_poll = 0.0_f64;
    let mut ticks = 0_u32;
    work::take();
    for _ in 0..3 {
        w.tick.0 = ((w.tick.0 + 1_219) / 20) * 20;
        let mut finished = false;
        for _ in 0..2_000 {
            let continuing = w.vassal_planning_pending(v);
            let start = Instant::now();
            w.advance_vassals(&mut Vec::new());
            let ms = start.elapsed().as_secs_f64() * 1_000.0;
            worst = worst.max(ms);
            if continuing {
                worst_poll = worst_poll.max(ms);
            } else {
                worst_creation = worst_creation.max(ms);
            }
            total += ms;
            ticks += 1;
            if !w.vassal_planning_pending(v) {
                finished = true;
                break;
            }
            w.tick.0 += 1;
        }
        assert!(finished, "a budgeted plan must not starve");
    }
    let counts = work::take();
    let net = w.vassal_network(v);
    eprintln!(
        "terrain_reads={} max_tile_reads={}",
        counts.terrain_reads, counts.max_tile_reads
    );
    assert!(net.iter().any(|s| s.0 == BuildingKind::Bridge));
    assert_eq!(
        format!("{net:?}"),
        include_str!("network-plan.txt").trim(),
        "budgeting must preserve the static-world road and bridge decisions"
    );
    let report = format!(
        "buildings={} plans=3 ticks={ticks} mean_vassal_ms={:.6} validation_calls={} occupancy_visits={} validation_ms={:.3} total_vassal_ms={total:.3} worst_vassal_ms={worst:.3} worst_creation_ms={worst_creation:.3} worst_poll_ms={worst_poll:.3} operations={} max_poll_operations={} terrain_reads={} max_tile_reads={} hash={} network={net:?}\n",
        w.buildings().count(),
        total / f64::from(ticks),
        counts.calls,
        counts.visits,
        counts.elapsed.as_secs_f64() * 1_000.0,
        counts.operations,
        counts.max_poll_operations,
        counts.terrain_reads,
        counts.max_tile_reads,
        w.canonical_hash_hex()
    );
    eprintln!("{report}");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/cx-bridge");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("measurement.txt"), report).unwrap();
    assert!(
        counts.calls > 0,
        "the fixture must exercise bridge validation"
    );
    assert_eq!(
        w.canonical_hash_hex(),
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/game_world/tests/economy/vassal/budgeted-checkpoint.txt")
        )
        .unwrap()
        .trim(),
        "Vassal explicit replan deadlines are included in canonical state"
    );
    assert!(
        counts.visits <= counts.calls * 15,
        "span occupancy must be bounded by its tiles: {counts:?}"
    );
    assert!(
        counts.max_tile_reads <= 1,
        "read each terrain tile at most once per immutable poll: {counts:?}"
    );
    assert!(
        counts.max_poll_operations <= 128,
        "every planning poll must yield: {counts:?}"
    );
}
