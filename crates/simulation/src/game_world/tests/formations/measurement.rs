//! Informational shared-host timing, using the same 200 militia and map for
//! both paths. No wall-clock value enters a replay or a test threshold.
use super::*;

#[test]
fn formations_measure_warm_army_movement_and_route_searches() {
    for grouped in [false, true] {
        let mut w = wide_world();
        w.set_fog_disabled(true);
        let ids = army(&mut w, 200);
        if grouped {
            w.issue_formation_move(
                PlayerId(0),
                &ids,
                aoe_core::Formation::Line,
                center(180, 60),
                false,
                false,
            )
            .unwrap();
        } else {
            for &id in &ids {
                w.issue_move(id, center(180, 60)).unwrap();
            }
        }
        // Exclude order issuance and initial assembly. Both armies are still
        // travelling during the identical tick 801..1000 measurement window.
        for _ in 0..800 {
            w.advance();
        }
        let from = w.formation_anchor(ids[0]);
        let mut samples = Vec::with_capacity(200);
        for _ in 0..200 {
            let start = std::time::Instant::now();
            w.advance_with_probe(&mut |_| true, &mut |phase| {
                if phase == TickPhase::Movement {
                    samples.push(start.elapsed().as_nanos());
                }
            });
        }
        samples.sort_unstable();
        let total = samples.iter().sum::<u128>();
        let metrics = w.route_scheduler_stats().metrics;
        eprintln!(
            "army=200 grouped={grouped} warmup_ticks=800 sample_ticks=200 route_orders={} searches_started={} movement_total_us={} movement_mean_ns={} movement_p50_ns={} movement_p95_ns={}",
            metrics.orders,
            metrics.searches_started,
            total / 1000,
            total / 200,
            samples[100],
            samples[190]
        );
        assert!(ids.iter().any(|&id| w.unit(id).unwrap().moving));
        // This individual workload starts 205 searches for 200 requests:
        // scheduler restarts are counted as well as the first attempts.
        // A shared route must perform exactly one search, including retries.
        assert_eq!(metrics.orders, if grouped { 1 } else { 200 });
        assert_eq!(metrics.searches_started, if grouped { 1 } else { 205 });
        if let Some(from) = from {
            let to = w.formation_anchor(ids[0]).unwrap();
            assert!(
                to.x > from.x + 1024,
                "the measurement must include actual centroid travel: from={from:?} to={to:?}"
            );
        }
    }
}

/// Count executed operations, not a retired steering allowance. Open blocks
/// need a handful of member passes, local path windows of at most five legs,
/// and bounded cell/body probes. These linear ceilings leave room for normal
/// fallback but reject whole-route/per-member rescans and quadratic work.
/// The flag reports a shared-path rebuild: only a changed route (a fetched
/// continuation or its straightened corridor) may write the path, so callers
/// bound the number of rebuilding ticks.
pub(super) fn bounded_group_work(
    before: &crate::game_world::route_scheduler::RouteMetrics,
    after: &crate::game_world::route_scheduler::RouteMetrics,
    members: usize,
) -> ((u64, u64, u64), bool) {
    assert!(
        before.group_path_writes > 0,
        "the issued route must be measured"
    );
    let rebuilt = after.group_path_writes != before.group_path_writes;
    let work = (
        after.group_member_work - before.group_member_work,
        after.group_path_work - before.group_path_work,
        after.group_collision_work - before.group_collision_work,
    );
    let n = members as u64;
    assert!((n..=12 * n).contains(&work.0), "member visits: {work:?}");
    assert!((n..=24 * n).contains(&work.1), "path work: {work:?}");
    assert!(
        (n..=128 * n).contains(&work.2),
        "collision probes: {work:?}"
    );
    (work, rebuilt)
}
