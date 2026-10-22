use aoe_core::{EntityId, PlayerId, Seed, TileRect, WorldConfig, WorldPosition};
use aoe_protocol::{
    GameplayRole, GameplayServerMessage, GameplayUnitState, MAX_ACK_HISTORY,
    MAX_PENDING_COMMANDS_GLOBAL, MAX_PENDING_COMMANDS_PER_CONNECTION, MAX_SUBSCRIBED_UNITS,
    MAX_SUBSCRIPTION_TILES, MapMetadata, ResumeToken,
};
use aoe_simulation::{GameWorld, GameWorldError, NavigationCacheUsage};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, atomic::AtomicU64},
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, RwLock, mpsc};

#[path = "gameplay/commands.rs"]
mod commands;
#[path = "gameplay/deliver.rs"]
mod deliver;
#[path = "gameplay/economy.rs"]
mod economy;
#[path = "gameplay/fanout.rs"]
mod fanout;
#[path = "gameplay/fog.rs"]
mod fog;
#[path = "gameplay/frame.rs"]
mod frame;
#[path = "gameplay/probe.rs"]
mod probe;
#[path = "gameplay/slots.rs"]
mod slots;
#[path = "gameplay/tick.rs"]
mod tick;
pub(crate) use slots::{Access, ControllerLease, Ownership};
pub use slots::{SlotInfo, SlotSetup};
#[path = "gameplay/wildlife.rs"]
mod wildlife;
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
    fanout_probe: Arc<probe::FanoutProbe>,
}

pub(super) struct Session {
    pub(super) role: GameplayRole,
    pub(super) token: Option<ResumeToken>,
    /// Index of the match slot this session controls; spectators have none.
    pub(super) slot: Option<usize>,
    /// Whose fog-filtered view this session receives: its slot's player, or
    /// the first slot's player for a spectator.
    pub(super) viewer: PlayerId,
    /// The slot's primary unit (the Welcome identity), kept for view queries.
    pub(super) primary_unit_id: EntityId,
    /// AI slots get a higher order budget and quiet idle ticks.
    pub(super) ai: bool,
    pub(super) connected_at: Instant,
    pub(super) sender: mpsc::Sender<GameplayServerMessage>,
    subscription: Option<Subscription>,
    last_sequence: Option<u64>,
    acknowledgements: VecDeque<u64>,
    command_tokens: f64,
    last_command_refill: Instant,
    subscription_times: VecDeque<Instant>,
    pending: VecDeque<PendingCommand>,
    /// Units the client holds, sorted by id.
    resident: Vec<GameplayUnitState>,
    resource_revision: Option<u64>,
    resident_players: BTreeMap<aoe_core::PlayerId, aoe_protocol::GameplayPlayerState>,
    resident_buildings: BTreeMap<EntityId, aoe_protocol::GameplayBuildingState>,
    resident_animals: Vec<aoe_protocol::AnimalState>,
    resident_projectiles: Vec<aoe_protocol::ProjectileState>,
    vision_revision: Option<u64>,
    /// The fog view the client holds; an unchanged rebuild is not resent.
    resident_fog: Option<aoe_protocol::FogState>,
    /// Set when the building baseline can no longer be advanced by change
    /// lists alone (a new point of view, fog switched).
    rebuild_buildings: bool,
    resource_vision_revision: Option<u64>,
    resident_resources: Vec<aoe_protocol::ResourceAmount>,
}

#[derive(Clone, Copy)]
struct Subscription {
    revision: u64,
    region: TileRect,
}

#[derive(Clone)]
struct PendingCommand {
    sequence: u64,
    action: PendingAction,
}

#[derive(Clone)]
enum PendingAction {
    Stop {
        entity_id: EntityId,
    },
    Hunt {
        unit_ids: Vec<EntityId>,
        animal_id: EntityId,
    },
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
    Attack {
        entity_id: EntityId,
        target_id: EntityId,
    },
    Train {
        building_id: EntityId,
        kind: aoe_protocol::UnitKind,
    },
    Build {
        builder_ids: Vec<EntityId>,
        kind: aoe_protocol::BuildingKind,
        origin: aoe_core::TileCoord,
    },
    BuildWall {
        builder_ids: Vec<EntityId>,
        from: aoe_core::TileCoord,
        to: aoe_core::TileCoord,
    },
    Construct {
        builder_ids: Vec<EntityId>,
        building_id: EntityId,
    },
    CancelConstruction {
        building_id: EntityId,
    },
    Farm {
        entity_id: EntityId,
        farm_id: EntityId,
    },
    Research {
        building_id: EntityId,
        tech: aoe_protocol::Tech,
    },
    CancelResearch {
        building_id: EntityId,
    },
    MarketBuy {
        building_id: EntityId,
        resource: aoe_protocol::TradeResource,
    },
    MarketSell {
        building_id: EntityId,
        resource: aoe_protocol::TradeResource,
    },
}

