//! Server encode -> wire bytes -> SDK decode. After every message the slot
//! holds exactly the fog the server holds for its player, tile for tile:
//! through sight changes, sends skipped on a full queue, camera moves and
//! resubscribes. No fog message is named here, so the encoding stays free.
use crate::{Visibility, World};
use aoe_core::{EntityId, PlayerId, Seed, TileCoord, TileRect, WorldConfig, WorldPosition};
use aoe_protocol::{
    GameplayServerMessage as M, ResumeToken, decode_gameplay_server, encode_gameplay_server,
};
use aoe_server::GameplayService;
use aoe_simulation::GameWorld;
use tokio::sync::mpsc;

#[path = "fog_wire/corrupt.rs"]
mod corrupt;

const HOME: TileRect = TileRect::from_xywh(0, 0, 96, 96);

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

/// Fog is whatever the server sends that is not one of the other kinds.
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

/// Two players far apart, each with one scout that keeps walking east. The
/// first seat is a person's; the second is a person's or an AI's.
fn start_with(second_is_a_person: bool) -> (GameplayService, [EntityId; 2]) {
    let mut world = GameWorld::new(WorldConfig::new(1024, 1024, Seed(7)).unwrap()).unwrap();
    let mut scouts = [EntityId(0); 2];
    for (player, row) in [(0, 30), (1, 70)] {
        let scout = world.spawn_unit(PlayerId(player), at(20, row)).unwrap();
        world.issue_move(scout, at(300, row)).unwrap();
        scouts[usize::from(player)] = scout;
    }
    let seats = [(0, true), (1, second_is_a_person)]
        .map(|(player, person)| (PlayerId(player), scouts[usize::from(player)], person));
    (
        GameplayService::from_scale_world_slots(world, seats),
        scouts,
    )
}

fn start() -> (GameplayService, [EntityId; 2]) {
    start_with(true)
}

/// One connected page: its outgoing queue on the server and the world the SDK
/// builds from what arrives.
struct Page {
    session: u64,
    player: PlayerId,
    welcome: M,
    sender: mpsc::Sender<M>,
    receiver: mpsc::Receiver<M>,
    world: World,
    region: TileRect,
}

async fn join(
    service: &GameplayService,
    token: Option<ResumeToken>,
    player: PlayerId,
    scout: EntityId,
) -> Page {
    let (sender, receiver) = mpsc::channel(64);
    let (session, welcome) = service.register(token, sender.clone()).await;
    assert!(
        matches!(welcome, M::Welcome { primary_unit_id, .. } if primary_unit_id == scout),
        "fixture: the page sits in the seat of {player:?}"
    );
    let world = World::from_welcome(&welcome).unwrap();
    Page {
        session,
        player,
        welcome,
        sender,
        receiver,
        world,
        region: HOME,
    }
}

fn through_the_wire(message: &M) -> M {
    let bytes = encode_gameplay_server(message).expect("the server sends what encodes");
    decode_gameplay_server(&bytes).expect("and what it encodes decodes")
}

/// Everything waiting for the page goes through the wire into its world.
fn pump(page: &mut Page) -> Vec<M> {
    let mut arrived = Vec::new();
    while let Ok(message) = page.receiver.try_recv() {
        let message = through_the_wire(&message);
        page.world.apply(message.clone());
        arrived.push(message);
    }
    arrived
}

/// A new view: from here on the SDK accepts this revision's messages.
async fn subscribe(
    service: &GameplayService,
    page: &mut Page,
    revision: u64,
    region: TileRect,
) -> Vec<M> {
    service.subscribe(page.session, revision, region).await;
    page.world.subscription = revision;
    page.region = region;
    pump(page)
}

