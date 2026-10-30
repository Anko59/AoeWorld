use super::economy::{building_rule, player_rule, unit_rule};
use super::*;
use std::fmt;

/// How the strict decoder reports a violation, kept so existing callers still
/// see the same error variants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Class {
    Economy,
    Resource,
    TooMany,
}

/// One broken wire invariant in a server message: which rule, in which
/// message, and the offending entity when a single one is to blame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Violation {
    pub message: &'static str,
    pub rule: &'static str,
    pub entity: Option<EntityId>,
    /// Debug rendering of the offending entity, or the list bound for a
    /// whole-message rule.
    pub detail: String,
    class: Class,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} breaks {}", self.message, self.rule)?;
        if let Some(id) = self.entity {
            write!(f, " at entity {}", id.0)?;
        }
        write!(f, ": {}", self.detail)
    }
}

impl Violation {
    pub(super) fn new(message: &'static str, rule: &'static str, class: Class) -> Self {
        Self {
            message,
            rule,
            entity: None,
            detail: String::new(),
            class,
        }
    }
    pub(super) fn entity(
        message: &'static str,
        rule: &'static str,
        id: EntityId,
        detail: &dyn fmt::Debug,
    ) -> Self {
        Self {
            entity: Some(id),
            detail: format!("{detail:?}"),
            ..Self::new(message, rule, Class::Economy)
        }
    }
    fn error(&self) -> Error {
        match self.class {
            Class::Economy => Error::InvalidEconomyState,
            Class::Resource => Error::InvalidResourceState,
            Class::TooMany => Error::TooManyUnits,
        }
    }
}

fn sorted<T>(items: &[T], key: impl Fn(&T) -> u64) -> bool {
    items.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}

fn push_entities<T: fmt::Debug>(
    out: &mut Vec<Violation>,
    message: &'static str,
    items: &[T],
    id: impl Fn(&T) -> EntityId,
    rule: impl Fn(&T) -> Option<&'static str>,
) {
    for item in items {
        if let Some(rule) = rule(item) {
            out.push(Violation::entity(message, rule, id(item), item));
        }
    }
}

fn whole(out: &mut Vec<Violation>, message: &'static str, rule: &'static str, class: Class) {
    out.push(Violation::new(message, rule, class));
}

/// Every wire invariant a server message breaks, empty when it is valid.
pub fn server_violations(message: &ServerMessage) -> Vec<Violation> {
    let mut out = Vec::new();
    match message {
        ServerMessage::MoveQueues {
            revision, queues, ..
        } => {
            let name = "MoveQueues";
            if *revision == 0 {
                whole(&mut out, name, "revision_nonzero", Class::Economy);
            }
            if queues.len() > MAX_SUBSCRIBED_UNITS {
                whole(&mut out, name, "queues_bound", Class::Economy);
            }
            if !sorted(queues, |q| u64::from(q.entity_id.0)) {
                whole(&mut out, name, "queues_sorted_by_id", Class::Economy);
            }
            push_entities(
                &mut out,
                name,
                queues,
                |q| q.entity_id,
                |q| (!queues::valid_queue(q)).then_some("queue.destinations"),
            );
        }
        ServerMessage::WildlifeState { animals, .. } => {
            let name = "WildlifeState";
            if animals.len() > MAX_ANIMALS {
                whole(&mut out, name, "animals_bound", Class::Economy);
            }
            if !sorted(animals, |a| u64::from(a.id.0)) {
                whole(&mut out, name, "animals_sorted_by_id", Class::Economy);
            }
            push_entities(
                &mut out,
                name,
                animals,
                |a| a.id,
                |a| (!wildlife::valid_animal(a)).then_some("animal.invariants"),
            );
        }
        ServerMessage::Projectiles {
            revision,
            tick,
            projectiles,
        } => {
            let name = "Projectiles";
            if *revision == 0 {
                whole(&mut out, name, "revision_nonzero", Class::Economy);
            }
            if projectiles.len() > MAX_PROJECTILES {
                whole(&mut out, name, "projectiles_bound", Class::Economy);
            }
            if !sorted(projectiles, |p| p.id) {
                whole(&mut out, name, "projectiles_sorted_by_id", Class::Economy);
            }
            for p in projectiles {
                if !projectiles::valid_projectile(p, *tick) {
                    let mut v = Violation::new(name, "projectile.invariants", Class::Economy);
                    v.detail = format!("{p:?} at tick {tick:?}");
                    out.push(v);
                }
            }
        }
        ServerMessage::FogState(state) if !state.valid() => {
            whole(&mut out, "FogState", "fog.state", Class::Economy);
        }
        ServerMessage::ResourceState(state) if !state.valid() => {
            whole(
                &mut out,
                "ResourceState",
                "resources.state",
                Class::Resource,
            );
        }
        ServerMessage::Snapshot {
            units,
            players,
            buildings,
            ..
        } => {
            let name = "Snapshot";
            if units.len() > MAX_SUBSCRIBED_UNITS {
                whole(&mut out, name, "units_bound", Class::TooMany);
            }
            entity_lists(&mut out, name, units, players, buildings);
        }
        ServerMessage::Tick {
            changed_units,
            removals,
            changed_players,
            changed_buildings,
            building_removals,
            ..
        } => {
            let name = "Tick";
            if changed_units.len() > MAX_SUBSCRIBED_UNITS || removals.len() > MAX_SUBSCRIBED_UNITS {
                whole(&mut out, name, "units_bound", Class::TooMany);
            }
            if building_removals.len() > MAX_ECONOMY_BUILDINGS {
                whole(&mut out, name, "building_removals_bound", Class::Economy);
            }
            entity_lists(
                &mut out,
                name,
                changed_units,
                changed_players,
                changed_buildings,
            );
        }
        _ => {}
    }
    out
}

fn entity_lists(
    out: &mut Vec<Violation>,
    name: &'static str,
    units: &[UnitState],
    players: &[GameplayPlayerState],
    buildings: &[GameplayBuildingState],
) {
    push_entities(out, name, units, |u| u.id, unit_rule);
    if players.len() > MAX_ECONOMY_PLAYERS {
        whole(out, name, "players_bound", Class::Economy);
    }
    if !sorted(players, |p| u64::from(p.player.0)) {
        whole(out, name, "players_sorted_by_id", Class::Economy);
    }
    for p in players {
        if let Some(rule) = player_rule(p) {
            let mut v = Violation::new(name, rule, Class::Economy);
            v.detail = format!("{p:?}");
            out.push(v);
        }
    }
    if buildings.len() > MAX_ECONOMY_BUILDINGS {
        whole(out, name, "buildings_bound", Class::Economy);
    }
    if !sorted(buildings, |b| u64::from(b.id.0)) {
        whole(out, name, "buildings_sorted_by_id", Class::Economy);
    }
    push_entities(out, name, buildings, |b| b.id, building_rule);
}

pub(super) fn validate_server(message: &ServerMessage) -> Result<(), Error> {
    match server_violations(message).first() {
        None => Ok(()),
        Some(violation) => Err(violation.error()),
    }
}
