//! Fog bytes per message for a 96 x 96 view. `docs/scale/measurements.md`
//! records 9.2 KB per message: one byte per tile and the whole strategic
//! overview, resent whenever the viewer's sight moves.
//!
//! Nothing here names a fog message. Whatever the server sends that is not one
//! of the other kinds counts as fog, in one message or several, so these
//! budgets do not fix the encoding.
use crate::gameplay::GameplayService;
use aoe_core::{EntityId, PlayerId, Seed, TileCoord, TileRect, WorldConfig, WorldPosition};
use aoe_protocol::{GameplayServerMessage as M, encode_gameplay_server};
use aoe_simulation::GameWorld;
use std::collections::{BTreeMap, BTreeSet};
use tokio::sync::mpsc;

const VIEWER: PlayerId = PlayerId(0);
const VIEW: TileRect = TileRect::from_xywh(0, 0, 96, 96);
/// Header, lengths and markers of the fog messages of one send.
const FRAMING: usize = 96;
/// The most a strategic pixel costs today: its index and its state.
const PIXEL: usize = 4;
/// "A message after a one-tile change of one observer is under ~200 bytes."
const ONE_TILE_BUDGET: usize = 200;

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

fn carries_fog(message: &M) -> bool {
    !matches!(
        message,
        M::Welcome { .. }
            | M::Snapshot { .. }
            | M::Tick { .. }
            | M::CommandAck { .. }
            | M::WorldReset { .. }
            | M::ResourceState(_)
            | M::Error { .. }
            | M::WildlifeState { .. }
            | M::Projectiles { .. }
            | M::MoveQueues { .. }
            | M::Holdings { .. }
    )
}

struct Seat {
    service: GameplayService,
    session: u64,
    receiver: mpsc::Receiver<M>,
    scout: EntityId,
    far: Vec<EntityId>,
}

/// One scout of the viewer inside the view. `far` more of its scouts stand
/// still well outside it: each lights strategic pixels and no tile of the view.
async fn seat(far: i32) -> Seat {
    let mut world = GameWorld::new(WorldConfig::new(1024, 1024, Seed(7)).unwrap()).unwrap();
    let scout = world.spawn_unit(VIEWER, at(40, 40)).unwrap();
    let far = (0..far)
        .map(|n| {
            world
                .spawn_unit(VIEWER, at(208 + 16 * (n % 8), 208 + 16 * (n / 8)))
                .unwrap()
        })
        .collect();
    let service = GameplayService::from_world(world, scout, None, None);
    let (sender, receiver) = mpsc::channel(256);
    let (session, _) = service.register(None, sender).await;
    Seat {
        service,
        session,
        receiver,
        scout,
        far,
    }
}

/// The viewer's fog as the server holds it: the tiles of the view, row-major,
/// and the strategic overview.
async fn fog(service: &GameplayService) -> (Vec<u8>, Vec<(u16, u8)>) {
    let world = service.world.read().await;
    (
        world.visibility_tiles(VIEWER, VIEW),
        world.vision_overview(VIEWER),
    )
}

/// Encoded size of every fog message waiting for the session.
fn fog_bytes(receiver: &mut mpsc::Receiver<M>) -> usize {
    let mut bytes = 0;
    while let Ok(message) = receiver.try_recv() {
        if carries_fog(&message) {
            bytes += encode_gameplay_server(&message)
                .expect("the server sends fog that encodes")
                .len();
        }
    }
    bytes
}

/// One tick of a walk: how far the walker went, what that changed in the fog
/// the client holds, and the fog bytes the session was sent for it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Step {
    moved: i32,
    tiles: usize,
    pixels: usize,
    bytes: usize,
}

fn changed_pixels(before: &[(u16, u8)], after: &[(u16, u8)]) -> usize {
    let states = |list: &[(u16, u8)]| list.iter().copied().collect::<BTreeMap<u16, u8>>();
    let (before, after) = (states(before), states(after));
    let pixels = before
        .keys()
        .chain(after.keys())
        .copied()
        .collect::<BTreeSet<u16>>();
    pixels
        .into_iter()
        .filter(|pixel| before.get(pixel) != after.get(pixel))
        .count()
}

/// Walks `walker` to the tile `to`, one server tick at a time.
async fn walk(seat: &mut Seat, walker: EntityId, to: TileCoord) -> Vec<Step> {
    let tile = |world: &GameWorld| world.unit(walker).unwrap().position.tile_floor();
    let mut from = {
        let mut world = seat.service.world.write().await;
        world.issue_move(walker, at(to.x, to.y)).unwrap();
        tile(&world)
    };
    let mut before = fog(&seat.service).await;
    let mut steps = Vec::new();
    while from != to {
        assert!(steps.len() < 600, "fixture: the walker reaches {to:?}");
        seat.service.tick().await;
        let after = fog(&seat.service).await;
        let now = tile(&*seat.service.world.read().await);
        steps.push(Step {
            moved: (now.x - from.x).abs() + (now.y - from.y).abs(),
            tiles: before
                .0
                .iter()
                .zip(&after.0)
                .filter(|(old, new)| old != new)
                .count(),
            pixels: changed_pixels(&before.1, &after.1),
            bytes: fog_bytes(&mut seat.receiver),
        });
        (before, from) = (after, now);
    }
    steps
}

