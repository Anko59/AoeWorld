use crate::gameplay_transport;
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, Seed, TileRect, WorldConfig, WorldPosition};
use aoe_protocol::{
    GAMEPLAY_VERSION, GameplayRole, GameplayServerMessage, GameplayUnitState, MAX_ACK_HISTORY,
    MAX_PENDING_COMMANDS_GLOBAL, MAX_PENDING_COMMANDS_PER_CONNECTION, MAX_SUBSCRIBED_UNITS,
    MAX_SUBSCRIPTION_TILES, MapMetadata, ResumeToken,
};
use aoe_simulation::{GameWorld, GameWorldError, NavigationCacheUsage};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, RwLock, mpsc};

#[path = "gameplay/commands.rs"]
mod commands;
#[path = "gameplay/economy.rs"]
mod economy;
#[path = "gameplay/tick.rs"]
mod tick;
use economy::unit_state;
#[path = "gameplay/resources.rs"]
mod resources;
pub use resources::{PersistedDepletion, ResourceLifecycleError};

#[derive(Clone)]
pub struct GameplayService {
    world: Arc<RwLock<GameWorld>>,
    pub(super) sessions: Arc<Mutex<BTreeMap<u64, Session>>>,
    pub(super) ownership: Arc<Mutex<Ownership>>,
    next_session: Arc<AtomicU64>,
    pub(super) world_id: u64,
    map_content_hash: Option<[u8; 32]>,
    map_metadata: Option<MapMetadata>,
    primary_unit_id: EntityId,
    resource_journal: Arc<std::sync::Mutex<resources::Journal>>,
    resource_directory: Option<std::path::PathBuf>,
}

pub(super) struct Session {
    pub(super) role: GameplayRole,
    pub(super) token: Option<ResumeToken>,
    pub(super) connected_at: Instant,
    pub(super) sender: mpsc::Sender<GameplayServerMessage>,
    subscription: Option<Subscription>,
    last_sequence: Option<u64>,
    acknowledgements: VecDeque<u64>,
    command_tokens: f64,
    last_command_refill: Instant,
    subscription_times: VecDeque<Instant>,
    pending: VecDeque<PendingCommand>,
    resident: BTreeMap<EntityId, GameplayUnitState>,
    resource_revision: Option<u64>,
    resident_players: BTreeMap<aoe_core::PlayerId, aoe_protocol::GameplayPlayerState>,
    resident_buildings: BTreeMap<EntityId, aoe_protocol::GameplayBuildingState>,
}

#[derive(Clone, Copy)]
struct Subscription {
    revision: u64,
    region: TileRect,
}

#[derive(Clone, Copy)]
struct PendingCommand {
    sequence: u64,
    action: PendingAction,
}

#[derive(Clone, Copy)]
enum PendingAction {
    Move {
        entity_id: EntityId,
        destination: WorldPosition,
    },
    Gather {
        entity_id: EntityId,
        resource_id: u64,
    },
    CancelTraining {
        building_id: EntityId,
    },
    Train {
        building_id: EntityId,
        kind: aoe_protocol::UnitKind,
    },
}

#[derive(Clone, Copy)]
pub(super) struct ControllerLease {
    pub(super) session_id: u64,
    pub(super) token: ResumeToken,
    pub(super) disconnected_at: Option<Instant>,
}

pub(super) struct Ownership {
    pub(super) controller: Option<ControllerLease>,
    pub(super) next_token: u64,
}

impl GameplayService {
    pub fn new(seed: Seed) -> Self {
        let (world, primary_unit_id) = GameWorld::default_with_village(seed);
        Self::from_world(world, primary_unit_id, None, None)
    }

    pub fn with_population(
        config: WorldConfig,
        count: u32,
        hotspot_count: u32,
        players: u16,
        extent_tiles: i32,
    ) -> Result<Self, GameWorldError> {
        let world = GameWorld::with_population_in_extent(
            config,
            count,
            hotspot_count,
            players,
            extent_tiles,
        )?;
        if !world.unit_exists(EntityId(0)) {
            return Err(GameWorldError::EntityIdExhausted);
        }
        Ok(Self::from_world(world, EntityId(0), None, None))
    }

    pub(super) fn from_world(
        world: GameWorld,
        primary_unit_id: EntityId,
        map_content_hash: Option<[u8; 32]>,
        map_metadata: Option<MapMetadata>,
    ) -> Self {
        let world_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        Self {
            world: Arc::new(RwLock::new(world)),
            sessions: Arc::new(Mutex::new(BTreeMap::new())),
            ownership: Arc::new(Mutex::new(Ownership {
                controller: None,
                next_token: 1,
            })),
            next_session: Arc::new(AtomicU64::new(1)),
            world_id,
            map_content_hash,
            map_metadata,
            primary_unit_id,
            resource_directory: None,
            resource_journal: Arc::new(std::sync::Mutex::new(resources::Journal::default())),
        }
    }

    pub async fn navigation_cache_usage(&self) -> NavigationCacheUsage {
        self.world.read().await.navigation_cache_usage()
    }

