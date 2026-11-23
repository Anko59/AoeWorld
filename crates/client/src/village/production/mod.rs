//! Group production uses public state plus bounded, unacknowledged clicks.
use super::{Client, owner, selection::Selection};
use crate::building_ui::availability::{self, Availability};
use aoe_core::{EntityId, PlayerId};
use aoe_protocol::{
    CommandResult, GameplayBuildingState, GameplayPlayerState, Stockpile, UnitKind,
};

pub(crate) mod here;

#[derive(Clone, Default)]
pub(crate) struct PendingTraining {
    // Accepted requests remain reserved until the resulting tick arrives.
    entries: Vec<(u64, EntityId, Option<u64>, UnitKind)>,
    observed: u64,
}
impl PendingTraining {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.observed = 0;
    }
    pub fn can_send(&self) -> bool {
        self.entries.len() < aoe_protocol::MAX_PENDING_COMMANDS_PER_CONNECTION
    }
    pub fn count(&self, id: EntityId) -> usize {
        self.entries.iter().filter(|entry| entry.1 == id).count()
    }
    pub fn record(&mut self, sequence: u64, id: EntityId, kind: UnitKind) {
        if self.can_send() {
            self.entries.push((sequence, id, None, kind));
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
        self.observed = tick;
        self.entries
            .retain(|entry| entry.2.is_none_or(|applied| applied >= tick));
    }
    fn unspent(&self, mut stockpile: Stockpile) -> Stockpile {
        for entry in &self.entries {
            if entry.2.is_none_or(|tick| tick > self.observed) {
                subtract(
                    &mut stockpile,
                    availability::training_cost(entry.3).unwrap_or_default(),
                );
            }
        }
        stockpile
    }

    pub fn resident_bytes(&self) -> usize {
        self.entries.capacity() * std::mem::size_of::<(u64, EntityId, Option<u64>, UnitKind)>()
    }
}

fn shortest<'a>(
    buildings: &'a [GameplayBuildingState],
    selected: &Selection,
    player: Option<&GameplayPlayerState>,
    owner: PlayerId,
    kind: UnitKind,
    pending: &PendingTraining,
    occupied: impl Fn(&GameplayBuildingState) -> bool,
) -> Option<&'a GameplayBuildingState> {
    if !pending.can_send() {
        return None;
    }
    buildings
        .iter()
        .filter(|b| {
            b.hp > 0
                && selected.has_building(b.id)
                && (kind != UnitKind::Steward || (!occupied(b) && pending.count(b.id) == 0))
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
        has_steward,
    )
}

/// A Town Center admits at most one Steward, queued or garrisoned.
pub(in super::super) fn has_steward(castle: &GameplayBuildingState) -> bool {
    castle
        .training_queue
        .iter()
        .any(|e| e.kind == UnitKind::Steward)
        || castle.garrison.iter().any(|g| g.kind == UnitKind::Steward)
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
    if kind == UnitKind::Steward
        && (has_steward(building) || client.village.pending_training.count(building.id) > 0)
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

fn subtract(stockpile: &mut Stockpile, cost: Stockpile) {
    stockpile.food = stockpile.food.saturating_sub(cost.food);
    stockpile.wood = stockpile.wood.saturating_sub(cost.wood);
    stockpile.gold = stockpile.gold.saturating_sub(cost.gold);
    stockpile.stone = stockpile.stone.saturating_sub(cost.stone);
}

/// Publish a selected queue change on the next frame. Work progress retains
/// the ordinary HUD cadence.
pub(in crate::playground::game) fn refresh_changed_queue(
    client: &mut Client,
    changed: &[GameplayBuildingState],
) {
    if changed.iter().any(|building| {
        client.selected.has_building(building.id)
            && client
                .village
                .buildings
                .iter()
                .find(|old| old.id == building.id)
                .is_some_and(|old| {
                    !old.training_queue
                        .iter()
                        .map(|entry| entry.kind)
                        .eq(building.training_queue.iter().map(|entry| entry.kind))
                })
    }) {
        client.village.last_hud = f64::NEG_INFINITY;
    }
}

pub(super) fn train(client: &mut Client, kind: UnitKind, batch: bool) {
    // Managers are trained one at a time, whatever the modifier keys.
    let batch = batch
        && !matches!(
            kind,
            UnitKind::Vassal | UnitKind::General | UnitKind::Steward
        );
    let Some(player) = client
        .village
        .players
        .iter()
        .find(|p| p.player == owner(client))
    else {
        return;
    };
    let Some(cost) = availability::training_cost(kind) else {
        return;
    };
    let mut stockpile = client.village.pending_training.unspent(player.stockpile);
    let limit = if batch {
        aoe_protocol::MAX_TRAIN_BATCH
    } else {
        1
    };
    let sequence = client.next_sequence;
    let mut building_ids = Vec::new();
    for _ in 0..limit {
        if !matches!(availability::cost(stockpile, cost), Availability::Ready) {
            break;
        }
        let Some(building_id) = target(client, kind).map(|b| b.id) else {
            break;
        };
        building_ids.push(building_id);
        client
            .village
            .pending_training
            .record(sequence, building_id, kind);
        subtract(&mut stockpile, cost);
    }
    if building_ids.is_empty() {
        return;
    }
    use aoe_protocol::GameplayClientMessage;
    let request = if batch {
        GameplayClientMessage::TrainBatchOrder {
            sequence,
            kind,
            building_ids,
        }
    } else {
        GameplayClientMessage::TrainOrder {
            sequence,
            building_id: building_ids[0],
            kind,
        }
    };
    if !super::orders::send(client, request) {
        client
            .village
            .pending_training
            .entries
            .retain(|entry| entry.0 != sequence);
    }
}

#[cfg(test)]
mod tests;