/// Every fog change of the walk is one observer moving one tile and fits the
/// budget; a tick that changed nothing the client holds sends no fog at all.
fn assert_one_tile_budget(steps: &[Step]) {
    let mut costs = Vec::new();
    for step in steps {
        if step.tiles == 0 && step.pixels == 0 {
            assert_eq!(
                step.bytes, 0,
                "fog was sent though nothing the client holds changed: {step:?}"
            );
        } else {
            assert_eq!(step.moved, 1, "fixture: one observer, one tile: {step:?}");
            assert!(step.bytes > 0, "a fog change was not sent: {step:?}");
            costs.push(step.bytes);
        }
    }
    assert!(
        costs.iter().all(|bytes| *bytes <= ONE_TILE_BUDGET),
        "fog bytes per one-tile move of one observer {costs:?}; budget {ONE_TILE_BUDGET}"
    );
}

#[tokio::test]
async fn a_96x96_keyframe_costs_two_bits_per_tile() {
    let mut seat = seat(0).await;
    let scout = seat.scout;
    // A trail behind the scout: the view holds all three states.
    walk(&mut seat, scout, TileCoord::new(52, 40)).await;
    seat.service.subscribe(seat.session, 1, VIEW).await;
    let bytes = fog_bytes(&mut seat.receiver);
    let (tiles, overview) = fog(&seat.service).await;
    for state in 0..=2 {
        assert!(tiles.contains(&state), "fixture: the view holds {state}");
    }
    assert!(bytes > 0, "a first view carries fog");
    let budget = tiles.len() / 4 + PIXEL * overview.len() + FRAMING;
    assert!(
        bytes <= budget,
        "the first fog of a 96 x 96 view took {bytes} bytes; two bits per tile, \
         its {} strategic pixels and the frame allow {budget}",
        overview.len()
    );
}

#[tokio::test]
async fn one_observer_moving_one_tile_costs_under_200_fog_bytes() {
    // The viewer also holds some 240 strategic pixels that never change.
    let mut seat = seat(40).await;
    let scout = seat.scout;
    seat.service.subscribe(seat.session, 1, VIEW).await;
    fog_bytes(&mut seat.receiver);
    let held = fog(&seat.service).await.1.len();
    assert!(held >= 200, "fixture: {held} strategic pixels held");
    let steps = walk(&mut seat, scout, TileCoord::new(46, 40)).await;
    let moves = steps
        .iter()
        .filter(|step| step.tiles > 0)
        .collect::<Vec<_>>();
    assert_eq!(moves.len(), 6, "fixture: six one-tile moves: {steps:?}");
    assert!(
        moves.iter().any(|step| step.pixels == 0) && moves.iter().any(|step| step.pixels > 0),
        "fixture: moves with and without a strategic pixel change: {moves:?}"
    );
    assert_one_tile_budget(&steps);
}

#[tokio::test]
async fn an_unchanged_overview_adds_zero_fog_bytes() {
    // The same walk, seen by a viewer holding a handful of strategic pixels
    // and by one holding hundreds more that never change.
    let mut runs = Vec::new();
    for far in [0, 40] {
        let mut seat = seat(far).await;
        let scout = seat.scout;
        seat.service.subscribe(seat.session, 1, VIEW).await;
        fog_bytes(&mut seat.receiver);
        let held = fog(&seat.service).await.1.len();
        runs.push((held, walk(&mut seat, scout, TileCoord::new(46, 40)).await));
    }
    let [(few, small), (many, large)] = &runs[..] else {
        unreachable!()
    };
    assert!(*many >= *few + 200, "fixture: {few} and {many} pixels held");
    let changes = |steps: &[Step]| {
        steps
            .iter()
            .map(|step| (step.moved, step.tiles, step.pixels))
            .collect::<Vec<_>>()
    };
    assert_eq!(changes(small), changes(large), "fixture: the same walk");
    let sent = |steps: &[Step]| {
        steps
            .iter()
            .map(|step| step.bytes)
            .filter(|bytes| *bytes > 0)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        sent(large),
        sent(small),
        "fog bytes per message while holding {many} strategic pixels (left) and {few} \
         (right): the pixels that did not change must add nothing"
    );
}

#[tokio::test]
async fn a_far_observer_costs_only_its_changed_strategic_pixels() {
    let mut seat = seat(40).await;
    seat.service.subscribe(seat.session, 1, VIEW).await;
    fog_bytes(&mut seat.receiver);
    let walker = seat.far[0];
    let steps = walk(&mut seat, walker, TileCoord::new(214, 208)).await;
    assert!(
        steps.iter().all(|step| step.tiles == 0),
        "fixture: the walker never changes a tile of the view"
    );
    assert!(
        steps.iter().any(|step| step.pixels > 0),
        "fixture: its walk changes strategic pixels"
    );
    assert!(
        steps.iter().any(|step| step.moved == 1 && step.pixels == 0),
        "fixture: and some of its moves change none"
    );
    assert_one_tile_budget(&steps);
}
