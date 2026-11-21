//! One connected client's state: its slot, view subscription, command
//! budget and the baselines its deltas are diffed against.
use super::super::commands::queue::QueuedCommand;
use super::super::fanout;
use aoe_core::{EntityId, PlayerId, TileRect};
use aoe_protocol::GameplayServerMessage;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

pub(crate) struct Session {
    /// Match slot index. Sessions sharing a slot retain their own order sequences and rate limits.
    pub(crate) slot: usize,
    /// Whose fog-filtered view this session receives: its slot's player.
    pub(crate) viewer: PlayerId,
    /// The slot's Welcome identity for view queries.
    pub(crate) primary_unit_id: EntityId,
    /// AI slots get a higher order budget and quiet idle ticks.
    pub(crate) ai: bool,
    pub(crate) sender: mpsc::Sender<GameplayServerMessage>,
    pub(crate) subscription_delivery: Option<fanout::subscription::Delivery>,
    pub(crate) subscription: Option<Subscription>,
    pub(crate) last_sequence: Option<u64>,
    pub(crate) acknowledgements: VecDeque<u64>,
    /// Acks (one-shot verdicts) waiting for room in the outgoing queue.
    pub(crate) pending_acks: VecDeque<GameplayServerMessage>,
    pub(crate) command_tokens: f64,
    pub(crate) last_command_refill: Instant,
    pub(crate) subscription_times: VecDeque<Instant>,
    pub(in crate::gameplay) pending: VecDeque<QueuedCommand>,
    pub(in crate::gameplay) resident: super::Resident,
    pub(crate) resource_revision: Option<u64>,
    pub(crate) resident_players: BTreeMap<aoe_core::PlayerId, aoe_protocol::GameplayPlayerState>,
    pub(crate) resident_buildings: BTreeMap<EntityId, aoe_protocol::GameplayBuildingState>,
    pub(crate) resident_animals: Vec<aoe_protocol::AnimalState>,
    pub(crate) resident_projectiles: Vec<aoe_protocol::ProjectileState>,
    pub(crate) resident_land: Option<Arc<aoe_simulation::LandControl>>,
    pub(crate) resident_move_queues: Vec<aoe_protocol::MoveQueue>,
    pub(crate) vision_revision: Option<u64>,
    /// The fog view the client holds; an unchanged rebuild is not resent.
    pub(crate) resident_fog: Option<aoe_protocol::FogState>,
    /// Set when the building baseline can no longer be advanced by change
    /// lists alone (a new point of view, fog switched).
    pub(crate) rebuild_buildings: bool,
    pub(crate) resource_vision_revision: Option<u64>,
    pub(crate) resident_resources: Vec<aoe_protocol::ResourceAmount>,
}
#[derive(Clone, Copy)]
pub(crate) struct Subscription {
    pub(crate) revision: u64,
    pub(crate) region: TileRect,
    pub(crate) unit_zoom: bool,
}
