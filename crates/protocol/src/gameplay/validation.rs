use super::economy::{building_rule, player_rule, unit_rule};
use super::*;

/// How the strict decoder reports a violation, kept so existing callers still
/// see the same error variants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Class {
    Economy,
    Resource,
    TooMany,
}

/// Receives each broken wire invariant while a message is walked, in walk
/// order. Decoding keeps only the first class; the server-only report keeps
/// every rule with its entity and a Debug rendering of the detail.
pub(super) trait Sink {
    fn found(
        &mut self,
        message: &'static str,
        rule: &'static str,
        class: Class,
        entity: Option<EntityId>,
        detail: Option<&dyn std::fmt::Debug>,
    );
}

/// The decoder's sink: the class of the first broken invariant. It formats
/// nothing, so the Debug output of the entities is not reachable from decode.
#[derive(Default)]
struct FirstClass(Option<Class>);

impl Sink for FirstClass {
    fn found(
        &mut self,
        _message: &'static str,
        _rule: &'static str,
        class: Class,
        _entity: Option<EntityId>,
        _detail: Option<&dyn std::fmt::Debug>,
    ) {
        self.0.get_or_insert(class);
    }
}

/// One broken wire invariant in a server message: which rule, in which
/// message, and the offending entity when a single one is to blame.
#[cfg(feature = "server-repair")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Violation {
    pub message: &'static str,
    pub rule: &'static str,
    pub entity: Option<EntityId>,
    /// Debug rendering of the offending entity, or the list bound for a
    /// whole-message rule.
    pub detail: String,
    pub(super) class: Class,
}

#[cfg(feature = "server-repair")]
impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} breaks {}", self.message, self.rule)?;
        if let Some(id) = self.entity {
            write!(f, " at entity {}", id.0)?;
        }
        write!(f, ": {}", self.detail)
    }
}

#[cfg(feature = "server-repair")]
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
}

/// Every violation of a server message, in walk order.
#[cfg(feature = "server-repair")]
#[derive(Default)]
struct Collect(Vec<Violation>);

#[cfg(feature = "server-repair")]
impl Sink for Collect {
    fn found(
        &mut self,
        message: &'static str,
        rule: &'static str,
        class: Class,
        entity: Option<EntityId>,
        detail: Option<&dyn std::fmt::Debug>,
    ) {
        self.0.push(Violation {
            message,
            rule,
            entity,
            detail: detail.map(|d| format!("{d:?}")).unwrap_or_default(),
            class,
        });
    }
}

fn sorted<T>(items: &[T], key: impl Fn(&T) -> u64) -> bool {
    items.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}

fn push_entities<T: std::fmt::Debug, S: Sink>(
    sink: &mut S,
    message: &'static str,
    items: &[T],
    id: impl Fn(&T) -> EntityId,
    rule: impl Fn(&T) -> Option<&'static str>,
) {
    for item in items {
        if let Some(rule) = rule(item) {
            sink.found(
                message,
                rule,
                Class::Economy,
                Some(id(item)),
                Some(item as &dyn std::fmt::Debug),
            );
        }
    }
}

fn whole<S: Sink>(sink: &mut S, message: &'static str, rule: &'static str, class: Class) {
    sink.found(message, rule, class, None, None);
}

