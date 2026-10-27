//! Reproducible phase timings, informational on this shared machine.
use super::*;
use crate::TickPhase;
use std::time::Instant;

#[test]
fn combat_far_orders_benchmark_500_versus_500() {
    let mut w = world();
    w.set_fog_disabled(true);
    let mut ours = Vec::new();
    let mut enemies = Vec::new();
    for n in 0..500 {
        ours.push(
            w.spawn_kind(
                PlayerId(0),
                crate::UnitKind::Archer,
                at(10 + n % 20 * 4, 10 + n / 20 * 4),
            )
            .unwrap(),
        );
        enemies.push(
            w.spawn_kind(
                PlayerId(1),
                crate::UnitKind::Archer,
                at(150 + n % 20 * 4, 150 + n / 20 * 4),
            )
            .unwrap(),
        );
    }
    let mut orders = Vec::new();
    let mut combat = Vec::new();
    let mut ticks = Vec::new();
    for tick in 0..32 {
        let start = Instant::now();
        for n in 0..32 {
            let index = (tick * 32 + n) % 500;
            w.issue_attack(PlayerId(0), ours[index], enemies[(index + tick) % 500])
                .unwrap();
        }
        orders.push(start.elapsed().as_micros());
        let start = Instant::now();
        let mut phase = start;
        w.advance_with_probe(&mut |_| true, &mut |p| {
            let now = Instant::now();
            if p == TickPhase::Combat {
                combat.push(now.duration_since(phase).as_micros());
            }
            phase = now;
        });
        ticks.push(start.elapsed().as_micros());
    }
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reports/perf");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("combat-far-orders.txt"),
        format!(
            "units=1000 ticks=32 orders_per_tick=32\norders_us={orders:?}\ncombat_us={combat:?}\ntick_us={ticks:?}\n"
        ),
    )
    .unwrap();
    assert!(orders.iter().all(|&us| us > 0));
}

#[test]
fn combat_benchmark_500_versus_500() {
    for gap in [1, 12] {
        let mut w = world();
        w.set_fog_disabled(true);
        let mut pairs = Vec::new();
        for n in 0..500 {
            let x = 10 + (n % 10) * 24;
            let y = 10 + (n / 10) * 4;
            let a = w
                .spawn_kind(PlayerId(0), crate::UnitKind::Archer, at(x, y))
                .unwrap();
            let b = w
                .spawn_kind(PlayerId(1), crate::UnitKind::Archer, at(x + gap, y))
                .unwrap();
            pairs.push((a, b));
        }
        let start = Instant::now();
        for &(a, b) in &pairs {
            w.issue_attack(PlayerId(0), a, b).unwrap();
            w.issue_attack(PlayerId(1), b, a).unwrap();
        }
        let setup = start.elapsed();
        let (mut ticks, mut combat, mut orders) = (Vec::new(), Vec::new(), Vec::new());
        let mut hits = 0_u64;
        for tick in 0..400 {
            let start = Instant::now();
            for n in 0..32 {
                let (a, b) = pairs[(tick * 32 + n) % pairs.len()];
                if w.unit(a).unwrap().hp > 0 && w.unit(b).unwrap().hp > 0 {
                    let _ = w.issue_attack(PlayerId(0), a, b);
                }
            }
            orders.push(start.elapsed().as_micros());
            let start = Instant::now();
            let mut phase = start;
            w.advance_with_probe(&mut |_| true, &mut |p| {
                let now = Instant::now();
                if p == TickPhase::Combat {
                    combat.push(now.duration_since(phase).as_micros());
                }
                phase = now;
            });
            ticks.push(start.elapsed().as_micros());
            hits += u64::from(w.combat_stats().hits);
        }
        let engaged_pairs = pairs
            .iter()
            .filter(|&&(a, b)| {
                w.unit(a).unwrap().last_hit.is_some() && w.unit(b).unwrap().last_hit.is_some()
            })
            .count();
        let quantiles = |mut samples: Vec<u128>| {
            samples.sort_unstable();
            [
                samples[samples.len() / 2],
                samples[(samples.len() * 95).div_ceil(100) - 1],
                *samples.last().unwrap(),
            ]
        };
        println!(
            "combat-benchmark units=1000 ticks=400 gap={gap} setup_us={} tick_us(p50,p95,max)={:?} combat_us={:?} orders_us={:?}",
            setup.as_micros(),
            quantiles(ticks.clone()),
            quantiles(combat.clone()),
            quantiles(orders.clone())
        );
        // Nextest suppresses successful stdout; keep the raw samples as well.
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reports/perf");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(format!("combat-500-gap-{gap}.txt")),
            format!("units=1000 ticks=400 gap={gap} setup_us={} engaged_pairs={engaged_pairs} hits={hits}\ntick_us={ticks:?}\ncombat_us={combat:?}\norders_us={orders:?}\n", setup.as_micros())).unwrap();
        if gap == 1 {
            assert_eq!(
                engaged_pairs, 500,
                "the stationary workload must engage both armies"
            );
        }
        assert!(
            pairs
                .iter()
                .any(|&(a, b)| w.unit(a).unwrap().last_hit.is_some()
                    && w.unit(b).unwrap().last_hit.is_some()),
            "two-way fighting must be measured"
        );
    }
}
