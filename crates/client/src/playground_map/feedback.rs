//! Player-facing descriptions use loaded resources and acknowledged orders only.
use super::{CHUNK_TILES, Client, terrain_visible_tiles};
use crate::resource_state::ResourceStateCache;
use aoe_core::TileCoord;
use aoe_map::{ObjectKind, ResourceNode};
use aoe_protocol::{CommandResult, MAX_ACK_HISTORY};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(crate) struct TerrainFailures {
    pub chunks: BTreeSet<(i32, i32)>,
    pub bounds: bool,
}

impl TerrainFailures {
    pub fn message(&self) -> Option<&'static str> {
        (self.bounds || !self.chunks.is_empty())
            .then_some("Some terrain could not be loaded; retrying.")
    }
}

pub(in super::super) fn retain_visible_failures(client: &mut Client) {
    let visible = terrain_visible_tiles(client);
    client.terrain_bounds.failures.chunks.retain(|&(x, y)| {
        x >= visible.min.x.div_euclid(CHUNK_TILES)
            && x <= visible.max.x.saturating_sub(1).div_euclid(CHUNK_TILES)
            && y >= visible.min.y.div_euclid(CHUNK_TILES)
            && y <= visible.max.y.saturating_sub(1).div_euclid(CHUNK_TILES)
    });
}

pub(in super::super) fn connection_label(client: &Client) -> &str {
    if client.status == "connected" {
        client
            .terrain_bounds
            .failures
            .message()
            .unwrap_or(&client.status)
    } else {
        &client.status
    }
}

pub(crate) fn live_blocker(node: &ResourceNode, state: &ResourceStateCache) -> Option<ObjectKind> {
    (node.initial_amount > 0 && state.visible(node.id)).then_some(node.object)
}

pub(in super::super) fn blocker_at(client: &Client, tile: TileCoord) -> Option<ObjectKind> {
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
        RejectedInvalidDestination => match blocker {
            Some(ObjectKind::Tree) => "Cannot move there: a tree is in the way.",
            Some(ObjectKind::ForageBush) => "Cannot move there: a forage bush is in the way.",
            Some(ObjectKind::GoldDeposit) => "Cannot move there: a gold deposit is in the way.",
            Some(ObjectKind::StoneDeposit) => "Cannot move there: a stone deposit is in the way.",
            Some(ObjectKind::Decoration) => "Cannot move there: a resource is in the way.",
            None => "Cannot move there: the destination is blocked or outside the map.",
        },
        RejectedUnreachable => "Cannot move there: no reachable route was found.",
        RejectedPathBudgetExceeded => {
            "Cannot plan this route: the path planning limit was reached."
        }
    }
}

#[derive(Default)]
pub(crate) struct OrderFeedback(BTreeMap<u64, Option<ObjectKind>>);

impl OrderFeedback {
    pub fn record(&mut self, sequence: u64, blocker: Option<ObjectKind>) {
        self.0.insert(sequence, blocker);
        while self.0.len() > MAX_ACK_HISTORY {
            self.0.pop_first();
        }
    }

    pub fn acknowledge(&mut self, sequence: u64, result: CommandResult) -> &'static str {
        order_message(result, self.0.remove(&sequence).flatten())
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}
