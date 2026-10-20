//! Player-facing descriptions use loaded resources and acknowledged orders only.
use super::{CHUNK_TILES, Client, heights};
use crate::resource_state::ResourceStateCache;
use aoe_core::TileCoord;
use aoe_map::{ObjectKind, ResourceNode};
use aoe_protocol::{CommandResult, MAX_ACK_HISTORY};
use std::collections::VecDeque;

const MAX_FAILED_CHUNKS: usize = 64;

#[derive(Default)]
pub(crate) struct FailedChunks(VecDeque<(i32, i32)>);

impl FailedChunks {
    pub fn insert(&mut self, coordinate: (i32, i32)) -> bool {
        if self.contains(&coordinate) {
            return false;
        }
        if self.len() == MAX_FAILED_CHUNKS {
            self.0.pop_front();
        }
        self.0.push_back(coordinate);
        true
    }

    pub fn remove(&mut self, coordinate: &(i32, i32)) -> bool {
        let old_len = self.len();
        self.0.retain(|value| value != coordinate);
        self.len() != old_len
    }

    pub fn contains(&self, coordinate: &(i32, i32)) -> bool {
        self.0.contains(coordinate)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &(i32, i32)> {
        self.0.iter()
    }
}

#[derive(Default)]
pub(crate) struct TerrainFailures {
    pub chunks: FailedChunks,
    pub bounds: bool,
}

impl TerrainFailures {
    pub fn message(&self) -> Option<&'static str> {
        (self.bounds || !self.chunks.is_empty())
            .then_some("Some terrain could not be loaded; retrying.")
    }
}

pub(in super::super) fn chunk_is_visible(client: &Client, coordinate: (i32, i32)) -> bool {
    // Unloaded terrain can project into view above the resident height range.
    let bounds = client.terrain_bounds.levels.unwrap_or((i16::MIN, i16::MAX));
    heights::chunk_may_be_visible(client.camera, client.config, coordinate, bounds)
}

fn has_visible_failure(client: &Client) -> bool {
    let failures = &client.terrain_bounds.failures;
    failures.bounds
        || failures
            .chunks
            .iter()
            .any(|&coordinate| chunk_is_visible(client, coordinate))
}

pub(in super::super) fn connection_label(client: &Client) -> &str {
    if client.status == "connected" && has_visible_failure(client) {
        client
            .terrain_bounds
            .failures
            .message()
            .unwrap_or(&client.status)
    } else {
        &client.status
    }
}

pub(in super::super) fn terrain_failed(client: &mut Client) {
    client.order_feedback.dismiss(&mut client.status);
}

pub(crate) fn live_blocker(node: &ResourceNode, state: &ResourceStateCache) -> Option<ObjectKind> {
    (node.initial_amount > 0 && state.visible(node.id)).then_some(node.object)
}

pub(in super::super) fn blocker_at(client: &Client, tile: TileCoord) -> Option<ObjectKind> {
    if let Some(node) = client
        .resources
        .starting_nodes()
        .find(|node| node.tile == tile)
    {
        return Some(node.object);
    }
    client
        .terrain_chunks
        .get(&(
            tile.x.div_euclid(CHUNK_TILES),
            tile.y.div_euclid(CHUNK_TILES),
        ))?
        .resources()
        .find_map(|(node, _)| {
            (node.tile == tile)
                .then(|| live_blocker(node, &client.resources))
                .flatten()
        })
}

pub(crate) fn movement_label(passable: bool, blocker: Option<ObjectKind>) -> &'static str {
    match blocker {
        Some(ObjectKind::Tree) => "tree, blocked",
        Some(ObjectKind::ForageBush) => "forage bush, blocked",
        Some(ObjectKind::GoldDeposit) => "gold deposit, blocked",
        Some(ObjectKind::StoneDeposit) => "stone deposit, blocked",
        Some(ObjectKind::Decoration) => "resource, blocked",
        Some(ObjectKind::ShoreFish | ObjectKind::DeepFish) => "fish",
        None if passable => "passable",
        None => "blocked",
    }
}

