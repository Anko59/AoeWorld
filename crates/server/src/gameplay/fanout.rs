//! Building every session's view of one tick from a single read of the world.
//! Nothing here touches session state, so it runs on worker threads while the
//! world is only read; `deliver` then diffs and sends under the session lock.
use super::Subscription;
use super::economy::{self, wire_building_state};
use super::frame::{Lens, Scratch, UnitFrame};
use aoe_core::{EntityId, PlayerId, Tick};
use aoe_protocol::{
    AnimalState, FogState, GameplayBuildingState, GameplayPlayerState, ProjectileState,
};
use aoe_simulation::GameWorld;
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Idle AI sessions still hear the clock this often (in ticks).
pub(super) const AI_HEARTBEAT_TICKS: u64 = 10;
/// An AI's fog view is rebuilt at most this often (in ticks).
pub(super) const AI_FOG_TICKS: u64 = 20;

/// A subscribed session's inputs to one tick, copied out of the session lock.
pub(super) struct View {
    pub session_id: u64,
    pub subscription: Subscription,
    pub vision: Option<u64>,
    pub fog_disabled: Option<bool>,
    pub viewer: PlayerId,
    pub primary: EntityId,
    pub ai: bool,
    /// The session's building baseline cannot be advanced by change lists.
    pub rebuild: bool,
}

/// How a viewer's known buildings differ from last tick.
pub(super) enum Buildings {
    /// Every known building; the diff is against the whole baseline.
    Full(Arc<BTreeMap<EntityId, GameplayBuildingState>>),
    /// Only buildings whose memory changed (`None` once forgotten), by id.
    Changes(Vec<(EntityId, Option<GameplayBuildingState>)>),
}

/// Everything one player's sessions need, built once per tick however many
/// sessions look through that player.
pub(super) struct ViewerState {
    pub players: BTreeMap<PlayerId, GameplayPlayerState>,
    pub buildings: Buildings,
    pub vision: u64,
    pub land: Option<Arc<aoe_simulation::LandControl>>,
}

pub(super) struct SessionBuild {
    pub session_id: u64,
    pub revision: u64,
    pub viewer: PlayerId,
    /// Slots of the units in view, by id; `None` when over the entity limit.
    pub units: Option<Vec<u32>>,
    pub fog: Option<(FogState, u64)>,
    pub animals: Vec<AnimalState>,
    pub projectiles: Vec<ProjectileState>,
    pub move_queues: Vec<aoe_protocol::MoveQueue>,
}

pub(super) struct Built {
    #[cfg(test)]
    workers: Vec<std::thread::ThreadId>,
    pub tick: Tick,
    pub fog_disabled: bool,
    pub frame: Arc<UnitFrame>,
    pub viewers: BTreeMap<PlayerId, ViewerState>,
    pub sessions: Vec<SessionBuild>,
    /// Time spent on viewers, the frame and the sessions.
    pub phases: [std::time::Duration; 3],
}

fn viewer_states(
    world: &GameWorld,
    views: &[View],
    changes: &BTreeMap<PlayerId, BTreeSet<EntityId>>,
) -> BTreeMap<PlayerId, ViewerState> {
    let mut rebuild = BTreeMap::<PlayerId, bool>::new();
    for view in views {
        *rebuild.entry(view.viewer).or_default() |=
            view.rebuild || view.fog_disabled != Some(world.fog_disabled());
    }
    let disabled = world.fog_disabled();
    // Shared by every viewer when fog is disabled, so built once up front.
    let first = rebuild.keys().next().filter(|_| disabled);
    let everything = first.map(|first| Arc::new(economy::buildings(world, *first)));
    let land_tick = world.tick().0.is_multiple_of(20);
    let rebuild = rebuild.into_iter().collect::<Vec<_>>();
    // Each viewer's state reads only the world, so viewers are built in
    // parallel: once a second every viewer's land report is rastered.
    rebuild
        .par_iter()
        .with_min_len(VIEWERS_PER_TASK)
        .map(|&(viewer, rebuild)| {
            let state = viewer_state(
                world,
                viewer,
                rebuild,
                changes,
                everything.as_ref(),
                land_tick,
            );
            (viewer, state)
        })
        .collect::<Vec<_>>()
        .into_iter()
        .collect()
}

/// Viewers one worker task builds at least; a tick usually has few rebuilds,
/// so small tasks would cost more to schedule than to run.
const VIEWERS_PER_TASK: usize = 4;
/// Sessions one worker task builds at least.
const SESSIONS_PER_TASK: usize = 2;

fn viewer_state(
    world: &GameWorld,
    viewer: PlayerId,
    rebuild: bool,
    changes: &BTreeMap<PlayerId, BTreeSet<EntityId>>,
    everything: Option<&Arc<BTreeMap<EntityId, GameplayBuildingState>>>,
    land_tick: bool,
) -> ViewerState {
    let players = economy::player(world, viewer)
        .map(|state| (state.player, state))
        .into_iter()
        .collect();
    let buildings = if let Some(all) = everything {
        // With fog disabled all targets are revealed as well. Keep one
        // shared map even when a hundred seats have set their rally points.
        Buildings::Full(all.clone())
    } else if rebuild {
        Buildings::Full(Arc::new(economy::buildings(world, viewer)))
    } else {
        Buildings::Changes(
            changes
                .get(&viewer)
                .into_iter()
                .flatten()
                .map(|id| {
                    let known = world.remembered_building(viewer, *id);
                    (*id, known.map(|b| wire_building_state(b, Some(viewer))))
                })
                .collect(),
        )
    };
    ViewerState {
        players,
        land: (land_tick || rebuild).then(|| world.land_control(viewer)),
        buildings,
        vision: world.player_vision_revision(viewer),
    }
}

