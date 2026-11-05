//! One rule for detached soldiers: any player order detaches and restarts the
//! clock; only a player's order to his own General readmits idle, near,
//! delayed soldiers. Each test pins one reviewer finding.
use super::*;

fn muster(w: &mut GameWorld) -> (Vec<EntityId>, EntityId) {
    let ids: Vec<_> = (0..6)
        .map(|n| soldier(w, UnitKind::Militia, 100 + n % 3, 100 + n / 3))
        .collect();
    let g = soldier(w, UnitKind::General, 102, 104);
    (ids, g)
}

fn clock(w: &GameWorld, id: EntityId) -> Option<u64> {
    w.economy.generals.manual.get(&id).copied()
}

#[test]
fn an_order_naming_a_general_and_his_soldiers_detaches_the_soldiers() {
    let (mut w, _) = setup();
    let (ids, g) = muster(&mut w);
    advance(&mut w, 20);
    assert_eq!(w.general_controller(ids[0]), Some(g));
    let mut named = ids[..3].to_vec();
    named.push(g);
    w.general_player_order(PlayerId(0), &named, Some(at(150, 104)));
    for &id in &ids[..3] {
        assert_eq!(w.general_controller(id), None, "{id:?} follows its order");
        assert!(clock(&w, id).is_some());
    }
    assert_eq!(w.general_controller(ids[5]), Some(g), "the rest stay");
}

#[test]
fn orders_restart_a_detached_clock_with_no_army_alive() {
    let (mut w, _) = setup();
    let (ids, g) = muster(&mut w);
    advance(&mut w, 20);
    w.general_player_order(PlayerId(0), &ids[..1], None);
    w.forget_general(g);
    assert!(w.economy.generals.armies.is_empty());
    advance(&mut w, 300);
    w.general_player_order(PlayerId(0), &ids[..1], None);
    assert_eq!(clock(&w, ids[0]), Some(w.tick.0));
}

#[test]
fn a_garrisoned_detached_soldier_keeps_its_protection() {
    let (mut w, castle) = setup();
    let (ids, g) = muster(&mut w);
    advance(&mut w, 20);
    w.issue_garrison(PlayerId(0), ids[0], castle).unwrap();
    w.general_player_order(PlayerId(0), &ids[..1], None);
    advance(&mut w, 600);
    assert!(w.unit(ids[0]).is_none(), "sheltered in the castle");
    w.issue_move(g, at(150, 104)).unwrap();
    w.general_player_order(PlayerId(0), &[g], Some(at(150, 104)));
    assert!(clock(&w, ids[0]).is_some(), "still detached inside");
}

#[test]
fn a_fresh_stop_on_the_general_readmits_eligible_soldiers() {
    let (mut w, _) = setup();
    let (ids, g) = muster(&mut w);
    advance(&mut w, 20);
    w.general_player_order(PlayerId(0), &ids[..1], None);
    advance(&mut w, 400);
    w.issue_stop(PlayerId(0), g).unwrap();
    w.general_player_order(PlayerId(0), &[g], None);
    advance(&mut w, 40);
    assert_eq!(w.general_controller(ids[0]), Some(g));
}

#[test]
fn an_active_auto_scout_is_never_readmitted() {
    let (mut w, _) = setup();
    let (ids, g) = muster(&mut w);
    let scout = soldier(&mut w, UnitKind::Scout, 103, 104);
    advance(&mut w, 20);
    assert_eq!(w.general_controller(scout), Some(g));
    w.issue_auto_scout(PlayerId(0), scout, true).unwrap();
    w.general_player_order(PlayerId(0), &[scout], None);
    advance(&mut w, 400);
    // Searching or sleeping: stationary for the moment. A scout in this
    // village never pauses by itself, so the pause is forced; the order that
    // follows is the real command path.
    let index = w.lookup[&scout];
    let state = &mut w.units[index].state;
    (state.moving, state.planning, state.attack_target) = (false, false, None);
    assert!(w.unit(scout).unwrap().auto_scout);
    w.general_player_order(PlayerId(0), &[g], Some(at(150, 104)));
    assert!(clock(&w, scout).is_some());
    advance(&mut w, 20);
    assert_eq!(w.general_controller(scout), None);
    let _ = ids;
}

#[test]
fn an_automatic_march_never_readmits() {
    let (mut w, _) = setup();
    let (ids, g) = muster(&mut w);
    advance(&mut w, 20);
    w.general_player_order(PlayerId(0), &ids[..1], None);
    advance(&mut w, 400);
    w.general_march(g, at(150, 104));
    assert!(
        clock(&w, ids[0]).is_some(),
        "no player order, still detached"
    );
    advance(&mut w, 20);
    assert_eq!(w.general_controller(ids[0]), None);
}

#[test]
fn a_dead_detached_soldier_leaves_the_map() {
    let (mut w, _) = setup();
    let (ids, g) = muster(&mut w);
    advance(&mut w, 20);
    w.general_player_order(PlayerId(0), &ids[..2], None);
    assert!(clock(&w, ids[0]).is_some());
    let index = w.lookup[&ids[0]];
    w.kill_unit(index);
    assert_eq!(clock(&w, ids[0]), None, "killed: entry gone at once");
    // An entry that outlived its soldier is swept at the next General order.
    w.economy.generals.manual.insert(EntityId(9_999), 0);
    w.general_player_order(PlayerId(0), &[g], None);
    assert_eq!(clock(&w, EntityId(9_999)), None, "the stray entry is swept");
    assert!(clock(&w, ids[1]).is_some(), "a live, recent one stays");
}
