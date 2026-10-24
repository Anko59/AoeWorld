//! Match seats: who may control which player, how a connection claims one, and
//! how a service is assembled with several of them.
use super::{GameplayService, Session, fog, resources};
use crate::gameplay_transport;
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId};
use aoe_protocol::{
    GAMEPLAY_VERSION, GameplayRole, GameplayServerMessage, MapMetadata, ResumeToken,
};
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

#[derive(Clone, Copy)]
pub(crate) struct ControllerLease {
    pub(crate) session_id: u64,
    pub(crate) disconnected_at: Option<Instant>,
}

/// One seat of a match. Every slot owns exactly one player and is reachable
/// only through its token; a human slot may also be claimed by a plain Hello.
pub(crate) struct Slot {
    pub(crate) player: PlayerId,
    pub(crate) primary_unit_id: EntityId,
    pub(crate) human: bool,
    pub(crate) token: ResumeToken,
    pub(crate) lease: Option<ControllerLease>,
}

pub(crate) struct Ownership {
    pub(crate) slots: Vec<Slot>,
    pub(crate) next_token: u64,
}

/// A `JoinSlot` named a token no slot of this match owns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnknownSlotToken;

/// What a connection asked for in its first message.
pub(crate) enum Access {
    /// Legacy `Hello`: resume with a token, or take the first free human slot.
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

impl Session {
    /// A different point of view: rebuild every delta baseline.
    pub(crate) fn change_viewer(&mut self, viewer: PlayerId) {
        self.viewer = viewer;
        self.resident.clear();
        self.resident_fog = None;
        self.rebuild_buildings = true;
        self.resident_players.clear();
        self.resident_buildings.clear();
        self.resident_animals.clear();
        self.resident_projectiles.clear();
        self.resident_resources.clear();
        self.vision_revision = None;
        self.resource_revision = None;
        self.resource_vision_revision = None;
    }
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

    /// Hosts one match seat per entry; the first slot is also the spectators'
    /// point of view. Tokens are issued here and never derived from a counter
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
        let mut next_token = 1;
        let slots = setups
            .into_iter()
            .map(|setup| {
                let token = gameplay_transport::make_token(world_id, next_token);
                next_token += 1;
                Slot {
                    player: setup.player,
                    primary_unit_id: setup.primary_unit_id,
                    human: setup.human,
                    token,
                    lease: None,
                }
            })
            .collect();
        let probe = Arc::new(crate::gameplay_match::TickProbe::default());
        world.set_planning_probe(probe.clone());
        if let aoe_simulation::Terrain::Map { generator, .. } = world.terrain() {
            generator.set_navigation_probe(probe.clone());
        }
        Self {
            world: Arc::new(RwLock::new(world)),
            sessions: Arc::new(Mutex::new(BTreeMap::new())),
            ownership: Arc::new(Mutex::new(Ownership { slots, next_token })),
            next_session: Arc::new(AtomicU64::new(1)),
            world_id,
            map_content_hash,
            map_metadata,
            primary_unit_id,
            resource_directory: None,
            probe,
            resource_journal: Arc::new(std::sync::Mutex::new(resources::Journal::default())),
            fanout_probe: Arc::default(),
        }
    }

    /// The seats of this service with their tokens, for the match creator.
    pub async fn slot_infos(&self) -> Vec<SlotInfo> {
        self.ownership
            .lock()
            .await
            .slots
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
            .unwrap_or_else(|_| unreachable!("a hello never fails"))
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
        let mut ownership = self.ownership.lock().await;
        let mut sessions = self.sessions.lock().await;
        gameplay_transport::promote_expired(&mut ownership, &mut sessions, self.world_id);
        let by_token = |token: ResumeToken| ownership.slots.iter().position(|s| s.token == token);
        let slot_index = match access {
            Access::Join(token) => Some(by_token(token).ok_or(UnknownSlotToken)?),
            Access::Hello(Some(token)) => by_token(token),
            Access::Hello(None) => None,
        }
        .or_else(|| match access {
            Access::Hello(_) => ownership
                .slots
                .iter()
                .position(|slot| slot.human && slot.lease.is_none()),
            Access::Join(_) => None,
        });
        let session_id = self.next_session.fetch_add(1, Ordering::Relaxed);
        let (role, token, viewer, primary, ai) = if let Some(index) = slot_index {
            let slot = &mut ownership.slots[index];
            if let Some(lease) = slot.lease.replace(ControllerLease {
                session_id,
                disconnected_at: None,
            }) {
                sessions.remove(&lease.session_id);
            }
            (
                GameplayRole::Controller,
                Some(slot.token),
                slot.player,
                slot.primary_unit_id,
                !slot.human,
            )
        } else {
            let first = ownership.slots.first();
            (
                GameplayRole::Spectator,
                None,
                first.map_or(PlayerId(0), |slot| slot.player),
                first.map_or(self.primary_unit_id, |slot| slot.primary_unit_id),
                false,
            )
        };
        sessions.insert(
            session_id,
            Session {
                role,
                token,
                slot: slot_index,
                viewer,
                primary_unit_id: primary,
                ai,
                connected_at: Instant::now(),
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
            role,
            primary_unit_id: primary,
            resume_token: token,
        };
        Ok((session_id, welcome))
    }
}