fn build_session(
    world: &GameWorld,
    frame: &UnitFrame,
    viewers: &BTreeMap<PlayerId, ViewerState>,
    view: &View,
    scratch: &mut Scratch,
) -> Option<SessionBuild> {
    let Subscription { revision, region } = view.subscription;
    let vision = viewers.get(&view.viewer)?.vision;
    let tick = world.tick();
    // An AI gets a changed fog view at most once a second. A view whose
    // region and overview did not change since the held one would equal it
    // (and not be sent), so it is not rebuilt.
    let mode_changed = view.fog_disabled != Some(world.fog_disabled());
    let fog = (mode_changed
        || (view.vision != Some(vision)
            && (view.vision.is_none() || !view.ai || tick.0.is_multiple_of(AI_FOG_TICKS))
            && view
                .vision
                .is_none_or(|held| world.fog_changed_since(view.viewer, region, held))))
    .then(|| {
        (
            super::fog::state(world, view.viewer, revision, region),
            vision,
        )
    });
    let lens = Lens {
        viewer: view.viewer,
        primary: view.primary,
        region,
    };
    let slots = frame.current(world, &lens, scratch);
    // The frame already holds each unit's owner and health this tick, so
    // only the viewer's living units are looked up in the world.
    let move_queues = destinations(
        world,
        slots
            .iter()
            .map(|slot| frame.state(*slot))
            .filter(|state| state.player == view.viewer && state.hp > 0)
            .map(|state| state.id),
    );
    Some(SessionBuild {
        session_id: view.session_id,
        revision,
        viewer: view.viewer,
        units: (slots.len() <= aoe_protocol::MAX_SUBSCRIBED_UNITS).then_some(slots),
        fog,
        animals: super::wildlife::animals(world, region, view.viewer),
        projectiles: super::wildlife::projectiles(world, region, view.viewer),
        move_queues,
    })
}

pub(super) fn move_queues(
    world: &GameWorld,
    viewer: PlayerId,
    ids: impl Iterator<Item = EntityId>,
) -> Vec<aoe_protocol::MoveQueue> {
    destinations(
        world,
        ids.filter(|id| {
            world
                .unit(*id)
                .is_some_and(|u| u.player == viewer && u.hp > 0)
        }),
    )
}

/// The queued destinations of units already known to belong to the viewer.
fn destinations(
    world: &GameWorld,
    ids: impl Iterator<Item = EntityId>,
) -> Vec<aoe_protocol::MoveQueue> {
    ids.filter_map(|entity_id| {
        let destinations = world.move_destinations(entity_id);
        (!destinations.is_empty()).then_some(aoe_protocol::MoveQueue {
            entity_id,
            destinations,
        })
    })
    .collect()
}

/// Reads `world` once and builds every session's view, on as many threads as
/// there is work for.
pub(super) fn build(
    world: &GameWorld,
    views: &[View],
    changes: &BTreeMap<PlayerId, BTreeSet<EntityId>>,
    (previous, generation): (Option<Arc<UnitFrame>>, u64),
) -> Option<Built> {
    let started = std::time::Instant::now();
    let viewers = viewer_states(world, views, changes);
    let viewers_done = started.elapsed();
    let threads = rayon::current_num_threads().min(8);
    let previous = previous.and_then(|frame| Arc::try_unwrap(frame).ok());
    let frame = UnitFrame::build(world, threads, generation, previous)?;
    #[cfg(test)]
    frame.assert_matches_world(world);
    let frame_done = started.elapsed();
    #[cfg(test)]
    let workers = std::sync::Mutex::new(Vec::new());
    let scratch = || {
        #[cfg(test)]
        workers.lock().unwrap().push(std::thread::current().id());
        Scratch::default()
    };
    let one =
        |scratch: &mut Scratch, view: &View| build_session(world, &frame, &viewers, view, scratch);
    // Sessions differ widely in cost (a crowded camera against an idle AI),
    // so they are stolen a few at a time rather than split into fixed parts.
    let sessions = if threads <= 1 || views.len() < 2 * SESSIONS_PER_TASK {
        let mut scratch = scratch();
        views
            .iter()
            .filter_map(|view| one(&mut scratch, view))
            .collect()
    } else {
        views
            .par_iter()
            .with_min_len(SESSIONS_PER_TASK)
            .map_init(scratch, |scratch, view| {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| one(scratch, view)))
                    .ok()
                    .flatten()
            })
            .collect::<Vec<_>>()
            .into_iter()
            .flatten()
            .collect()
    };
    let done = started.elapsed();
    Some(Built {
        #[cfg(test)]
        workers: workers.into_inner().unwrap(),
        tick: world.tick(),
        fog_disabled: world.fog_disabled(),
        frame: Arc::new(frame),
        viewers,
        sessions,
        phases: [viewers_done, frame_done - viewers_done, done - frame_done],
    })
}

#[path = "fanout/subscription.rs"]
pub(super) mod subscription;
#[path = "fanout/tests.rs"]
#[cfg(test)]
mod tests;

#[cfg(any(test, feature = "bench-fixtures"))]
#[path = "fanout/benchmark.rs"]
mod benchmark;