    pub async fn register(
        &self,
        requested_token: Option<ResumeToken>,
        sender: mpsc::Sender<GameplayServerMessage>,
    ) -> (u64, GameplayServerMessage) {
        let session_id = self.next_session.fetch_add(1, Ordering::Relaxed);
        let config = self.world.read().await.config();
        let mut ownership = self.ownership.lock().await;
        let mut sessions = self.sessions.lock().await;
        gameplay_transport::promote_expired(
            &mut ownership,
            &mut sessions,
            self.world_id,
            self.primary_unit_id,
        );
        let (role, token) = if let Some(token) = requested_token
            .filter(|token| Some(*token) == ownership.controller.map(|lease| lease.token))
        {
            if let Some(lease) = ownership.controller {
                sessions.remove(&lease.session_id);
            }
            ownership.controller = Some(ControllerLease {
                session_id,
                token,
                disconnected_at: None,
            });
            (GameplayRole::Controller, Some(token))
        } else if ownership.controller.is_none() {
            let token = gameplay_transport::make_token(self.world_id, ownership.next_token);
            ownership.next_token = ownership.next_token.wrapping_add(1);
            ownership.controller = Some(ControllerLease {
                session_id,
                token,
                disconnected_at: None,
            });
            (GameplayRole::Controller, Some(token))
        } else {
            (GameplayRole::Spectator, None)
        };
        sessions.insert(
            session_id,
            Session {
                role,
                token,
                connected_at: Instant::now(),
                sender,
                subscription: None,
                last_sequence: None,
                acknowledgements: VecDeque::new(),
                command_tokens: 20.0,
                last_command_refill: Instant::now(),
                subscription_times: VecDeque::new(),
                pending: VecDeque::new(),
                resident: BTreeMap::new(),
                resource_revision: None,
                resident_players: BTreeMap::new(),
                resident_buildings: BTreeMap::new(),
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
            primary_unit_id: self.primary_unit_id,
            resume_token: token,
        };
        (session_id, welcome)
    }

    pub async fn subscribe(&self, session_id: u64, revision: u64, region: TileRect) {
        let world = self.world.read().await;
        let config = world.config();
        if revision == 0
            || region.width() > MAX_SUBSCRIPTION_TILES
            || region.height() > MAX_SUBSCRIPTION_TILES
            || !region.valid(config.width_tiles, config.height_tiles)
        {
            drop(world);
            self.send_to(
                session_id,
                GameplayServerMessage::Error {
                    code: 400,
                    message: "invalid gameplay subscription region or revision".to_owned(),
                },
            )
            .await;
            return;
        }
        let mut units = world.query(region).0;
        if !units.iter().any(|unit| unit.id == self.primary_unit_id)
            && let Some(primary) = world.unit(self.primary_unit_id)
        {
            units.push(primary);
        }
        if units.len() > MAX_SUBSCRIBED_UNITS {
            drop(world);
            self.send_to(
                session_id,
                GameplayServerMessage::Error {
                    code: 413,
                    message: "subscribed region exceeds gameplay entity limit".to_owned(),
                },
            )
            .await;
            return;
        }
        units.sort_by_key(|unit| unit.id);
        let players = economy::players(&world);
        let buildings = economy::buildings(&world);
        let snapshot = GameplayServerMessage::Snapshot {
            revision,
            tick: world.tick(),
            units: units.iter().map(unit_state).collect(),
            players: players.values().copied().collect(),
            buildings: buildings.values().cloned().collect(),
        };
        drop(world);
        let mut sessions = self.sessions.lock().await;
        let mut failed = false;
        if let Some(session) = sessions.get_mut(&session_id) {
            let now = Instant::now();
            while session
                .subscription_times
                .front()
                .is_some_and(|time| now.duration_since(*time) >= Duration::from_secs(1))
            {
                session.subscription_times.pop_front();
            }
            if session.subscription_times.len() >= 10 {
                let _ = session.sender.try_send(GameplayServerMessage::Error {
                    code: 429,
                    message: "subscription updates are rate limited".to_owned(),
                });
                return;
            }
            if session
                .subscription
                .is_some_and(|old| revision <= old.revision)
            {
                let _ = session.sender.try_send(GameplayServerMessage::Error {
                    code: 409,
                    message: "subscription revision is obsolete".to_owned(),
                });
                return;
            }
            session.subscription_times.push_back(now);
            session.subscription = Some(Subscription { revision, region });
            session.resource_revision = None;
            session.resident = units
                .iter()
                .map(unit_state)
                .map(|unit| (unit.id, unit))
                .collect();
            session.resident_players = players;
            session.resident_buildings = buildings;
            failed = session.sender.try_send(snapshot).is_err();
        }
        drop(sessions);
        if failed {
            self.disconnect(session_id).await;
        }
    }

    pub async fn resync(&self, session_id: u64, revision: u64) {
        let region = self
            .sessions
            .lock()
            .await
            .get(&session_id)
            .and_then(|session| {
                session
                    .subscription
                    .map(|subscription| (subscription.revision, subscription.region))
            });
        if let Some((current_revision, region)) = region {
            self.subscribe(session_id, revision.max(current_revision + 1), region)
                .await;
        } else {
            self.send_to(
                session_id,
                GameplayServerMessage::Error {
                    code: 400,
                    message: "subscribe before resync".to_owned(),
                },
            )
            .await;
        }
    }
}

fn total_pending(sessions: &BTreeMap<u64, Session>) -> usize {
    sessions.values().map(|session| session.pending.len()).sum()
}
