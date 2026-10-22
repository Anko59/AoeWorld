//! A session whose delivery misses a tick's building changes must still end
//! up with the buildings its viewer knows: nothing may leave it stale.
use super::scenario::{Fixture, start, view_region};
use crate::gameplay::economy;
use aoe_core::{EntityId, PlayerId, TileCoord};
use aoe_protocol::{GameplayBuildingState, GameplayServerMessage};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;

type Buildings = BTreeMap<EntityId, GameplayBuildingState>;

/// What a client holds after applying every message it was sent.
fn apply(client: &mut Buildings, fixture: &mut Fixture, index: usize) -> bool {
    let mut over_limit = false;
    while let Ok(message) = fixture.receivers[index].try_recv() {
        match message {
            GameplayServerMessage::Snapshot { buildings, .. } => {
                *client = buildings.into_iter().map(|b| (b.id, b)).collect();
            }
            GameplayServerMessage::Tick {
                changed_buildings,
                building_removals,
                ..
            } => {
                for building in changed_buildings {
                    client.insert(building.id, building);
                }
                for id in building_removals {
                    client.remove(&id);
                }
            }
            GameplayServerMessage::Error { code: 413, .. } => over_limit = true,
            _ => {}
        }
    }
    over_limit
}

async fn known(fixture: &Fixture, viewer: PlayerId) -> Buildings {
    economy::buildings(&*fixture.service.world.read().await, viewer)
}

/// Places a house for `player` near its home at once, through the world.
async fn place_house(fixture: &Fixture, player: PlayerId) -> EntityId {
    let mut world = fixture.service.world.write().await;
    let home = fixture.placed[usize::from(player.0)];
    for dy in (-40..=40).step_by(3) {
        for dx in (-40..=40).step_by(3) {
            let at = TileCoord::new(home.x + dx, home.y + dy);
            if let Ok(id) = world.place_town_center(player, at) {
                return id;
            }
        }
    }
    panic!("no site for a building near {home:?}");
}

async fn settle(fixture: &mut Fixture, clients: &mut [Buildings], ticks: usize) {
    for _ in 0..ticks {
        fixture.service.tick().await;
    }
    for (index, client) in clients.iter_mut().enumerate() {
        apply(client, fixture, index);
    }
}

async fn assert_converged(fixture: &Fixture, clients: &[Buildings]) {
    for (index, client) in clients.iter().enumerate() {
        let viewer = fixture.viewers[index];
        let truth = known(fixture, viewer).await;
        for id in client.keys().chain(truth.keys()) {
            assert_eq!(
                client
                    .get(id)
                    .map(|b| (b.progress_permille, b.hp, b.construction)),
                truth
                    .get(id)
                    .map(|b| (b.progress_permille, b.hp, b.construction)),
                "the client of session {index} is stale about {id:?}"
            );
        }
        let sessions = fixture.service.sessions.lock().await;
        assert_eq!(
            sessions[&fixture.ids[index]].resident_buildings, truth,
            "the server baseline of session {index} is stale"
        );
    }
}

fn poll_once<F: Future + Unpin>(future: &mut F) -> impl Future<Output = ()> + '_ {
    std::future::poll_fn(move |cx| {
        let _ = Pin::new(&mut *future).poll(cx);
        Poll::Ready(())
    })
}

/// Starts a house for player 0 and returns it with the number of ticks it
/// needs to complete, found by running one fixture to completion.
async fn house_and_completion_ticks() -> usize {
    let fixture = start().await;
    let house = start_house(&fixture).await;
    for ticks in 1..2_000 {
        fixture.service.tick().await;
        if complete(&fixture, house).await {
            return ticks;
        }
    }
    let b = known(&fixture, PlayerId(0)).await.get(&house).cloned();
    panic!(
        "the house never completed: {:?}",
        b.map(|b| (b.progress_permille, b.construction))
    );
}