pub(crate) fn order_message(result: CommandResult, blocker: Option<ObjectKind>) -> &'static str {
    use CommandResult::*;
    match result {
        Accepted => "connected",
        RejectedNotController => "Cannot give orders: this session is observing the game.",
        RejectedUnknownEntity => "Cannot give orders: the unit is no longer available.",
        RejectedSequence => "The order could not be confirmed. Please try again.",
        RejectedRateLimited => "Too many orders at once. Please try again shortly.",
        RejectedQueueFull => "The order queue is full. Please try again shortly.",
        RejectedNotOwner => "Cannot give orders: this entity belongs to another player.",
        RejectedWrongUnitKind => "This unit kind cannot perform that order.",
        RejectedInvalidResource => "Cannot gather: the resource is unknown or exhausted.",
        RejectedInsufficientFood => "Cannot train: not enough food.",
        RejectedPopulationCap => "Cannot train: the population limit is reached or reserved.",
        RejectedTrainingQueueFull => "Cannot train: the training queue is full.",
        RejectedInvalidTarget => "Cannot attack: that target is unavailable or friendly.",
        RejectedInsufficientResources => "Cannot give orders: not enough resources.",
        RejectedResourceUnavailable => "Cannot give orders: resource terrain is unavailable.",
        RejectedNotBuildable => "Cannot build: that building cannot be constructed.",
        RejectedRequirementMissing => "Cannot build: a required building is missing.",
        RejectedInvalidPlacement => "Cannot build there: the site is blocked or outside the map.",
        RejectedUnderConstruction => "That building is still under construction.",
        RejectedAlreadyComplete => "That building is already complete.",
        RejectedNotResearchable => "That building cannot research that technology.",
        RejectedAlreadyResearched => "That technology is already researched.",
        RejectedResearchBusy => "That research is already underway or the building is busy.",
        RejectedRequiresFeudalAge => "Requires Feudal Age.",
        RejectedTechRequirement => "That technology's requirements are not met.",
        RejectedFarmUnavailable => "That farm is exhausted or already being worked.",
        RejectedInvalidDestination => match blocker {
            Some(ObjectKind::Tree) => "Cannot move there: a tree is in the way.",
            Some(ObjectKind::ForageBush) => "Cannot move there: a forage bush is in the way.",
            Some(ObjectKind::GoldDeposit) => "Cannot move there: a gold deposit is in the way.",
            Some(ObjectKind::StoneDeposit) => "Cannot move there: a stone deposit is in the way.",
            Some(ObjectKind::Decoration) => "Cannot move there: a resource is in the way.",
            Some(ObjectKind::ShoreFish | ObjectKind::DeepFish) => {
                "Select a Fishing Ship or shore fisherman to fish."
            }
            None => "Cannot move there: the destination is blocked or outside the map.",
        },
        RejectedUnreachable => "Cannot move there: no reachable route was found.",
        RejectedPathBudgetExceeded => {
            "Cannot plan this route: the path planning limit was reached."
        }
    }
}

#[derive(Default)]
pub(crate) struct OrderFeedback {
    // At most MAX_ACK_HISTORY entries; a scan beats a map for this size.
    targets: Vec<(u64, Option<ObjectKind>)>,
    message: Option<&'static str>,
}

impl OrderFeedback {
    pub fn record(&mut self, sequence: u64, blocker: Option<ObjectKind>) {
        match self
            .targets
            .iter_mut()
            .find(|(known, _)| *known == sequence)
        {
            Some(entry) => entry.1 = blocker,
            None => self.targets.push((sequence, blocker)),
        }
        while self.targets.len() > MAX_ACK_HISTORY {
            let oldest = (0..self.targets.len()).min_by_key(|&i| self.targets[i].0);
            self.targets.swap_remove(oldest.unwrap_or(0));
        }
    }

    pub fn acknowledge(&mut self, sequence: u64, result: CommandResult) -> &'static str {
        let blocker = self
            .targets
            .iter()
            .position(|(known, _)| *known == sequence)
            .and_then(|slot| self.targets.swap_remove(slot).1);
        let message = order_message(result, blocker);
        self.message = (result != CommandResult::Accepted).then_some(message);
        message
    }

    fn dismiss(&mut self, status: &mut String) {
        if self
            .message
            .take()
            .is_some_and(|message| status.as_str() == message)
        {
            *status = "connected".to_owned();
        }
    }

    pub fn clear(&mut self) {
        self.targets.clear();
        self.message = None;
    }
}
