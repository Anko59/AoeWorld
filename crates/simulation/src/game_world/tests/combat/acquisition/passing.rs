use super::*;

/// Defenders standing idle at (100..104, 100..103); ten enemies walk north
/// to south three tiles east of them. Returns how many defenders engaged.
fn engaged(formation: bool) -> (usize, usize) {
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
