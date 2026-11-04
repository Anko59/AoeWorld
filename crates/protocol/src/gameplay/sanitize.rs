//! Graceful degradation for server-built messages. The strict validators stay
//! on the decode path for untrusted input; here the server drops or repairs the
//! one entity that breaks a rule so the rest of the state still reaches the
//! client instead of the whole message failing forever.
use super::validation::{Class, Violation, server_violations};
use super::*;
use economy::{building_rule, player_rule, unit_rule};
use std::fmt;

/// Repairs the named rule in place by clearing or clamping the offending
/// field; `false` when the rule has no safe repair (the entity is omitted).
fn repair_unit(rule: &str, u: &mut UnitState) -> bool {
    match rule {
        "unit.home_castle" => u.home_castle = None,
        "unit.managed_by" => u.managed_by = None,
        "unit.commanded_by" => u.commanded_by = None,
        "unit.general" => u.general = None,
        "unit.sergeant" => u.sergeant = false,
        "unit.cargo_count" => u.cargo_count = 0,
        "unit.facing" => u.facing &= 7,
        "unit.auto_scout" => u.auto_scout = false,
        "unit.hp_at_most_max_hp" => u.hp = u.max_hp,
        "unit.death_tick_only_when_dead" => u.death_tick = None,
        "unit.death_hunt" => u.death_hunt = None,
        "unit.hunt_target" => u.hunt_target = None,
        "unit.attack_target_when_dead" => u.attack_target = None,
        "unit.gathering" => u.gathering = None,
        "unit.carried" => u.carried = None,
        _ => return false,
    }
    true
}

fn repair_building(rule: &str, b: &mut GameplayBuildingState) -> bool {
    match rule {
        "building.rally_point" => b.rally_point = None,
        "building.gate_open" => b.gate_open = false,
        "building.hp_at_most_max_hp" => b.hp = b.max_hp,
        "building.garrison" => garrison::repair(b),
        "building.training_queue" | "building.queue_needs_complete" => b.training_queue.clear(),
        "building.research" => b.research = None,
        "building.farm_reseeds" => b.farm_reseeds = 0,
        _ => return false,
    }
    true
}

/// Repairs or removes each entity in `items` that breaks a rule, recording one
/// violation per entity with its state before the repair.
fn degrade<T: Clone + fmt::Debug>(
    items: &mut Vec<T>,
    id: impl Fn(&T) -> Option<EntityId>,
    check: impl Fn(&T) -> Option<&'static str>,
    repair: impl Fn(&str, &mut T) -> bool,
    message: &'static str,
    out: &mut Vec<Violation>,
) {
    items.retain_mut(|item| {
        let Some(first) = check(item) else {
            return true;
        };
        let before = item.clone();
        let mut broken = Some(first);
        for _ in 0..8 {
            let Some(rule) = broken else { break };
            if !repair(rule, item) {
                break;
            }
            broken = check(item);
        }
        let mut violation = Violation::new(message, first, Class::Economy);
        violation.entity = id(&before);
        let action = if broken.is_none() {
            "repaired"
        } else {
            "omitted"
        };
        violation.detail = format!("{action} {before:?}");
        out.push(violation);
        broken.is_none()
    });
}

fn sort_dedup<T>(items: &mut Vec<T>, key: impl Fn(&T) -> u64) {
    if !items.windows(2).all(|pair| key(&pair[0]) < key(&pair[1])) {
        items.sort_by_key(&key);
        items.dedup_by_key(|item| key(item));
    }
}

fn sanitize_entities(
    name: &'static str,
    units: &mut Vec<UnitState>,
    players: &mut Vec<GameplayPlayerState>,
    buildings: &mut Vec<GameplayBuildingState>,
    out: &mut Vec<Violation>,
) {
    degrade(units, |u| Some(u.id), unit_rule, repair_unit, name, out);
    degrade(players, |_| None, player_rule, |_, _| false, name, out);
    sort_dedup(players, |p| u64::from(p.player.0));
    degrade(
        buildings,
        |b| Some(b.id),
        building_rule,
        repair_building,
        name,
        out,
    );
    sort_dedup(buildings, |b| u64::from(b.id.0));
}

/// Repairs or omits every offending entity of a server-built message and
/// returns what was wrong, so the caller can log it. Whole-message rules
/// (revision zero, oversized lists, fog or resource frames) are not repaired:
/// the message then still fails encoding and the caller reports `server_violations`.
pub fn sanitize_server(message: &mut ServerMessage) -> Vec<Violation> {
    let mut out = Vec::new();
    match message {
        ServerMessage::Snapshot {
            units,
            players,
            buildings,
            ..
        } => sanitize_entities("Snapshot", units, players, buildings, &mut out),
        ServerMessage::Tick {
            changed_units,
            changed_players,
            changed_buildings,
            ..
        } => sanitize_entities(
            "Tick",
            changed_units,
            changed_players,
            changed_buildings,
            &mut out,
        ),
        ServerMessage::WildlifeState { animals, .. } => {
            degrade(
                animals,
                |a| Some(a.id),
                |a| (!wildlife::valid_animal(a)).then_some("animal.invariants"),
                |_, _| false,
                "WildlifeState",
                &mut out,
            );
            sort_dedup(animals, |a| u64::from(a.id.0));
        }
        ServerMessage::Projectiles {
            tick, projectiles, ..
        } => {
            let tick = *tick;
            degrade(
                projectiles,
                |_| None,
                |p| (!projectiles::valid_projectile(p, tick)).then_some("projectile.invariants"),
                |_, _| false,
                "Projectiles",
                &mut out,
            );
            sort_dedup(projectiles, |p| p.id);
        }
        ServerMessage::MoveQueues { queues, .. } => {
            degrade(
                queues,
                |q| Some(q.entity_id),
                |q| (!queues::valid_queue(q)).then_some("queue.destinations"),
                |_, _| false,
                "MoveQueues",
                &mut out,
            );
            sort_dedup(queues, |q| u64::from(q.entity_id.0));
        }
        _ => {}
    }
    debug_assert!(
        out.is_empty()
            || server_violations(message)
                .iter()
                .all(|v| v.entity.is_none())
    );
    out
}
