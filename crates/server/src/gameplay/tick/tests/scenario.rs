//! A deterministic five-seat match for fan-out equivalence and secrecy tests.
use crate::gameplay::{GameplayService, SlotSetup};
use aoe_core::EntityId;
use aoe_core::{PlayerId, TileCoord, TileRect, WorldPosition};
use aoe_map::{MapPackage, MapRequest, ResourceKind};
use aoe_protocol::GameplayServerMessage;
use aoe_simulation::{AnimalKind, BuildingKind, GameWorld, UnitKind};
use tokio::sync::mpsc;

pub(super) const SEATS: usize = 5;
const SIDE: i32 = 400;
const VIEW: i32 = 120;

/// Everything one session was sent, in order, and the player it looks through.
pub(super) struct SessionLog {
    pub viewer: PlayerId,
    pub messages: Vec<GameplayServerMessage>,
}

fn package() -> MapPackage {
    let request = MapRequest {
        center_latitude_e7: 466_000_000,
        center_longitude_e7: 25_000_000,
        requested_side_meters: SIDE as u64 * 60,
        compression: aoe_map::Ratio::new(30, 1).unwrap(),
        seed: 5,
        ..MapRequest::default()
    };
    MapPackage::new(1, request, Vec::new()).unwrap()
}

fn tile_position(tile: TileCoord) -> WorldPosition {
    WorldPosition::from_tile_center(tile).unwrap()
}

fn region(center: TileCoord) -> TileRect {
    let clamp = |v: i32| v.clamp(0, SIDE - VIEW);
    TileRect::from_xywh(
        clamp(center.x - VIEW / 2),
        clamp(center.y - VIEW / 2),
        VIEW,
        VIEW,
    )
}

/// Where each village was asked for; the world may shift it a little.
fn homes() -> Vec<TileCoord> {
    (0..SEATS as i32)
        .map(|i| TileCoord::new(150 + 22 * i, 170 + 14 * (i % 2)))
        .collect()
}

/// The first village that fits at or near `want`, searching outward.
fn found_village(
    world: &mut GameWorld,
    player: PlayerId,
    want: TileCoord,
) -> (TileCoord, EntityId) {
    let mut offsets = (-60..=60)
        .step_by(6)
        .flat_map(|dy| (-60..=60).step_by(6).map(move |dx| (dx, dy)))
        .collect::<Vec<(i32, i32)>>();
    offsets.sort_by_key(|(dx, dy)| dx * dx + dy * dy);
    for (dx, dy) in offsets {
        let at = TileCoord::new(want.x + dx, want.y + dy);
        if let Ok(primary) = world.initialize_village(player, at) {
            return (at, primary);
        }
    }
    panic!("no village site near {want:?}");
}