/// The fog the server holds for `player`: the tiles of `region`, row-major,
/// and the strategic overview.
async fn server_fog(
    service: &GameplayService,
    player: PlayerId,
    region: TileRect,
) -> (Vec<u8>, Vec<(u16, u8)>) {
    service
        .inspect_scale_world(|world| {
            (
                world.visibility_tiles(player, region),
                world.vision_overview(player),
            )
        })
        .await
}

async fn vision(service: &GameplayService, player: PlayerId) -> u64 {
    service
        .inspect_scale_world(|world| world.player_vision_revision(player))
        .await
}

/// The same tiles as the SDK answers for them.
fn sdk_tiles(world: &World, region: TileRect) -> Vec<u8> {
    let mut tiles = Vec::new();
    for y in region.min.y..region.max.y {
        for x in region.min.x..region.max.x {
            tiles.push(match world.visibility(TileCoord::new(x, y)) {
                Visibility::Unexplored => 0,
                Visibility::Explored => 1,
                Visibility::Visible => 2,
            });
        }
    }
    tiles
}

fn assert_same_tiles(held: &[u8], truth: &[u8], region: TileRect, when: &str) {
    let differing = held.iter().zip(truth).filter(|(a, b)| a != b).count();
    if let Some(index) = held.iter().zip(truth).position(|(a, b)| a != b) {
        let (x, y) = (index as i32 % region.width(), index as i32 / region.width());
        panic!(
            "{when}: the slot holds {} at tile ({}, {}) where the server holds {}; \
             {differing} of {} tiles differ",
            held[index],
            region.min.x + x,
            region.min.y + y,
            truth[index],
            truth.len()
        );
    }
    assert_eq!(held.len(), truth.len(), "{when}: tile count");
}

async fn assert_in_sync(service: &GameplayService, page: &Page, when: &str) {
    let (tiles, overview) = server_fog(service, page.player, page.region).await;
    let fog = page.world.fog();
    assert_eq!(fog.region(), Some(page.region), "{when}: the live window");
    assert_same_tiles(
        &sdk_tiles(&page.world, page.region),
        &tiles,
        page.region,
        when,
    );
    assert_eq!(fog.overview(), overview, "{when}: the strategic overview");
}

/// A client that holds nothing is given `messages`. It may refuse them, but a
/// view it accepts must be the server's whole view: changes are never laid
/// over a view the client does not have. True when it took a view.
async fn whole_or_refused(
    service: &GameplayService,
    page: &Page,
    messages: &[M],
    when: &str,
) -> bool {
    let mut fresh = World::from_welcome(&page.welcome).unwrap();
    fresh.subscription = page.world.subscription;
    for message in messages {
        fresh.apply(message.clone());
    }
    let Some(region) = fresh.fog().region() else {
        return false;
    };
    assert_eq!(region, page.region, "{when}: a client holding nothing");
    let (tiles, _) = server_fog(service, page.player, region).await;
    let when = format!("{when}: a client holding nothing");
    assert_same_tiles(&sdk_tiles(&fresh, region), &tiles, region, &when);
    true
}

/// One server tick, after which every page holds the server's fog. Returns
/// how many fog messages the pages were sent.
async fn tick(service: &GameplayService, pages: &mut [&mut Page], when: &str) -> usize {
    service.tick().await;
    let mut fog = 0;
    for page in pages.iter_mut() {
        let arrived = pump(page);
        fog += arrived.iter().filter(|m| carries_fog(m)).count();
        assert_in_sync(service, page, when).await;
        whole_or_refused(service, page, &arrived, when).await;
    }
    fog
}

/// Fills the page's outgoing queue: the server's next sends find no room.
fn fill(page: &Page) -> usize {
    let filler = || M::Error {
        code: 429,
        message: "queue pressure".to_owned(),
    };
    let mut sent = 0;
    while page.sender.try_send(filler()).is_ok() {
        sent += 1;
    }
    assert!(sent > 0, "fixture: the queue had room to fill");
    sent
}