async fn complete(fixture: &Fixture, house: EntityId) -> bool {
    known(fixture, PlayerId(0))
        .await
        .get(&house)
        .is_some_and(|b| b.construction == aoe_protocol::ConstructionState::Complete)
}

/// Orders a house through the world on the first reachable site.
async fn start_house(fixture: &Fixture) -> EntityId {
    let mut world = fixture.service.world.write().await;
    let workers = world
        .units()
        .filter(|u| u.player == PlayerId(0) && u.kind == aoe_simulation::UnitKind::Villager)
        .map(|u| u.id)
        .take(4)
        .collect::<Vec<_>>();
    let home = fixture.placed[0];
    let mut offsets = (-30..=30)
        .step_by(3)
        .flat_map(|dy| (-30..=30).step_by(3).map(move |dx| (dx, dy)))
        .collect::<Vec<(i32, i32)>>();
    offsets.sort_by_key(|(dx, dy)| dx * dx + dy * dy);
    for (dx, dy) in offsets {
        let at = TileCoord::new(home.x + dx, home.y + dy);
        let kind = aoe_simulation::BuildingKind::House;
        if let Ok(id) = world.issue_build(PlayerId(0), &workers, kind, at) {
            return id;
        }
    }
    panic!("no reachable house site near {home:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_subscribe_that_races_the_tick_still_learns_the_ticks_buildings() {
    // The simulation is deterministic: a first run tells which tick completes
    // the house, a second one races exactly that tick.
    let completing = house_and_completion_ticks().await;
    let mut fixture = start().await;
    let house = start_house(&fixture).await;
    let mut clients = vec![Buildings::new(); fixture.ids.len()];
    settle(&mut fixture, &mut clients, completing - 1).await;
    assert!(!complete(&fixture, house).await);
    assert_converged(&fixture, &clients).await;
    let (session, region) = (fixture.ids[0], view_region(fixture.placed[0]));
    // Order: the tick queues for the sessions lock first, a subscribe that
    // already holds the world read lock queues behind it. The tick copies its
    // views, then waits for the write lock; the subscribe publishes a
    // baseline from the world as it stood before the tick advanced.
    let held = fixture.service.sessions.lock().await;
    let mut tick = Box::pin(fixture.service.tick());
    poll_once(&mut tick).await;
    let mut subscribe = Box::pin(fixture.service.subscribe(session, 9, region));
    poll_once(&mut subscribe).await;
    drop(held);
    tokio::join!(tick, subscribe);
    apply(&mut clients[0], &mut fixture, 0);
    assert!(
        complete(&fixture, house).await,
        "the raced tick did not complete the house, so the race proved nothing"
    );
    settle(&mut fixture, &mut clients, 2).await;
    assert_converged(&fixture, &clients).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tick_skipped_for_an_over_limit_region_does_not_lose_buildings() {
    let mut fixture = start().await;
    let mut clients = vec![Buildings::new(); fixture.ids.len()];
    settle(&mut fixture, &mut clients, 3).await;
    let home = fixture.placed[3];
    let crowd = {
        let mut world = fixture.service.world.write().await;
        let mut crowd = Vec::new();
        let mut n = 0;
        while crowd.len() <= aoe_protocol::MAX_SUBSCRIBED_UNITS {
            // Visitors standing in plain sight of the village.
            let at = TileCoord::new(home.x + 1 + n % 5, home.y + 1 + (n / 5) % 5);
            n += 1;
            let position = aoe_core::WorldPosition::from_tile_center(at).unwrap();
            if let Ok(id) = world.spawn_unit(PlayerId(9), position) {
                crowd.push(id);
            }
            if n > 400_000 {
                break;
            }
        }
        crowd
    };
    assert!(
        crowd.len() > aoe_protocol::MAX_SUBSCRIBED_UNITS,
        "the world refuses a crowd this size: {}",
        crowd.len()
    );
    place_house(&fixture, PlayerId(3)).await;
    fixture.service.tick().await;
    let overloaded = apply(&mut clients[3], &mut fixture, 3);
    assert!(overloaded, "the region never went over the limit");
    // The view moves off the crowd (same revision), so delivery resumes.
    {
        let mut sessions = fixture.service.sessions.lock().await;
        let session = sessions.get_mut(&fixture.ids[3]).unwrap();
        let revision = session.subscription.unwrap().revision;
        session.subscription = Some(crate::gameplay::Subscription {
            revision,
            region: aoe_core::TileRect::from_xywh(home.x + 25, home.y + 25, 30, 30),
        });
    }
    settle(&mut fixture, &mut clients, 3).await;
    assert_converged(&fixture, &clients).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_viewer_gets_the_full_building_view() {
    let mut fixture = start().await;
    let mut clients = vec![Buildings::new(); fixture.ids.len()];
    settle(&mut fixture, &mut clients, 3).await;
    // Session 2 now looks through player 0: its baselines are dropped.
    fixture.viewers[2] = PlayerId(0);
    fixture
        .service
        .sessions
        .lock()
        .await
        .get_mut(&fixture.ids[2])
        .unwrap()
        .change_viewer(PlayerId(0));
    clients[2].clear();
    settle(&mut fixture, &mut clients, 3).await;
    assert!(!clients[2].is_empty());
    assert_converged(&fixture, &clients).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn toggling_fog_rebuilds_every_buildings_baseline() {
    let mut fixture = start().await;
    let mut clients = vec![Buildings::new(); fixture.ids.len()];
    settle(&mut fixture, &mut clients, 3).await;
    let fogged = clients[0].len();
    fixture.service.set_fog_disabled(true).await;
    settle(&mut fixture, &mut clients, 2).await;
    assert!(clients[0].len() > fogged, "everyone sees every building");
    assert_converged(&fixture, &clients).await;
    place_house(&fixture, PlayerId(1)).await;
    fixture.service.set_fog_disabled(false).await;
    settle(&mut fixture, &mut clients, 3).await;
    assert_eq!(clients[0].len(), fogged, "enemy buildings left again");
    assert_converged(&fixture, &clients).await;
}

/// A viewer without an economy knows its primary unit by id alone; the
/// region scan may collect that same unit, which must stay a single entry.
#[tokio::test(flavor = "multi_thread")]
async fn a_primary_unit_inside_the_region_is_listed_once() {
    use crate::gameplay::GameplayService;
    let service =
        GameplayService::with_population(aoe_core::WorldConfig::default(), 300, 3, 1, 64).unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(256);
    let (session, _) = service.register(None, sender).await;
    let region = aoe_core::TileRect::from_xywh(0, 0, 128, 128);
    service.subscribe(session, 1, region).await;
    {
        let mut world = service.world.write().await;
        let to = aoe_core::WorldPosition::from_tile_center(TileCoord::new(60, 60)).unwrap();
        world.issue_move(EntityId(0), to).unwrap();
    }
    for _ in 0..5 {
        service.tick().await;
    }
    let mut changed = 0;
    while let Ok(message) = receiver.try_recv() {
        if let GameplayServerMessage::Tick { changed_units, .. } = message {
            let mut ids = changed_units.iter().map(|u| u.id).collect::<Vec<_>>();
            ids.sort();
            let before = ids.len();
            ids.dedup();
            assert_eq!(before, ids.len(), "a unit was sent twice in one tick");
            changed += before;
        }
    }
    assert!(changed > 0, "no unit moved, so the test proved nothing");
    let sessions = service.sessions.lock().await;
    let resident = &sessions[&session].resident;
    let mut ids = resident.iter().map(|u| u.id).collect::<Vec<_>>();
    let before = ids.len();
    ids.dedup();
    assert_eq!(before, ids.len(), "the resident list holds a unit twice");
}
