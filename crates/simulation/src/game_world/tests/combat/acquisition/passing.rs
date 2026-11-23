use super::*;

/// Defenders standing idle at (100..104, 100..103); ten enemies walk north
/// to south three tiles east of them: the world, defenders, enemies and the
/// enemies' column.
fn passing(formation: bool) -> (GameWorld, Vec<EntityId>, Vec<EntityId>, i32) {
    let mut w = world();
    let ids = (0..20)
        .map(|n| {
            w.spawn_militia(PlayerId(0), at(96 + n % 5, 96 + n / 5))
                .unwrap()
        })
        .collect::<Vec<_>>();
    if formation {
        w.issue_formation_move(
            PlayerId(0),
            &ids,
            aoe_core::Formation::Line,
            at(102, 101),
            false,
            false,
        )
        .unwrap();
    } else {
        for (n, &id) in ids.iter().enumerate() {
            w.issue_move(id, at(100 + n as i32 % 5, 100 + n as i32 / 5))
                .unwrap();
        }
    }
    advance(&mut w, 300);
    let east = ids
        .iter()
        .map(|&id| w.unit(id).unwrap().position.tile_floor().x)
        .max()
        .unwrap();
    let enemies = (0..10)
        .map(|n| {
            let id = w
                .spawn_militia(PlayerId(1), at(east + 3, 90 - n * 2))
                .unwrap();
            w.issue_move(id, at(east + 3, 140)).unwrap();
            id
        })
        .collect::<Vec<_>>();
    (w, ids, enemies, east)
}

fn engaged(formation: bool) -> (usize, usize) {
    let (mut w, ids, enemies, east) = passing(formation);
    let mut engaged = std::collections::BTreeSet::new();
    for _ in 0..600 {
        w.advance();
        for &id in &ids {
            if w.unit(id).is_some_and(|u| u.attack_target.is_some()) {
                engaged.insert(id);
            }
        }
    }
    let passed = enemies
        .iter()
        .filter(|&&e| w.unit(e).is_some_and(|u| u.position.tile_floor().y > 110))
        .count();
    eprintln!("PASSED {passed} of 10, defenders east edge {east}");
    let hit = enemies
        .iter()
        .filter(|&&e| w.unit(e).is_none_or(|u| u.hp < u.max_hp))
        .count();
    (engaged.len(), hit)
}

#[test]
fn idle_soldiers_engage_enemies_walking_past() {
    let lone = engaged(false);
    let grouped = engaged(true);
    // A block engages as one: mates that cannot see the enemies yet join
    // the member that does, where lone soldiers each wait for their own sight.
    assert!(grouped.0 > lone.0, "lone {lone:?} formation {grouped:?}");
    assert!(grouped.0 >= 18, "lone {lone:?} formation {grouped:?}");
}

#[test]
fn combat_metrics_for_an_idle_group_with_enemies_passing_three_tiles_away() {
    let (mut w, ids, enemies, _) = passing(true);
    let fight = super::super::engage::measure_fight(&mut w, &ids, &enemies, 600);
    // Before the per-unit acquisition turns (dev c8c9337): 9 of 20 hit,
    // 34 hits, 1 kill, first hit p50 249 and p95 271.
    assert!(fight.engaged >= 10 && fight.hits > 0, "{fight:?}");
    assert!(fight.first_hit_p50 <= fight.first_hit_p95, "{fight:?}");
}