/// Ticks until the player's sight has changed `times` more times.
async fn until_sight_moved(service: &GameplayService, player: PlayerId, times: u64) {
    let before = vision(service, player).await;
    for _ in 0..400 {
        if vision(service, player).await >= before + times {
            return;
        }
        service.tick().await;
    }
    panic!("fixture: the scout of {player:?} stopped walking");
}

#[tokio::test]
async fn the_slot_holds_the_servers_fog_through_a_skipped_send_a_camera_move_and_a_resubscribe() {
    let (service, scouts) = start();
    let mut a = join(&service, None, PlayerId(0), scouts[0]).await;
    let mut b = join(&service, None, PlayerId(1), scouts[1]).await;
    for page in [&mut a, &mut b] {
        let first = subscribe(&service, page, 1, HOME).await;
        assert_in_sync(&service, page, "first view").await;
        assert!(whole_or_refused(&service, page, &first, "first view").await);
    }

    // Sight moves: both players look at the same window and each holds its own fog.
    let mut fog = 0;
    for _ in 0..40 {
        fog += tick(&service, &mut [&mut a, &mut b], "walking").await;
    }
    assert!(
        fog >= 6,
        "fixture: {fog} fog messages while both scouts walked"
    );
    let (tiles, _) = server_fog(&service, a.player, a.region).await;
    for state in 0..=2 {
        assert!(tiles.contains(&state), "fixture: the window holds {state}");
    }

    // A's queue is full while its scout walks on: every send to A is skipped.
    let filler = fill(&a);
    until_sight_moved(&service, a.player, 3).await;
    let waiting = pump(&mut a);
    assert_eq!(
        waiting.len(),
        filler,
        "nothing fit while the queue was full"
    );
    assert!(waiting.iter().all(|m| matches!(m, M::Error { .. })));
    assert_ne!(
        sdk_tiles(&a.world, a.region),
        server_fog(&service, a.player, a.region).await.0,
        "fixture: the skipped sends left A behind"
    );
    pump(&mut b);
    tick(&service, &mut [&mut a, &mut b], "after the skipped sends").await;

    // A camera move to an odd-sized window that overlaps the old one.
    let moved = TileRect::from_xywh(7, 3, 97, 95);
    let first = subscribe(&service, &mut a, 2, moved).await;
    assert_in_sync(&service, &a, "camera move").await;
    assert!(whole_or_refused(&service, &a, &first, "camera move").await);
    for _ in 0..16 {
        tick(&service, &mut [&mut a, &mut b], "after the camera move").await;
    }

    // A resubscribe of the same window: whole again, for a client that lost its view.
    service.resync(a.session, 3).await;
    a.world.subscription = 3;
    let first = pump(&mut a);
    assert_in_sync(&service, &a, "resubscribe").await;
    assert!(whole_or_refused(&service, &a, &first, "resubscribe").await);

    // A second page of A's seat, with its own window and its own history.
    let M::Welcome { resume_token, .. } = a.welcome else {
        unreachable!()
    };
    let mut second = join(&service, Some(resume_token), PlayerId(0), scouts[0]).await;
    let window = TileRect::from_xywh(10, 10, 64, 48);
    let first = subscribe(&service, &mut second, 1, window).await;
    assert_in_sync(&service, &second, "second page").await;
    assert!(whole_or_refused(&service, &second, &first, "second page").await);
    for _ in 0..16 {
        tick(&service, &mut [&mut a, &mut second, &mut b], "three pages").await;
    }

    // A looks far away: the ground it explored stays remembered, tile for tile.
    let (left, explored) = (a.region, server_fog(&service, a.player, a.region).await.0);
    subscribe(&service, &mut a, 4, TileRect::from_xywh(600, 600, 33, 17)).await;
    assert_in_sync(&service, &a, "far window").await;
    for (index, state) in explored.iter().enumerate() {
        let tile = TileCoord::new(
            left.min.x + index as i32 % left.width(),
            left.min.y + index as i32 / left.width(),
        );
        assert_eq!(
            a.world.fog().explored_exactly(tile),
            *state != 0,
            "{tile:?} was {state} when the window left it"
        );
    }
    for _ in 0..8 {
        tick(&service, &mut [&mut a, &mut second, &mut b], "far window").await;
    }
}

