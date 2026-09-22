use crate::gameplay_transport;
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, Seed, TileRect, WorldConfig, WorldPosition};
use aoe_protocol::{
    CommandResult, GAMEPLAY_VERSION, GameplayRole, GameplayServerMessage, GameplayUnitState,
    MAX_ACK_HISTORY, MAX_PENDING_COMMANDS_GLOBAL, MAX_PENDING_COMMANDS_PER_CONNECTION,
    MAX_SUBSCRIBED_UNITS, MAX_SUBSCRIPTION_TILES, MapMetadata, ResumeToken,
};
use aoe_simulation::{GameUnit, GameWorld, GameWorldError, NavigationCacheUsage};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, RwLock, mpsc};

#[path = "gameplay_commands.rs"]
mod commands;
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
}

#[derive(Clone, Copy)]
struct Subscription {
    revision: u64,
    region: TileRect,
}

#[derive(Clone, Copy)]
struct PendingCommand {
    sequence: u64,
    entity_id: EntityId,
    destination: WorldPosition,
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
        let (world, primary_unit_id) = GameWorld::default_with_cavalry(seed);
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
        }
    }

    pub async fn tick(&self) {
        let (pending, controllers) = {
            let mut ownership = self.ownership.lock().await;
            let mut sessions = self.sessions.lock().await;
            gameplay_transport::promote_expired(
                &mut ownership,
                &mut sessions,
                self.world_id,
                self.primary_unit_id,
            );
            let mut commands = Vec::new();
            let controllers = sessions
                .iter()
                .filter_map(|(id, session)| {
                    (session.role == GameplayRole::Controller).then_some(*id)
                })
                .collect::<std::collections::BTreeSet<_>>();
            for (session_id, session) in sessions.iter_mut() {
                while let Some(command) = session.pending.pop_front() {
                    commands.push((*session_id, command));
                }
            }
            commands.sort_by_key(|(session_id, command)| (*session_id, command.sequence));
            (commands, controllers)
        };
        let subscriptions = self
            .sessions
            .lock()
            .await
            .iter()
            .filter_map(|(id, session)| {
                session.subscription.map(|subscription| (*id, subscription))
            })
            .collect::<Vec<_>>();
        let mut world = self.world.write().await;
        let mut acknowledgements = Vec::new();
        for (session_id, command) in pending {
            let accepted = controllers.contains(&session_id);
            let result = if !accepted {
                CommandResult::RejectedNotController
            } else if !world.unit_exists(command.entity_id) {
                CommandResult::RejectedUnknownEntity
            } else if !world.config().valid_map_position(command.destination) {
                CommandResult::RejectedInvalidDestination
            } else {
                let destination = world.config().snap_ground_position(command.destination);
                match world.issue_move(command.entity_id, destination) {
                    Ok(_) => CommandResult::Accepted,
                    Err(GameWorldError::Unreachable) => CommandResult::RejectedUnreachable,
                    Err(GameWorldError::PathBudgetExceeded) => {
                        CommandResult::RejectedPathBudgetExceeded
                    }
                    Err(_) => CommandResult::RejectedInvalidDestination,
                }
            };
            let tick = world.tick();
            acknowledgements.push((
                session_id,
                GameplayServerMessage::CommandAck {
                    sequence: command.sequence,
                    result,
                    applied_tick: tick,
                },
            ));
        }
        world.advance();
        let tick = world.tick();
        let mut updates = Vec::new();
        for (session_id, subscription) in subscriptions {
            let mut current = world.query(subscription.region).0;
            if !current.iter().any(|unit| unit.id == self.primary_unit_id)
                && let Some(primary) = world.unit(self.primary_unit_id)
            {
                current.push(primary);
            }
            if current.len() > MAX_SUBSCRIBED_UNITS {
                updates.push((
                    session_id,
                    GameplayServerMessage::Error {
                        code: 413,
                        message: "subscribed region exceeds gameplay entity limit".to_owned(),
                    },
                    BTreeMap::new(),
                ));
                continue;
            }
            current.sort_by_key(|unit| unit.id);
            let current = current
                .iter()
                .map(|unit| (unit.id, unit_state(unit)))
                .collect::<BTreeMap<_, _>>();
            updates.push((
                session_id,
                GameplayServerMessage::Tick {
                    revision: subscription.revision,
                    tick,
                    changed_units: current.values().copied().collect(),
                    removals: Vec::new(),
                },
                current,
            ));
        }
        drop(world);
        let mut sessions = self.sessions.lock().await;
        for (session_id, message) in acknowledgements {
            if let Some(session) = sessions.get_mut(&session_id) {
                if let GameplayServerMessage::CommandAck { sequence, .. } = message {
                    session.acknowledgements.push_back(sequence);
                    while session.acknowledgements.len() > MAX_ACK_HISTORY {
                        session.acknowledgements.pop_front();
                    }
                }
                let _ = session.sender.try_send(message);
            }
        }
        for (session_id, mut message, current) in updates {
            let Some(session) = sessions.get_mut(&session_id) else {
                continue;
            };
            if let GameplayServerMessage::Tick {
                changed_units,
                removals,
                ..
            } = &mut message
            {
                let changed = current
                    .iter()
                    .filter_map(|(id, unit)| {
                        (session.resident.get(id) != Some(unit)).then_some(*unit)
                    })
                    .collect::<Vec<_>>();
                let removed = session
                    .resident
                    .keys()
                    .filter(|id| !current.contains_key(id))
                    .copied()
                    .collect::<Vec<_>>();
                session.resident = current;
                *changed_units = changed;
                *removals = removed;
            }
            let _ = session.sender.try_send(message);
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
        let snapshot = GameplayServerMessage::Snapshot {
            revision,
            tick: world.tick(),
            units: units.iter().map(unit_state).collect(),
        };
        drop(world);
        let mut sessions = self.sessions.lock().await;
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
            session.resident = units
                .iter()
                .map(unit_state)
                .map(|unit| (unit.id, unit))
                .collect();
            let _ = session.sender.try_send(snapshot);
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

fn unit_state(unit: &GameUnit) -> GameplayUnitState {
    GameplayUnitState {
        id: unit.id,
        player: unit.player,
        position: unit.position,
        moving: unit.moving,
        planning: unit.planning,
        facing: unit.facing as u8,
    }
}

fn total_pending(sessions: &BTreeMap<u64, Session>) -> usize {
    sessions.values().map(|session| session.pending.len()).sum()
}