fn orders(world: &mut GameWorld, tick: u32, homes: &[TileCoord]) {
    if tick == 0 {
        for (index, home) in homes.iter().enumerate() {
            let player = PlayerId(index as u16);
            let next = homes[(index + 1) % SEATS];
            let units = world
                .units()
                .filter(|u| u.player == player)
                .collect::<Vec<_>>();
            let villagers = units.iter().filter(|u| u.kind == UnitKind::Villager);
            for (n, villager) in villagers.enumerate() {
                if n < 4 {
                    if let Some(node) = world.nearest_resource(*home, ResourceKind::Wood, 24, None)
                    {
                        let _ = world.issue_gather(player, villager.id, node.id);
                    }
                } else if n == 4 {
                    let _ = world.issue_move(villager.id, tile_position(next));
                }
            }
            if let Some(scout) = units.iter().find(|u| u.kind == UnitKind::Scout) {
                let _ = world.issue_move(scout.id, tile_position(next));
            }
        }
        let _ = world.spawn_animal(AnimalKind::Deer, tile_position(TileCoord::new(165, 178)));
        let _ = world.spawn_animal(AnimalKind::Wolf, tile_position(TileCoord::new(200, 172)));
        let hall = world
            .buildings()
            .find(|b| b.player == PlayerId(0) && b.kind == BuildingKind::TownCenter)
            .map(|b| b.id);
        if let Some(hall) = hall {
            let _ = world.issue_train(PlayerId(0), hall, UnitKind::Villager);
        }
    }
    if tick == 5 {
        // Visitors standing in plain sight of the neighbour's village.
        for (index, home) in homes.iter().enumerate() {
            let visitor = PlayerId(((index + 1) % SEATS) as u16);
            for dx in 4..12 {
                let at = TileCoord::new(home.x + dx, home.y + 3);
                if let Ok(id) = world.spawn_unit(visitor, tile_position(at)) {
                    let _ = world.issue_move(id, tile_position(TileCoord::new(at.x - 3, at.y + 6)));
                    break;
                }
            }
        }
    }
    if tick == 10 {
        let builders = world
            .units()
            .filter(|u| u.player == PlayerId(0) && u.kind == UnitKind::Villager)
            .skip(5)
            .take(2)
            .map(|u| u.id)
            .collect::<Vec<_>>();
        let origin = TileCoord::new(homes[0].x + 6, homes[0].y + 6);
        let _ = world.issue_build(PlayerId(0), &builders, BuildingKind::House, origin);
    }
    if tick == 150 {
        // Everyone walks home again, so units leave and re-enter other views.
        for (index, home) in homes.iter().enumerate() {
            let scouts = world
                .units()
                .filter(|u| u.player == PlayerId(index as u16) && u.kind == UnitKind::Scout)
                .map(|u| u.id)
                .collect::<Vec<_>>();
            for scout in scouts {
                let _ = world.issue_move(scout, tile_position(*home));
            }
        }
    }
}

/// Runs `ticks` ticks of the match with one session per seat plus a spectator.
/// `observe` sees each session's new messages together with the world they
/// were built from, before the next tick can change it.
pub(super) async fn run(
    ticks: u32,
    observe: &mut dyn FnMut(&GameWorld, PlayerId, &[GameplayServerMessage]),
) -> Vec<SessionLog> {
    let mut world = GameWorld::from_map(package()).unwrap();
    let mut setups = Vec::new();
    let mut placed = Vec::new();
    for (index, want) in homes().into_iter().enumerate() {
        let player = PlayerId(index as u16);
        let (home, primary) = found_village(&mut world, player, want);
        placed.push(home);
        setups.push(SlotSetup {
            player,
            primary_unit_id: primary,
            human: index == 0,
        });
    }
    let service = GameplayService::from_world_slots(world, setups, None, None);
    let infos = service.slot_infos().await;
    let mut logs = Vec::new();
    let mut ids = Vec::new();
    let mut receivers = Vec::new();
    for (index, info) in infos.iter().enumerate() {
        let (sender, receiver) = mpsc::channel(8192);
        let (id, _) = service.join_slot(info.token, sender).await.unwrap();
        service.subscribe(id, 1, region(placed[index])).await;
        logs.push(SessionLog {
            viewer: info.player,
            messages: Vec::new(),
        });
        ids.push(id);
        receivers.push(receiver);
    }
    let (sender, receiver) = mpsc::channel(8192);
    let (id, _) = service.register(None, sender).await;
    service.subscribe(id, 1, region(placed[1])).await;
    logs.push(SessionLog {
        viewer: infos[0].player,
        messages: Vec::new(),
    });
    ids.push(id);
    receivers.push(receiver);
    for tick in 0..ticks {
        {
            let mut world = service.world.write().await;
            orders(&mut world, tick, &placed);
        }
        if tick == 100 {
            // A moved view and a resync rebuild their baselines mid-match.
            service
                .subscribe(ids[2], 2, region(TileCoord::new(190, 180)))
                .await;
            service.resync(ids[3], 3).await;
        }
        service.tick().await;
        let world = service.world.read().await;
        for (log, receiver) in logs.iter_mut().zip(&mut receivers) {
            let from = log.messages.len();
            while let Ok(message) = receiver.try_recv() {
                log.messages.push(message);
            }
            observe(&world, log.viewer, &log.messages[from..]);
        }
    }
    logs
}
