//! Measured timeline of a Vassal domain with a given number of builders: ticks
//! to the first wall tiles, the first finished gate and the finished domain.
use super::*;

fn count(w: &GameWorld, f: impl Fn(&crate::GameBuilding) -> bool) -> usize {
    w.buildings()
        .filter(|b| b.player == PlayerId(0) && b.is_complete() && f(b))
        .count()
}

/// Run `builders` villagers for `limit` ticks, printing one line per event.
pub(super) fn timeline(builders: usize, limit: u32) -> (Option<u32>, Option<u32>, Option<u32>) {
    let (mut w, v, _) = setup();
    w.set_fog_disabled(true);
    w.economy.players.get_mut(&PlayerId(0)).unwrap().stockpile = Stockpile {
        food: 100_000,
        wood: 100_000,
        gold: 100_000,
        stone: 100_000,
    };
    for i in 0..builders {
        let at = TileCoord::new(86 + i as i32 % 10, 97 + i as i32 / 10);
        w.spawn_villager(PlayerId(0), WorldPosition::from_tile_center(at).unwrap())
            .unwrap();
    }
    let (mut first_wall, mut first_gate, mut done) = (None, None, None);
    let mut t = 0;
    while t < limit && done.is_none() {
        advance(&mut w, 100);
        t += 100;
        let walls = count(&w, |b| b.kind.is_wall());
        let gates = count(&w, |b| b.kind.is_gate());
        if walls >= 20 && first_wall.is_none() {
            first_wall = Some(t);
        }
        if gates >= 1 && first_gate.is_none() {
            first_gate = Some(t);
        }
        if false {
            let mut tally: std::collections::BTreeMap<String, usize> = Default::default();
            for u in w.units().filter(|u| u.kind == UnitKind::Villager) {
                *tally
                    .entry(format!("{:?}/{}", u.task, u.moving))
                    .or_default() += 1;
            }
            eprintln!("  t={t}: villagers {tally:?} walls {walls}");
            let open = w.buildings().filter(|b| !b.is_complete()).count();
            let lines: Vec<_> = w
                .units()
                .filter(|u| u.kind == UnitKind::Villager && w.vassal_controller(u.id) == Some(v))
                .map(|u| {
                    let job = w.economy.builders.get(&u.id).map(|j| j.building.0);
                    format!(
                        "{}:{:?}@{},{}{} j{:?}",
                        u.id.0,
                        u.task,
                        u.position.tile_floor().x,
                        u.position.tile_floor().y,
                        if u.moving { "m" } else { "" },
                        job
                    )
                })
                .collect();
            eprintln!("  open foundations {open}; {}", lines.join(" | "));
        }
        if t % 5_000 == 0 {
            let s = w.vassal_status(v).unwrap();
            eprintln!(
                "  t={t}: walls {walls} gates {gates} towers {} roads {} bridges {} sites {}/{} wait {:?}",
                count(&w, |b| b.kind == BuildingKind::WatchTower),
                count(&w, |b| b.kind == BuildingKind::Road),
                count(&w, |b| b.kind == BuildingKind::Bridge),
                s.completed,
                s.total,
                s.wait
            );
        }
        if w.vassal_status(v).unwrap().developed {
            done = Some(t);
        }
    }
    eprintln!(
        "timeline {builders} builders: 20 wall tiles {first_wall:?}, first gate {first_gate:?}, developed {done:?}"
    );
    (first_wall, first_gate, done)
}

#[test]
fn vassal_timeline_with_eight_builders() {
    let r = timeline(8, 60_000);
    // Measured 8 builders: 20 wall tiles ~3,500, the threatened side ~10,000 and
    // the whole domain 39,600 ticks (the branch before: not finished at 60,000).
    assert!(r.2.is_some_and(|t| t <= 45_000), "{r:?}");
}
