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
    pub tick: Tick,
    pub frame: UnitFrame,
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
        *rebuild.entry(view.viewer).or_default() |= view.rebuild;
    }
    let disabled = world.fog_disabled();
    let mut everything = None;
    let mut viewers = BTreeMap::new();
    for (viewer, rebuild) in rebuild {
        let players = if disabled {
            economy::players(world)
        } else {
            economy::player(world, viewer)
                .map(|state| (state.player, state))
                .into_iter()
                .collect()
        };
        let buildings = if disabled {
            // With fog disabled all targets are revealed as well. Keep one
            // shared map even when a hundred seats have set their rally points.
            let all: &Arc<_> =
                everything.get_or_insert_with(|| Arc::new(economy::buildings(world, viewer)));
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
        viewers.insert(
            viewer,
            ViewerState {
                players,
                buildings,
                vision: world.player_vision_revision(viewer),
            },
        );
    }
    viewers
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
    // An AI gets a changed fog view at most once a second.
    let fog = (view.vision != Some(vision) && (!view.ai || tick.0.is_multiple_of(AI_FOG_TICKS)))
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
    let move_queues = move_queues(
        world,
        view.viewer,
        slots.iter().map(|slot| frame.state(*slot).id),
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
    ids.filter(|id| {
        world
            .unit(*id)
            .is_some_and(|u| u.player == viewer && u.hp > 0)
    })
    .filter_map(|entity_id| {
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
) -> Option<Built> {
    let started = std::time::Instant::now();
    let viewers = viewer_states(world, views, changes);
    let viewers_done = started.elapsed();
    let distinct = viewers.keys().copied().collect::<Vec<_>>();
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(8);
    let frame = UnitFrame::build(world, &distinct, threads)?;
    let frame_done = started.elapsed();
    let threads = threads.min(views.len() / 8 + 1);
    let work = |part: &[View]| {
        let mut scratch = Scratch::default();
        part.iter()
            .filter_map(|view| build_session(world, &frame, &viewers, view, &mut scratch))
            .collect::<Vec<_>>()
    };
    let sessions = if threads <= 1 {
        work(views)
    } else {
        let size = views.len().div_ceil(threads);
        std::thread::scope(|scope| {
            let handles = views
                .chunks(size)
                .map(|part| scope.spawn(|| work(part)))
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .flat_map(|handle| handle.join().unwrap_or_default())
                .collect()
        })
    };
    let done = started.elapsed();
    Some(Built {
        tick: world.tick(),
        frame,
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