impl GameplayService {
    pub fn military_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::military_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    /// Server-side scenario setup, never a gameplay client command.
    pub async fn set_fog_disabled(&self, disabled: bool) {
        self.world.write().await.set_fog_disabled(disabled);
        for session in self.sessions.lock().await.values_mut() {
            session.rebuild_buildings = true;
        }
    }
    /// Local Feudal showcase setup; the ordinary command and wire paths apply.
    pub fn feudal_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::feudal_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
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
        let mut world = GameWorld::with_population_in_extent(
            config,
            count,
            hotspot_count,
            players,
            extent_tiles,
        )?;
        // Synthetic population workloads explicitly observe the whole world.
        world.set_fog_disabled(true);
        if !world.unit_exists(EntityId(0)) {
            return Err(GameWorldError::EntityIdExhausted);
        }
        Ok(Self::from_world(world, EntityId(0), None, None))
    }

    /// A prebuilt world for headless scale measurements; it has no map identity.
    pub fn from_scale_world(world: GameWorld, primary_unit_id: EntityId) -> Self {
        Self::from_world(world, primary_unit_id, None, None)
    }

    /// A prebuilt multi-player world with one seat per entry, for headless
    /// scale measurements of the per-slot fan-out.
    /// Each seat is `(player, primary unit, is a person)`.
    pub fn from_scale_world_slots(
        world: GameWorld,
        seats: impl IntoIterator<Item = (PlayerId, EntityId, bool)>,
    ) -> Self {
        let setups = seats
            .into_iter()
            .map(|(player, primary_unit_id, human)| SlotSetup {
                player,
                primary_unit_id,
                human,
            })
            .collect();
        Self::from_world_slots(world, setups, None, None)
    }

    pub async fn navigation_cache_usage(&self) -> NavigationCacheUsage {
        self.world.read().await.navigation_cache_usage()
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
        // Admission precedes all view construction. Keep the session lock until
        // publication so concurrent requests cannot pass the same revision.
        let mut sessions = self.sessions.lock().await;
        let Some(session) = sessions.get_mut(&session_id) else {
            return;
        };
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
        // At most ten admitted views per second, each at most 512x512 tiles
        // plus the fixed 256x256 overview, regardless of total map area.
        session.subscription_times.push_back(now);
        let viewer = session.viewer;
        let units = economy::query_units(&world, region, session.primary_unit_id, viewer);
        if units.len() > MAX_SUBSCRIBED_UNITS {
            let _ = session.sender.try_send(GameplayServerMessage::Error {
                code: 413,
                message: "subscribed region exceeds gameplay entity limit".to_owned(),
            });
            return;
        }
        let player = viewer;
        let players = economy::players(&world)
            .into_iter()
            .filter(|(id, _)| *id == player || world.fog_disabled())
            .collect::<BTreeMap<_, _>>();
        let buildings = economy::buildings(&world, player);
        let units = units
            .iter()
            .map(|unit| unit_state(&world, unit))
            .collect::<Vec<_>>();
        let snapshot = GameplayServerMessage::Snapshot {
            revision,
            tick: world.tick(),
            units: units.clone(),
            players: players.values().copied().collect(),
            buildings: buildings.values().cloned().collect(),
        };
        let animals = wildlife::animals(&world, region, player);
        let projectiles = wildlife::projectiles(&world, region, player);
        let projectile_message = GameplayServerMessage::Projectiles {
            revision,
            tick: world.tick(),
            projectiles: projectiles.clone(),
        };
        let fog_state = fog::state(&world, player, revision, region);
        session.resident_fog = Some(fog_state.clone());
        session.rebuild_buildings = false;
        let fog = GameplayServerMessage::FogState(fog_state);
        let vision_revision = world.player_vision_revision(player);
        let wildlife = GameplayServerMessage::WildlifeState {
            revision,
            tick: world.tick(),
            animals: animals.clone(),
        };
        session.subscription = Some(Subscription { revision, region });
        session.resource_revision = None;
        session.resident = units.clone();
        session.resident_players = players;
        session.resident_buildings = buildings;
        session.resident_animals = animals;
        session.resident_projectiles = projectiles;
        session.vision_revision = Some(vision_revision);
        let failed = session.sender.try_send(snapshot).is_err()
            || session.sender.try_send(wildlife).is_err()
            || session.sender.try_send(fog).is_err()
            || session.sender.try_send(projectile_message).is_err();
        drop(sessions);
        drop(world);
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
