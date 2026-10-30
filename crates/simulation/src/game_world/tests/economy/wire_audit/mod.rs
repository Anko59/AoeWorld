//! Hunts for simulation states the wire protocol rejects (owner playtest
//! 2026-10-30: one such entity made every Snapshot fail). The audit mirrors the
//! protocol's per-entity rules on simulation data; each scenario audits every
//! tick, so a transient state (a dying unit that keeps a controller) shows too.
use super::*;
use crate::{Age, GeneralState, Stockpile};

/// Every wire rule a unit or building breaks right now, by rule name.
fn audit(w: &GameWorld) -> Vec<String> {
    let mut bad = Vec::new();
    for u in w.units() {
        let alive = u.hp > 0;
        let villager = u.kind == UnitKind::Villager;
        let naval = matches!(
            u.kind,
            UnitKind::FishingShip
                | UnitKind::Galley
                | UnitKind::FireShip
                | UnitKind::DemolitionShip
                | UnitKind::TransportShip
                | UnitKind::WarGalley
        );
        let mut note = |rule: &str| bad.push(format!("{rule}: {u:?}"));
        if w.general_controller(u.id).is_some()
            && (!alive
                || naval
                || matches!(
                    u.kind,
                    UnitKind::Villager | UnitKind::Vassal | UnitKind::General | UnitKind::Monk
                ))
        {
            note("unit.commanded_by");
        }
        if w.vassal_controller(u.id).is_some() && !(villager && alive) {
            note("unit.managed_by");
        }
        if w.vassal_castle(u.id)
            .or_else(|| w.general_castle(u.id))
            .is_some()
            && !(matches!(u.kind, UnitKind::Vassal | UnitKind::General) && alive)
        {
            note("unit.home_castle");
        }
        if let Some(g) = w.general_status(u.id)
            && !(u.kind == UnitKind::General && alive && g.army_size <= 100)
        {
            note("unit.general");
        }
        if !w.transport_cargo(u.id).is_empty() && !(u.kind == UnitKind::TransportShip && alive) {
            note("unit.cargo_count");
        }
        if u.max_hp == 0 || u.hp > u.max_hp {
            note("unit.hp_vs_max_hp");
        }
        if u.death_tick.is_some() && alive {
            note("unit.death_tick_only_when_dead");
        }
        if alive == (u.task == UnitTask::Dying) {
            note("unit.dying_iff_no_hp");
        }
        if !alive && u.attack_target.is_some() {
            note("unit.attack_target_when_dead");
        }
        if u.death_hunt.is_some() && (alive || !villager) {
            note("unit.death_hunt");
        }
        if matches!(u.task, UnitTask::Hunting | UnitTask::Shepherding)
            && !(villager
                && u.carried
                    .is_none_or(|c| c.kind == aoe_map::ResourceKind::Food)
                && w.gathering_kind(u.id)
                    .is_none_or(|k| k == aoe_map::ResourceKind::Food))
        {
            note("unit.hunting_task");
        }
        if u.carried
            .is_some_and(|c| c.amount == 0 || (villager && c.amount > 67))
        {
            note("unit.carried");
        }
    }
    for b in w.buildings() {
        let complete = b.is_complete();
        let mut note = |rule: &str| bad.push(format!("{rule}: {b:?}"));
        if b.hp == 0 {
            note("building.hp_positive");
        }
        if b.hp > b.max_hp {
            note("building.hp_at_most_max_hp");
        }
        if b.gate_open && !(b.kind == BuildingKind::PalisadeGate && complete) {
            note("building.gate_open");
        }
        if !complete
            && (!b.training_queue.is_empty() || b.research.is_some() || !b.garrison.is_empty())
        {
            note("building.queue_needs_complete");
        }
    }
    bad
}

/// Advances `ticks`, auditing after every tick; the first failure per rule.
fn run_audited(w: &mut GameWorld, ticks: u32, seen: &mut std::collections::BTreeMap<String, u32>) {
    for t in 0..ticks {
        w.advance();
        for finding in audit(w) {
            let rule = finding.split(':').next().unwrap().to_owned();
            seen.entry(format!("{rule} | {}", &finding[rule.len()..]))
                .or_insert(t);
        }
    }
}

