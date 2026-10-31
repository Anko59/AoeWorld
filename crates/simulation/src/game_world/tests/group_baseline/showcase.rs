//! The filmed sandy-strip order: 32 militia, nominally 25 tiles north.
use super::*;

#[derive(Debug, Default)]
struct Phases {
    first_motion: u32,
    formed: u32,
    anchor_arrival: u32,
    slow_ticks: u32,
    full_ticks: u32,
    first: u32,
    last: u32,
    catch_up: u64,
    pace_changes: Vec<(u32, bool)>,
}

#[test]
fn thirty_two_militia_showcase_keeps_block_and_normal_march_speed() {
    use aoe_map::{EnvironmentalProvenance, MapPackage, MapRequest, ProjectionMetadata, Ratio};
    let (original, pages, _) = super::super::economy::fixture::flat_fixture(false);
    let package = MapPackage::with_prepared_environment(
        1,
        MapRequest {
            requested_side_meters: 256,
            compression: Ratio::new(1, 1).unwrap(),
            seed: 1,
            ..MapRequest::default()
        },
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        original.environment,
    )
    .unwrap();
    let mut w = GameWorld::from_prepared_map(package, pages, vec![], vec![], vec![]).unwrap();
    w.prepare_navigation(&|| false).unwrap();
    for _ in 0..40 {
        w.allocate_entity_id().unwrap();
    }
    w.set_fog_disabled(true);
    // Positions after the original staging script's snapped individual orders.
    // One member sidestepped a neighbour; preserve that real starting geometry.
    let ids = (0..32)
        .map(|k| {
            let x = [103, 104, 105, 107, 108, 109][k % 6];
            let y = [26, 27, 28, 30, 31, 32][k / 6];
            let at = if k == 25 {
                WorldPosition::new(107315, 33579)
            } else {
                center(x, y)
            };
            w.spawn_kind(PlayerId(0), UnitKind::Militia, at).unwrap()
        })
        .collect::<Vec<_>>();
    let to = center(106, 4);
    let (speed, denom) = w.movement_speed(UnitKind::Militia, PlayerId(0));
    let ideal = (25 * 1024 * denom).div_ceil(speed);
    w.tick = Tick(1810);
    w.issue_formation_move(PlayerId(0), &ids, Formation::Line, to, false, false)
        .unwrap();
    let mut out = Phases::default();
    let mut previous = w.formation_anchor(ids[0]).unwrap();
    let mut stopped = vec![None; ids.len()];
    for tick in 1..=1400 {
        w.advance();
        let anchor = w.formation_anchor(ids[0]).unwrap();
        let (progress, forming, own, pace) = w.group_march_probe(ids[0]);
        if anchor != previous {
            if out.first_motion == 0 {
                out.first_motion = tick;
            }
            let slow = pace.0 * own.1 < own.0 * pace.1;
            if out
                .pace_changes
                .last()
                .is_none_or(|&(_, was_slow)| slow != was_slow)
            {
                out.pace_changes.push((tick, slow));
            }
            if slow {
                out.slow_ticks += 1;
            } else {
                out.full_ticks += 1;
            }
            out.catch_up = crate::game_world::formation::matched_speed(pace, own);
        }
        if out.formed == 0 && progress >= forming {
            out.formed = tick;
        }
        if out.anchor_arrival == 0 && anchor == to {
            out.anchor_arrival = tick;
        }
        previous = anchor;
        for (n, &id) in ids.iter().enumerate() {
            let u = w.unit(id).unwrap();
            if !u.moving && stopped[n].is_none() {
                stopped[n] = Some(tick);
                if out.first == 0 {
                    out.first = tick;
                }
            }
        }
        if stopped.iter().all(Option::is_some) {
            out.last = tick;
            break;
        }
    }
    eprintln!("showcase militia speed={speed}/{denom} subunits/tick ideal={ideal} {out:?}");
    assert_eq!(ideal, 556);
    for &id in &ids {
        // A timeout at a blocked body is not arrival.
        assert_eq!(
            w.unit(id).unwrap().position,
            w.formation_destination_slot(id).unwrap()
        );
    }
    assert_eq!(out.first_motion, 1, "no stationary assembly: {out:?}");
    assert!(out.last > 0 && out.last <= 634, "{out:?}");
    assert!(out.last - out.first <= 100, "{out:?}");
    assert_eq!(
        (
            out.formed,
            out.anchor_arrival,
            out.slow_ticks,
            out.full_ticks
        ),
        (283, 586, 294, 292),
        "pin the measured moving phases: {out:?}"
    );
    let slots = ids
        .iter()
        .map(|&id| w.unit(id).unwrap().position)
        .collect::<Vec<_>>();
    let width = slots.iter().map(|p| p.x).max().unwrap() - slots.iter().map(|p| p.x).min().unwrap();
    let depth = slots.iter().map(|p| p.y).max().unwrap() - slots.iter().map(|p| p.y).min().unwrap();
    assert!(width > depth * 2, "the destination retains broad ranks");
    let passing_gap = 4 * UnitKind::Militia.collision_radius() + speed.div_ceil(denom) as i32 + 16;
    for (n, a) in slots.iter().enumerate() {
        for b in &slots[n + 1..] {
            let dx = i64::from(a.x - b.x);
            let dy = i64::from(a.y - b.y);
            assert!(
                dx * dx + dy * dy >= i64::from(passing_gap - 2).pow(2),
                "open destination leaves room for a late member to pass"
            );
        }
    }
    assert_eq!(
        (out.first, out.last),
        (586, 630),
        "pin current arrival timing: {out:?}"
    );
}
