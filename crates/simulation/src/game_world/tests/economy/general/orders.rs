//! A General's army orders lead the army; a "General only" move does not.
use super::*;

fn muster(w: &mut GameWorld) -> (Vec<EntityId>, EntityId) {
    let ids: Vec<_> = (0..12)
        .map(|n| soldier(w, UnitKind::Militia, 100 + n % 4, 100 + n / 4))
        .collect();
    let g = soldier(w, UnitKind::General, 102, 104);
    (ids, g)
}

fn walked(w: &GameWorld, id: EntityId, from: WorldPosition) -> i64 {
    let p = w.unit(id).unwrap().position;
    i64::from(p.x - from.x)
        .abs()
        .max(i64::from(p.y - from.y).abs())
        / 1024
}

#[test]
fn army_move_leads_the_army_but_general_only_move_leaves_it_in_place() {
    for solo in [false, true] {
        let (mut w, _) = setup();
        let (ids, g) = muster(&mut w);
        let starts: Vec<_> = ids.iter().map(|&id| w.unit(id).unwrap().position).collect();
        let origin = w.unit(g).unwrap().position;
        if solo {
            w.issue_general_move(PlayerId(0), g, at(150, 104)).unwrap();
        } else {
            w.issue_move(g, at(150, 104)).unwrap();
        }
        advance(&mut w, 500);
        assert!(walked(&w, g, origin) > 20, "General walks (solo {solo})");
        let moved = ids
            .iter()
            .zip(&starts)
            .filter(|&(&id, &p)| walked(&w, id, p) > 10)
            .count();
        if solo {
            assert_eq!(moved, 0, "the army holds position");
            assert_eq!(w.general_status(g).unwrap().state, GeneralState::Idle);
        } else {
            assert_eq!(moved, ids.len(), "the army follows its General");
        }
    }
}

#[test]
fn general_only_move_ends_with_the_next_army_order() {
    let (mut w, _) = setup();
    let (ids, g) = muster(&mut w);
    w.issue_general_move(PlayerId(0), g, at(120, 104)).unwrap();
    advance(&mut w, 100);
    let start = w.unit(ids[0]).unwrap().position;
    w.issue_move(g, at(150, 104)).unwrap();
    advance(&mut w, 300);
    assert!(walked(&w, ids[0], start) > 10, "army marches again");
}

#[test]
fn general_only_move_rejects_other_owners_and_other_units() {
    let (mut w, _) = setup();
    let (ids, g) = muster(&mut w);
    assert!(w.issue_general_move(PlayerId(1), g, at(120, 104)).is_err());
    assert!(
        w.issue_general_move(PlayerId(0), ids[0], at(120, 104))
            .is_err()
    );
}

#[test]
fn general_walks_through_his_own_army_that_fills_a_corridor() {
    let (mut w, _) = setup();
    for x in 100..160 {
        w.economy.blocked_tiles.insert(TileCoord::new(x, 59));
        w.economy.blocked_tiles.insert(TileCoord::new(x, 61));
    }
    let g = soldier(&mut w, UnitKind::General, 101, 60);
    let wall: Vec<_> = (0..16)
        .map(|n| soldier(&mut w, UnitKind::Militia, 110 + n, 60))
        .collect();
    advance(&mut w, 5);
    w.issue_general_move(PlayerId(0), g, at(135, 60)).unwrap();
    advance(&mut w, 900);
    let p = w.unit(g).unwrap().position;
    assert!(p.x > 130 * 1024, "General crossed his army: {p:?}");
    assert!(wall.iter().all(|&id| w.unit(id).unwrap().hp > 0));
}