/// Walks every wire invariant a server message can break, reporting each to `sink`.
fn walk<S: Sink>(message: &ServerMessage, sink: &mut S) {
    match message {
        ServerMessage::MoveQueues {
            revision, queues, ..
        } => {
            let name = "MoveQueues";
            if *revision == 0 {
                whole(sink, name, "revision_nonzero", Class::Economy);
            }
            if queues.len() > MAX_SUBSCRIBED_UNITS {
                whole(sink, name, "queues_bound", Class::Economy);
            }
            if !sorted(queues, |q| u64::from(q.entity_id.0)) {
                whole(sink, name, "queues_sorted_by_id", Class::Economy);
            }
            push_entities(
                sink,
                name,
                queues,
                |q| q.entity_id,
                |q| (!queues::valid_queue(q)).then_some("queue.destinations"),
            );
        }
        ServerMessage::WildlifeState { animals, .. } => {
            let name = "WildlifeState";
            if animals.len() > MAX_ANIMALS {
                whole(sink, name, "animals_bound", Class::Economy);
            }
            if !sorted(animals, |a| u64::from(a.id.0)) {
                whole(sink, name, "animals_sorted_by_id", Class::Economy);
            }
            push_entities(
                sink,
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
                whole(sink, name, "revision_nonzero", Class::Economy);
            }
            if projectiles.len() > MAX_PROJECTILES {
                whole(sink, name, "projectiles_bound", Class::Economy);
            }
            if !sorted(projectiles, |p| p.id) {
                whole(sink, name, "projectiles_sorted_by_id", Class::Economy);
            }
            for p in projectiles {
                if !projectiles::valid_projectile(p, *tick) {
                    sink.found(
                        name,
                        "projectile.invariants",
                        Class::Economy,
                        None,
                        Some(&(p, *tick)),
                    );
                }
            }
        }
        ServerMessage::FogState(state) if !state.valid() => {
            whole(sink, "FogState", "fog.state", Class::Economy);
        }
        ServerMessage::ResourceState(state) if !state.valid() => {
            whole(sink, "ResourceState", "resources.state", Class::Resource);
        }
        ServerMessage::Snapshot {
            units,
            players,
            buildings,
            ..
        } => {
            let name = "Snapshot";
            if units.len() > MAX_SUBSCRIBED_UNITS {
                whole(sink, name, "units_bound", Class::TooMany);
            }
            entity_lists(sink, name, units, players, buildings);
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
                whole(sink, name, "units_bound", Class::TooMany);
            }
            if building_removals.len() > MAX_ECONOMY_BUILDINGS {
                whole(sink, name, "building_removals_bound", Class::Economy);
            }
            entity_lists(
                sink,
                name,
                changed_units,
                changed_players,
                changed_buildings,
            );
        }
        _ => {}
    }
}

fn entity_lists<S: Sink>(
    sink: &mut S,
    name: &'static str,
    units: &[UnitState],
    players: &[GameplayPlayerState],
    buildings: &[GameplayBuildingState],
) {
    push_entities(sink, name, units, |u| u.id, unit_rule);
    if players.len() > MAX_ECONOMY_PLAYERS {
        whole(sink, name, "players_bound", Class::Economy);
    }
    if !sorted(players, |p| u64::from(p.player.0)) {
        whole(sink, name, "players_sorted_by_id", Class::Economy);
    }
    for p in players {
        if let Some(rule) = player_rule(p) {
            sink.found(name, rule, Class::Economy, None, Some(p));
        }
    }
    if buildings.len() > MAX_ECONOMY_BUILDINGS {
        whole(sink, name, "buildings_bound", Class::Economy);
    }
    if !sorted(buildings, |b| u64::from(b.id.0)) {
        whole(sink, name, "buildings_sorted_by_id", Class::Economy);
    }
    push_entities(sink, name, buildings, |b| b.id, building_rule);
}

/// Every wire invariant a server message breaks, in walk order.
#[cfg(feature = "server-repair")]
pub fn server_violations(message: &ServerMessage) -> Vec<Violation> {
    let mut collect = Collect::default();
    walk(message, &mut collect);
    collect.0
}

pub(super) fn validate_server(message: &ServerMessage) -> Result<(), Error> {
    let mut first = FirstClass::default();
    walk(message, &mut first);
    match first.0 {
        None => Ok(()),
        Some(Class::Economy) => Err(Error::InvalidEconomyState),
        Some(Class::Resource) => Err(Error::InvalidResourceState),
        Some(Class::TooMany) => Err(Error::TooManyUnits),
    }
}
