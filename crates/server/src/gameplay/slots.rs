//! Match seats: who may control which player, how a connection claims one, and
//! how a service is assembled with several of them.
use super::{GameplayService, Session, fog, resources};
use crate::gameplay_transport;
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId};
use aoe_protocol::{GAMEPLAY_VERSION, GameplayServerMessage, MapMetadata, ResumeToken};
use aoe_simulation::GameWorld;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, RwLock, mpsc};

#[path = "slots/tests.rs"]
#[cfg(test)]
mod tests;

/// One seat of a match. Every slot owns exactly one player and is reachable
/// through its token; a human slot may also be taken by a plain Hello. Any
/// number of connections may sit in one slot and all of them play it.
pub(crate) struct Slot {
    pub(crate) player: PlayerId,
    pub(crate) primary_unit_id: EntityId,
    pub(crate) human: bool,
    pub(crate) token: ResumeToken,
}

/// A `JoinSlot` named a token no slot of this match owns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnknownSlotToken;

/// What a connection asked for in its first message.
pub(crate) enum Access {
    /// `Hello`: resume with a token, or take a human slot (an unknown token
    /// counts as none: a page from before a restart simply plays again).
    Hello(Option<ResumeToken>),
    /// `JoinSlot`: exactly this slot or nothing.
    Join(ResumeToken),
}

/// Slot layout of a new service: player, primary unit and whether a person
/// (rather than an AI process) is expected.
#[derive(Clone, Copy, Debug)]
pub struct SlotSetup {
    pub player: PlayerId,
    pub primary_unit_id: EntityId,
    pub human: bool,
}

/// What a match creator learns about each slot.
#[derive(Clone, Copy, Debug)]
pub struct SlotInfo {
    pub player: PlayerId,
    pub primary_unit_id: EntityId,
    pub human: bool,
    pub token: ResumeToken,
}

impl GameplayService {
    pub(crate) fn from_world(
        world: GameWorld,
        primary_unit_id: EntityId,
        map_content_hash: Option<[u8; 32]>,
        map_metadata: Option<MapMetadata>,
    ) -> Self {
        let player = fog::viewer(&world, primary_unit_id);
        Self::from_world_slots(
            world,
            vec![SlotSetup {
                player,
                primary_unit_id,
                human: true,
            }],
            map_content_hash,
            map_metadata,
        )
    }

    /// Hosts one match seat per entry; Tokens are issued here and never derived from a counter
    /// a client could guess.
    pub(crate) fn from_world_slots(
        mut world: GameWorld,
        setups: Vec<SlotSetup>,
        map_content_hash: Option<[u8; 32]>,
        map_metadata: Option<MapMetadata>,
    ) -> Self {
        let primary_unit_id = setups.first().map_or(EntityId(0), |s| s.primary_unit_id);
        if crate::Config::fog_disabled_from_env() {
            world.set_fog_disabled(true);
        }
        if crate::Config::fog_showcase_from_env() {
            fog::showcase_enemies(&mut world, primary_unit_id);
        }
        let world_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let slots = setups
            .into_iter()
            .zip(1..)
            .map(|(setup, counter)| Slot {
                player: setup.player,
                primary_unit_id: setup.primary_unit_id,
                human: setup.human,
                token: gameplay_transport::make_token(world_id, counter),
            })
            .collect();
        let probe = Arc::new(crate::gameplay_match::TickProbe::default());
        world.set_planning_probe(probe.clone());
        if let aoe_simulation::Terrain::Map { generator, .. } = world.terrain() {
            generator.set_navigation_probe(probe.clone());
        }
        let world = Arc::new(RwLock::new(world));
        super::probe::track(&world);
        Self {
            world,
            sessions: Arc::new(Mutex::new(BTreeMap::new())),
            slots,
            next_session: Arc::new(AtomicU64::new(1)),
            command_cursor: Arc::default(),
            world_id,
            map_content_hash,
            map_metadata,
            resource_directory: None,
            probe,
            resource_journal: Arc::new(std::sync::Mutex::new(resources::Journal::default())),
            fanout_probe: Arc::default(),
            preparation: Default::default(),
        }
    }