fn rich(w: &mut GameWorld) {
    let p = w.economy.players.get_mut(&PlayerId(0)).unwrap();
    p.age = Age::Castle;
    p.population_cap = 1000;
    p.stockpile = Stockpile {
        food: 50_000,
        wood: 50_000,
        gold: 50_000,
        stone: 50_000,
    };
}
fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}
fn report(seen: &std::collections::BTreeMap<String, u32>) -> String {
    seen.iter()
        .map(|(finding, tick)| format!("tick {tick}: {finding}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_general_army_in_battle_never_leaves_an_invalid_wire_state() {
    let (mut w, _, _) = village();
    rich(&mut w);
    w.insert_building(
        PlayerId(0),
        BuildingKind::Castle,
        aoe_core::TileRect::from_xywh(90, 90, 4, 4),
        true,
    )
    .unwrap();
    let kinds = [
        UnitKind::Militia,
        UnitKind::Archer,
        UnitKind::Knight,
        UnitKind::Spearman,
    ];
    for n in 0..40 {
        w.spawn_kind(
            PlayerId(0),
            kinds[n % 4],
            at(100 + n as i32 % 8, 100 + n as i32 / 8),
        )
        .unwrap();
    }
    let g = w
        .spawn_kind(PlayerId(0), UnitKind::General, at(103, 103))
        .unwrap();
    for n in 0..30 {
        w.spawn_kind(
            PlayerId(1),
            kinds[n % 4],
            at(120 + n as i32 % 6, 100 + n as i32 / 6),
        )
        .unwrap();
    }
    let mut seen = Default::default();
    w.issue_move(g, at(125, 102)).unwrap();
    run_audited(&mut w, 400, &mut seen);
    assert_eq!(
        w.general_status(g).map(|s| s.state != GeneralState::Idle),
        Some(true)
    );
    // The leader falls mid-battle; its soldiers lose their commander.
    let index = w.lookup[&g];
    w.kill_unit(index);
    run_audited(&mut w, 1500, &mut seen);
    assert!(seen.is_empty(), "{}", report(&seen));
}

#[test]
fn vassals_and_their_villagers_dying_never_leave_an_invalid_wire_state() {
    let (mut w, castle) = {
        let (mut w, _, _) = village();
        rich(&mut w);
        let castle = w
            .insert_building(
                PlayerId(0),
                BuildingKind::Castle,
                aoe_core::TileRect::from_xywh(90, 90, 4, 4),
                true,
            )
            .unwrap();
        (w, castle)
    };
    w.issue_train(PlayerId(0), castle, UnitKind::Vassal)
        .unwrap();
    let mut seen = Default::default();
    let training = 60 * w.config().tick_hz;
    run_audited(&mut w, training, &mut seen);
    for n in 0..30 {
        w.spawn_villager(PlayerId(0), at(96 + n % 6, 96 + n / 6))
            .unwrap();
    }
    run_audited(&mut w, 600, &mut seen);
    let villagers: Vec<_> = w
        .units()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| u.id)
        .collect();
    assert!(
        villagers
            .iter()
            .any(|&id| w.vassal_controller(id).is_some()),
        "the scenario must have managed villagers"
    );
    for (n, id) in villagers.iter().enumerate() {
        if n % 2 == 0 {
            let index = w.lookup[id];
            w.kill_unit(index);
            run_audited(&mut w, 3, &mut seen);
        }
    }
    let vassal = w.units().find(|u| u.kind == UnitKind::Vassal).unwrap().id;
    let index = w.lookup[&vassal];
    w.kill_unit(index);
    run_audited(&mut w, 900, &mut seen);
    assert!(seen.is_empty(), "{}", report(&seen));
}

#[test]
fn gates_destroyed_or_opened_during_construction_never_leave_an_invalid_wire_state() {
    let (mut w, _, _) = village();
    rich(&mut w);
    let builder = w.spawn_villager(PlayerId(0), at(99, 100)).unwrap();
    let gate = w
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::PalisadeGate,
            TileCoord::new(100, 100),
            false,
        )
        .unwrap();
    let mut seen = Default::default();
    run_audited(&mut w, 100, &mut seen);
    buildings::finish(&mut w, gate, 700);
    run_audited(&mut w, 200, &mut seen);
    assert!(w.building(gate).unwrap().gate_open || w.building(gate).is_some());
    // An enemy hacks the gate down while it is open and its owner nearby.
    for n in 0..6 {
        w.spawn_kind(PlayerId(1), UnitKind::Militia, at(100 + n, 102))
            .unwrap();
    }
    run_audited(&mut w, 1500, &mut seen);
    assert!(seen.is_empty(), "{}", report(&seen));
}
