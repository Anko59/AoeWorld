use aoe_core::{EntityId, PlayerId, Seed, TileRect, WorldConfig, WorldPosition};
use aoe_protocol::{
    GameplayServerMessage, MAX_ACK_HISTORY, MAX_PENDING_COMMANDS_GLOBAL,
    MAX_PENDING_COMMANDS_PER_CONNECTION, MAX_SUBSCRIBED_UNITS, MAX_SUBSCRIPTION_TILES, MapMetadata,
};
use aoe_simulation::{GameWorld, GameWorldError, NavigationCacheUsage};
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicU64},
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, RwLock};
#[path = "gameplay/arena.rs"]
pub(crate) mod arena;
#[path = "gameplay/commands.rs"]
mod commands;
pub(crate) use commands::SelectionMove;
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
#[path = "gameplay/preparation.rs"]
mod preparation;
#[path = "gameplay/probe.rs"]
pub(crate) mod probe;
pub mod replay;
#[path = "gameplay/scenarios/mod.rs"]
mod scenarios;
#[path = "gameplay/slots.rs"]
mod slots;
#[path = "gameplay/tick.rs"]
mod tick;
pub(crate) use slots::{Access, Slot};
pub use slots::{SlotInfo, SlotSetup};
#[path = "gameplay/wildlife.rs"]
mod wildlife;
use commands::queue::{PendingCommand, QueuedCommand};
pub(crate) use deliver::session::{Session, Subscription};
use economy::unit_state;
#[path = "gameplay/resources.rs"]
mod resources;
pub use resources::{PersistedDepletion, ResourceLifecycleError};
#[derive(Clone)]
pub struct GameplayService {
    pub(super) world: Arc<RwLock<GameWorld>>,
    pub(super) sessions: Arc<Mutex<BTreeMap<u64, Session>>>,
    pub(super) slots: Arc<[Slot]>,
    next_session: Arc<AtomicU64>,
    pub(super) world_id: u64,
    map_content_hash: Option<[u8; 32]>,
    map_metadata: Option<MapMetadata>,
    resource_journal: Arc<std::sync::Mutex<resources::Journal>>,
    resource_directory: Option<std::path::PathBuf>,
    fanout_probe: Arc<probe::FanoutProbe>,
    overlap: Arc<tick::Overlap>,
    preparation: preparation::Preparation,
    /// Set by the person's Pause: `run_ticks` stops stepping this match.
    pub(super) paused: Arc<std::sync::atomic::AtomicBool>,
    /// What the last tick spent where; read by `run_ticks`.
    probe: Arc<crate::gameplay_match::TickProbe>,
    recording: Arc<std::sync::Mutex<Option<crate::replay::Recorder>>>,
    /// Each human session's latest admitted view: an input of the next tick.
    pub(super) human_views: Arc<std::sync::Mutex<BTreeMap<u64, aoe_simulation::HumanView>>>,
    /// Rust AI seats played inside the server (`AOE_AI_RUNTIME=rust`).
    pub(super) ai_seats: Arc<crate::gameplay_match::AiSeats>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
enum PendingAction {
    VassalDevelopment {
        vassal_id: EntityId,
        enabled: bool,
    },
    VassalFocus {
        vassal_id: EntityId,
        focus: aoe_protocol::VassalFocus,
    },
    Stance {
        unit_ids: Vec<EntityId>,
        stance: aoe_core::CombatStance,
    },
    Garrison {
        entity_id: EntityId,
        building_id: EntityId,
    },
    BackToWork(EntityId),
    TownBell(EntityId),
    Ungarrison {
        building_id: EntityId,
        entity_id: Option<EntityId>,
    },
    Trade(EntityId, EntityId),
    Load {
        unit_ids: Vec<EntityId>,
        transport_id: EntityId,
    },
    Unload {
        transport_id: EntityId,
        destination: aoe_core::TileCoord,
    },
    Formation {
        unit_ids: Vec<EntityId>,
        formation: aoe_core::Formation,
        destination: Option<WorldPosition>,
        queued: bool,
        attack_move: bool,
    },
    /// One selection order: the same order for each unit, acknowledged once.
    Each(Vec<PendingAction>),
    AutoScout {
        entity_id: EntityId,
        enabled: bool,
    },
    FarmReseed {
        building_id: EntityId,
        add: bool,
    },
    StewardDismiss {
        steward_id: EntityId,
    },
    StewardSettings {
        steward_id: EntityId,
        settings: aoe_simulation::StewardSettings,
    },
    SetRally {
        building_id: EntityId,
        target: aoe_protocol::RallyTarget,
    },
    Stop {
        entity_id: EntityId,
    },
    Delete {
        entity_ids: Vec<EntityId>,
    },
    GeneralMoveAlone {
        entity_id: EntityId,
        destination: WorldPosition,
    },
    GeneralTraining {
        entity_id: EntityId,
        enabled: bool,
    },
    GeneralMix {
        entity_id: EntityId,
        mix: [u8; 5],
    },
    Hunt {
        unit_ids: Vec<EntityId>,
        animal_id: EntityId,
    },
    Move {
        entity_id: EntityId,
        destination: WorldPosition,
        queued: bool,
        attack_move: bool,
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
        queued: bool,
    },
    BuildBridge {
        builder_ids: Vec<EntityId>,
        from: aoe_core::TileCoord,
        to: aoe_core::TileCoord,
        queued: bool,
    },
    BuildWall {
        builder_ids: Vec<EntityId>,
        kind: aoe_protocol::BuildingKind,
        from: aoe_core::TileCoord,
        to: aoe_core::TileCoord,
        queued: bool,
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
    /// Single trades are one lot; bulk orders carry up to `MARKET_MAX_LOTS`.
    MarketTrade {
        building_id: EntityId,
        resource: aoe_protocol::TradeResource,
        buy: bool,
        lots: u32,
    },
    Patrol(EntityId, WorldPosition),
    MoveSelection(SelectionMove),
    AttackSelection {
        entity_ids: Vec<EntityId>,
        target_id: EntityId,
    },
    TrainBatch {
        building_ids: Vec<EntityId>,
        kind: aoe_protocol::UnitKind,
    },
    StopSelection(Vec<EntityId>),
}
impl GameplayService {
    /// Server-side scenario setup, never a gameplay client command.
    pub async fn set_fog_disabled(&self, disabled: bool) {
        self.world.write().await.set_fog_disabled(disabled);
        for session in self.sessions.lock().await.values_mut() {
            session.rebuild_buildings = true;
            session.vision_revision = None;
        }
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

    /// Units alive in the world, and what the last tick spent where.
    pub(crate) async fn tick_and_units(&self) -> (u64, usize) {
        let world = self.world.read().await;
        (world.tick().0, world.unit_count())
    }
    pub(crate) fn probe(&self) -> Arc<crate::gameplay_match::TickProbe> {
        self.probe.clone()
    }

    pub async fn navigation_cache_usage(&self) -> NavigationCacheUsage {
        self.world.read().await.navigation_cache_usage()
    }

    pub async fn subscribe(
        &self,
        session_id: u64,
        revision: u64,
        region: TileRect,
        unit_zoom: bool,
    ) {
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
        // An in-process seat resubscribes on game time; a wall-clock limit would
        // make its view, and its decisions, depend on how fast the arena steps.
        let in_process = session.command_tick.is_some();
        if !in_process && session.subscription_times.len() >= 10 {
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
        (!in_process).then(|| session.subscription_times.push_back(now));
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
        let players = economy::player(&world, player)
            .into_iter()
            .map(|state| (state.player, state))
            .collect::<BTreeMap<_, _>>();
        let camera = (unit_zoom || session.ai).then_some(region);
        let all_buildings = economy::subscription_buildings(&world, player, camera);
        let more_buildings = all_buildings.len() > aoe_protocol::MAX_ECONOMY_BUILDINGS;
        let buildings = all_buildings
            .into_iter()
            .take(aoe_protocol::MAX_ECONOMY_BUILDINGS)
            .collect::<BTreeMap<_, _>>();
        let units = units
            .iter()
            .map(|unit| unit_state(&world, unit).for_viewer(player))
            .collect::<Vec<_>>();
        let snapshot = GameplayServerMessage::Snapshot {
            revision,
            tick: world.tick(),
            units: units.clone(),
            players: players.values().cloned().collect(),
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
        session.rebuild_buildings = more_buildings;
        let fog = GameplayServerMessage::FogState(fog_state);
        let vision_revision = world.player_vision_revision(player);
        let wildlife = GameplayServerMessage::WildlifeState {
            revision,
            tick: world.tick(),
            animals: animals.clone(),
        };
        session.subscription = Some(Subscription {
            revision,
            region,
            unit_zoom,
        });
        if !session.ai {
            let view = aoe_simulation::HumanView {
                player: viewer,
                region,
                unit_zoom,
            };
            self.human_views
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(session_id, view);
        }
        session.resource_revision = None;
        session.resident = units.clone().into();
        session.resident_players = players;
        session.resident_buildings = buildings;
        session.resident_animals = animals;
        session.resident_projectiles = projectiles;
        session.resident_move_queues =
            fanout::move_queues(&world, player, units.iter().map(|u| u.id));
        session.vision_revision = Some(vision_revision);
        let land = world.land_control(player);
        let holding_message = economy::holdings::message(&land, revision, world.tick());
        session.resident_land = Some(land);
        let mut initial = vec![snapshot, wildlife, fog, projectile_message, holding_message];
        if !session.resident_move_queues.is_empty() {
            initial.push(GameplayServerMessage::MoveQueues {
                revision,
                tick: world.tick(),
                queues: session.resident_move_queues.clone(),
            });
        }
        let zones = session.office_zones.initial(&world, player, session.ai);
        let failed = !fanout::subscription::Delivery::offer(
            &mut session.subscription_delivery,
            &session.sender,
            Box::new(initial.into_iter().chain(zones)),
        );
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
                session.subscription.map(|subscription| {
                    (
                        subscription.revision,
                        subscription.region,
                        subscription.unit_zoom,
                    )
                })
            });
        if let Some((current_revision, region, unit_zoom)) = region {
            self.subscribe(
                session_id,
                revision.max(current_revision + 1),
                region,
                unit_zoom,
            )
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