#[tokio::test]
async fn a_camera_move_that_waits_for_room_is_followed_by_changes_to_that_view() {
    let (service, scouts) = start();
    let mut a = join(&service, None, PlayerId(0), scouts[0]).await;
    subscribe(&service, &mut a, 1, HOME).await;
    for _ in 0..8 {
        tick(&service, &mut [&mut a], "walking").await;
    }

    // The new window's first view finds the queue full and waits for room,
    // while the scout walks on.
    let filler = fill(&a);
    let moved = TileRect::from_xywh(5, 9, 71, 63);
    service.subscribe(a.session, 2, moved).await;
    (a.world.subscription, a.region) = (2, moved);
    until_sight_moved(&service, a.player, 3).await;

    // The reader catches up: the filler, then the waiting view, in order.
    let (mut arrived, mut quiet) = (Vec::new(), 0);
    while quiet < 4 {
        let messages = pump(&mut a);
        quiet = if messages.is_empty() { quiet + 1 } else { 0 };
        arrived.extend(messages);
        tokio::task::yield_now().await;
    }
    let errors = arrived.iter().filter(|m| matches!(m, M::Error { .. }));
    assert_eq!(errors.count(), filler);
    assert!(
        arrived
            .iter()
            .any(|m| matches!(m, M::Snapshot { revision: 2, .. })),
        "fixture: the waiting view arrived"
    );
    assert_eq!(a.world.fog().region(), Some(moved));
    assert_ne!(
        sdk_tiles(&a.world, moved),
        server_fog(&service, a.player, moved).await.0,
        "fixture: the view was built before the scout walked on"
    );
    for _ in 0..12 {
        tick(&service, &mut [&mut a], "after the waiting view").await;
    }
}

#[tokio::test]
async fn an_ai_seat_holds_the_servers_fog_each_time_it_is_told() {
    // An AI's fog goes out at most once a second: several changes per message.
    let (service, scouts) = start_with(false);
    let token = service.slot_infos().await[1].token;
    let mut bot = join(&service, Some(token), PlayerId(1), scouts[1]).await;
    let first = subscribe(&service, &mut bot, 1, HOME).await;
    assert_in_sync(&service, &bot, "first view").await;
    assert!(whole_or_refused(&service, &bot, &first, "first view").await);
    let mut told = 0;
    for _ in 0..120 {
        service.tick().await;
        let arrived = pump(&mut bot);
        if arrived.iter().any(carries_fog) {
            told += 1;
            assert_in_sync(&service, &bot, "an AI's fog").await;
            whole_or_refused(&service, &bot, &arrived, "an AI's fog").await;
        }
    }
    assert!(told >= 3, "fixture: the AI was told {told} times");
}

#[tokio::test]
async fn switching_fog_off_and_on_again_leaves_the_slot_with_the_servers_fog() {
    let (service, scouts) = start();
    let mut a = join(&service, None, PlayerId(0), scouts[0]).await;
    subscribe(&service, &mut a, 1, HOME).await;
    for _ in 0..8 {
        tick(&service, &mut [&mut a], "fog on").await;
    }
    service.set_fog_disabled(true).await;
    for _ in 0..4 {
        service.tick().await;
        pump(&mut a);
    }
    assert!(a.world.fog().disabled(), "the slot learns fog is off");
    assert_eq!(
        a.world.visibility(TileCoord::new(90, 90)),
        Visibility::Visible
    );
    service.set_fog_disabled(false).await;
    for _ in 0..8 {
        tick(&service, &mut [&mut a], "fog on again").await;
    }
    assert!(!a.world.fog().disabled());
}
