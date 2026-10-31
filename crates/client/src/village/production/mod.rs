//! Group production uses public state plus bounded, unacknowledged clicks.
use super::{Client, owner, selection::Selection};
use crate::building_ui::availability::{self, Availability};
use aoe_core::{EntityId, PlayerId};
use aoe_protocol::{CommandResult, GameplayBuildingState, GameplayPlayerState, UnitKind};

#[derive(Default)]
pub(crate) struct PendingTraining {
    // Accepted requests remain reserved until the resulting tick arrives.
    entries: Vec<(u64, EntityId, Option<u64>)>,
}
impl PendingTraining {
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub fn can_send(&self) -> bool {
        self.entries.len() < aoe_protocol::MAX_PENDING_COMMANDS_PER_CONNECTION
    }
    pub fn count(&self, id: EntityId) -> usize {
        self.entries.iter().filter(|entry| entry.1 == id).count()
    }
    pub fn record(&mut self, sequence: u64, id: EntityId) {
        if self.can_send() {
            self.entries.push((sequence, id, None));
        }
    }
    pub fn acknowledge(
        &mut self,
        sequence: u64,
        result: CommandResult,
        applied: u64,
        observed: u64,
    ) {
        self.entries.retain_mut(|entry| {
            if entry.0 != sequence {
                return true;
            }
            entry.2 = Some(applied);
            result == CommandResult::Accepted && observed <= applied
        });
    }
    pub fn observe(&mut self, tick: u64) {
        self.entries
            .retain(|entry| entry.2.is_none_or(|applied| applied >= tick));
    }
    pub fn resident_bytes(&self) -> usize {
        self.entries.capacity() * std::mem::size_of::<(u64, EntityId, Option<u64>)>()
    }
}

fn shortest<'a>(
    buildings: &'a [GameplayBuildingState],
    selected: &Selection,
    player: Option<&GameplayPlayerState>,
    owner: PlayerId,
    kind: UnitKind,
    pending: &PendingTraining,
    occupied_home: impl Fn(&GameplayBuildingState) -> bool,
) -> Option<&'a GameplayBuildingState> {
    if !pending.can_send() {
        return None;
    }
    buildings
        .iter()
        .filter(|b| {
            b.hp > 0
                && selected.has_building(b.id)
                && (kind != UnitKind::Vassal || (!occupied_home(b) && pending.count(b.id) == 0))
                && b.training_queue.len() + pending.count(b.id) < aoe_protocol::MAX_TRAINING_QUEUE
                && matches!(
                    availability::training(b, player, owner, kind),
                    Availability::Ready
                )
        })
        .min_by_key(|b| (b.training_queue.len() + pending.count(b.id), b.id))
}

fn representative<'a>(
    buildings: &'a [GameplayBuildingState],
    selected: &Selection,
) -> Option<&'a GameplayBuildingState> {
    buildings
        .iter()
        .filter(|b| b.hp > 0 && selected.has_building(b.id))
        .min_by_key(|b| {
            (
                b.construction != aoe_protocol::ConstructionState::Complete,
                Some(b.id) != selected.building_id,
                b.id,
            )
        })
}

pub(super) fn selected(client: &Client) -> Option<&GameplayBuildingState> {
    representative(&client.village.buildings, &client.selected)
}

fn target(client: &Client, kind: UnitKind) -> Option<&GameplayBuildingState> {
    shortest(
        &client.village.buildings,
        &client.selected,
        client
            .village
            .players
            .iter()
            .find(|p| p.player == owner(client)),
        owner(client),
        kind,
        &client.village.pending_training,
        |b| has_vassal(client, b),
    )
}

/// A Castle owns at most one Vassal: queued, garrisoned or alive in the field.
fn has_vassal(client: &Client, castle: &GameplayBuildingState) -> bool {
    castle
        .training_queue
        .iter()
        .any(|e| e.kind == UnitKind::Vassal)
        || castle.garrison.iter().any(|g| g.kind == UnitKind::Vassal)
        || client
            .units
            .values()
            .any(|u| u.kind == UnitKind::Vassal && u.hp > 0 && u.home_castle == Some(castle.id))
}

fn blocked_reason(
    building: &GameplayBuildingState,
    player: Option<&GameplayPlayerState>,
    owner: PlayerId,
    kind: UnitKind,
    pending: &PendingTraining,
) -> Availability {
    let reason = availability::training(building, player, owner, kind);
    if matches!(reason, Availability::Ready)
        && (!pending.can_send()
            || building.training_queue.len() + pending.count(building.id)
                >= aoe_protocol::MAX_TRAINING_QUEUE)
    {
        Availability::QueueFull
    } else {
        reason
    }
}

pub(super) fn availability(client: &Client, kind: UnitKind) -> Availability {
    if target(client, kind).is_some() {
        return Availability::Ready;
    }
    let Some(building) = selected(client) else {
        return Availability::Unavailable;
    };
    if kind == UnitKind::Vassal
        && (has_vassal(client, building) || client.village.pending_training.count(building.id) > 0)
    {
        return Availability::UniqueUnit;
    }
    blocked_reason(
        building,
        client
            .village
            .players
            .iter()
            .find(|p| p.player == owner(client)),
        owner(client),
        kind,
        &client.village.pending_training,
    )
}

pub(super) fn train(client: &mut Client, kind: UnitKind) {
    let Some(building_id) = target(client, kind).map(|b| b.id) else {
        return;
    };
    let sequence = client.next_sequence;
    super::orders::send(
        client,
        aoe_protocol::GameplayClientMessage::TrainOrder {
            sequence,
            building_id,
            kind,
        },
    );
    client
        .village
        .pending_training
        .record(sequence, building_id);
}

#[cfg(test)]
mod tests;
