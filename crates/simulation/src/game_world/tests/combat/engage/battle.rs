use super::*;
use aoe_core::Formation;

fn run(name: &str, defenders_march: bool) -> (Probe, Probe) {
    let mut w = world();
    w.set_fog_disabled(true);
    let a = army(&mut w, 0, 300, 25, 60, 100);
    let b = army(&mut w, 1, 300, 25, 120, 100);
    w.issue_formation_move(PlayerId(0), &a, Formation::Line, at(125, 106), false, true)
        .unwrap();
    if defenders_march {
        w.issue_formation_move(PlayerId(1), &b, Formation::Line, at(65, 106), false, true)
            .unwrap();
    }
    let all: Vec<_> = a.iter().chain(&b).copied().collect();
    let mut probe = Probe::default();
    let (mut pa, mut pb) = (Probe::default(), Probe::default());
    let mut spent = std::time::Duration::ZERO;
    for _ in 0..1_200 {
        let start = std::time::Instant::now();
        w.advance();
        spent += start.elapsed();
        probe.sample(&w, &all);
        pa.sample(&w, &a);
        pb.sample(&w, &b);
    }
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.cache/engage")
            .join(format!("{name}-time.txt")),
        format!("advance total ms={}", spent.as_millis()),
    )
    .unwrap();
    let mut kinds = std::collections::BTreeMap::<String, usize>::new();
    for &id in &all {
        if let Some(u) = w.unit(id).filter(|u| u.hp > 0 && u.attack_target.is_none()) {
            let near = w.units().any(|e| {
                e.player != u.player
                    && e.hp > 0
                    && (i64::from(e.position.x - u.position.x).pow(2)
                        + i64::from(e.position.y - u.position.y).pow(2))
                        < 5 * 1024 * 1024 * 5
            });
            if near {
                *kinds
                    .entry(format!("{:?}/{:?}/moving={}", u.kind, u.task, u.moving))
                    .or_default() += 1;
            }
        }
    }
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.cache/engage")
            .join(format!("{name}-final.txt")),
        format!("{kinds:?}"),
    )
    .unwrap();
    println!("{}", probe.report(name, 20, all.len()));
    pa.report(&format!("{name}-side-a"), 20, a.len());
    pb.report(&format!("{name}-side-b"), 20, b.len());
    (pa, pb)
}

/// Both armies attack-move into each other: every unit with an enemy in reach
/// fights within seconds, not only the few a group's look can release.
#[test]
fn marching_armies_engage_nearly_every_unit_within_reach() {
    let (a, b) = run("march_vs_march", true);
    for (side, p) in [("a", a), ("b", b)] {
        let share = p.with_target * 100 / p.in_reach;
        assert!(share >= 90, "side {side}: {share}% of units in reach fight");
        assert!(
            p.stuck_units(6, 20) <= 3,
            "side {side}: {} units idle in reach for 6s",
            p.stuck_units(6, 20)
        );
    }
}

#[test]
fn a_marching_army_engages_idle_defenders() {
    let (a, b) = run("march_vs_idle", false);
    for (side, p) in [("a", a), ("b", b)] {
        let share = p.with_target * 100 / p.in_reach;
        assert!(share >= 80, "side {side}: {share}% of units in reach fight");
        assert!(p.stuck_units(6, 20) <= 3, "side {side}");
    }
}