    /// The seats of this service with their tokens, for the match creator.
    pub async fn slot_infos(&self) -> Vec<SlotInfo> {
        self.slots
            .iter()
            .map(|slot| SlotInfo {
                player: slot.player,
                primary_unit_id: slot.primary_unit_id,
                human: slot.human,
                token: slot.token,
            })
            .collect()
    }

    /// Villages of this match whose starting kit was only partly placed.
    pub(crate) fn partial_starting_kits(&self) -> u64 {
        self.world
            .try_read()
            .map_or(0, |world| world.partial_starting_kits())
    }

    /// Where a slot's primary unit stands, as a tile (0,0 if it is gone).
    pub(crate) fn start_tile(&self, unit: EntityId) -> (i32, i32) {
        self.world
            .try_read()
            .ok()
            .and_then(|world| world.unit(unit))
            .map_or((0, 0), |unit| {
                let tile = unit.position.tile_floor();
                (tile.x, tile.y)
            })
    }

    pub async fn register(
        &self,
        requested_token: Option<ResumeToken>,
        sender: mpsc::Sender<GameplayServerMessage>,
    ) -> (u64, GameplayServerMessage) {
        self.register_access(Access::Hello(requested_token), sender)
            .await
            .expect("registration requires a human seat or a valid slot token")
    }

    /// Authenticates for exactly one slot; an unknown token is refused.
    pub async fn join_slot(
        &self,
        token: ResumeToken,
        sender: mpsc::Sender<GameplayServerMessage>,
    ) -> Result<(u64, GameplayServerMessage), UnknownSlotToken> {
        self.register_access(Access::Join(token), sender).await
    }

    pub(crate) async fn register_access(
        &self,
        access: Access,
        sender: mpsc::Sender<GameplayServerMessage>,
    ) -> Result<(u64, GameplayServerMessage), UnknownSlotToken> {
        let config = self.world.read().await.config();
        let mut sessions = self.sessions.lock().await;
        let by_token = |token: ResumeToken| self.slots.iter().position(|s| s.token == token);
        let slot_index = match access {
            Access::Join(token) => by_token(token).ok_or(UnknownSlotToken)?,
            Access::Hello(token) => token
                .and_then(by_token)
                .or_else(|| {
                    // Prefer a human seat nobody sits in, then any human seat.
                    let human = |slot: &&Slot| slot.human;
                    let free = |index: usize| sessions.values().all(|s| s.slot != index);
                    let humans = || self.slots.iter().enumerate().filter(|(_, s)| human(s));
                    humans()
                        .find(|(index, _)| free(*index))
                        .or_else(|| humans().next())
                        .map(|(index, _)| index)
                })
                .ok_or(UnknownSlotToken)?,
        };
        let session_id = self.next_session.fetch_add(1, Ordering::Relaxed);
        let slot = &self.slots[slot_index];
        let (token, viewer, primary, ai) =
            (slot.token, slot.player, slot.primary_unit_id, !slot.human);
        sessions.insert(
            session_id,
            Session {
                slot: slot_index,
                viewer,
                primary_unit_id: primary,
                ai,
                sender,
                subscription_delivery: None,
                subscription: None,
                last_sequence: None,
                acknowledgements: VecDeque::new(),
                pending_acks: VecDeque::new(),
                command_tokens: 20.0,
                last_command_refill: Instant::now(),
                subscription_times: VecDeque::new(),
                pending: VecDeque::new(),
                resident: Vec::new(),
                resident_fog: None,
                rebuild_buildings: true,
                resource_revision: None,
                resident_players: BTreeMap::new(),
                resident_buildings: BTreeMap::new(),
                resident_animals: Vec::new(),
                resident_projectiles: Vec::new(),
                resident_move_queues: Vec::new(),
                vision_revision: None,
                resource_vision_revision: None,
                resident_resources: Vec::new(),
            },
        );
        let welcome = GameplayServerMessage::Welcome {
            version: GAMEPLAY_VERSION,
            world_id: self.world_id,
            map_content_hash: self.map_content_hash,
            map_metadata: self.map_metadata,
            width_tiles: config.width_tiles,
            height_tiles: config.height_tiles,
            coordinate_precision: FIXED_SUBUNITS_PER_TILE as u16,
            tick_hz: config.tick_hz,
            primary_unit_id: primary,
            resume_token: token,
        };
        Ok((session_id, welcome))
    }
}
