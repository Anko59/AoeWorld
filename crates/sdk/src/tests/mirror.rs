//! `Mirror`: the in-process client state. It is what the socket session keeps
//! (the world model, the follow-view subscription, the wanted map chunks)
//! without the socket, so a seat played inside the server sees what a socket
//! client sees.
//!
//! The interface these tests pin, for the implementer (`crates/sdk/src/mirror.rs`,
//! exported as `aoe_sdk::Mirror`, with the socket session using the same code):
//! - `Mirror::new(&Welcome) -> Option<Mirror>`: `None` for any other message;
//!   the world starts at subscription revision 1 like a joined session, with
//!   the default `ViewPolicy`.
//! - `apply(&mut self, GameplayServerMessage) -> Vec<Event>`.
//! - `take_subscribe(&mut self) -> Option<GameplayClientMessage>`: the next
//!   `Subscribe` the follow view wants (`desired_region`, `drifted`,
//!   `RESUBSCRIBE_MIN_TICKS`), at most once per drift; it advances the world's
//!   subscription revision.
//! - `wanted_chunks(&mut self, limit: usize) -> Vec<([u8; 32], (i32, i32))>`:
//!   map hash and chunk of what the fog knows and was not yet requested.
//! - `insert_chunk(&mut self, (i32, i32), Result<CompactChunk, String>)`: a
//!   failed fetch lets the chunk be requested again.
//! - `world(&self) -> &World`.
use crate::{Mirror, UnitKind, UnitTask, World};
use aoe_core::{EntityId, PlayerId, Tick, TileCoord, TileRect, WorldPosition};
use aoe_protocol::{
    FogState, GameplayClientMessage, GameplayPlayerState, GameplayServerMessage as M,
    GameplayUnitState, MarketPrices, Stockpile,
};

const ME: PlayerId = PlayerId(3);
const FOE: PlayerId = PlayerId(4);

fn welcome(hash: Option<[u8; 32]>) -> M {
    M::Welcome {
        version: 3,
        world_id: 11,
        map_content_hash: hash,
        map_metadata: None,
        width_tiles: 200,
        height_tiles: 200,
        coordinate_precision: 1024,
        tick_hz: 20,
        primary_unit_id: EntityId(1),
        resume_token: aoe_protocol::ResumeToken([1; 24]),
    }
}

fn unit(id: u32, player: PlayerId, at: TileCoord) -> GameplayUnitState {
    GameplayUnitState {
        id: EntityId(id),
        player,
        position: WorldPosition::from_tile_center(at).unwrap(),
        moving: false,
        planning: false,
        facing: 0,
        kind: UnitKind::Villager,
        task: UnitTask::Idle,
        carried: None,
        gathering: None,
        hp: 25,
        max_hp: 25,
        attack_target: None,
        last_hit: None,
        death_tick: None,
        hunt_target: None,
        death_hunt: None,
        auto_scout: false,
        sergeant: false,
        cargo_count: 0,
        native_action: None,
        home_castle: None,
        managed_by: None,
        commanded_by: None,
        general: None,
        stance: Default::default(),
    }
}

fn player() -> GameplayPlayerState {
    GameplayPlayerState {
        player: ME,
        stockpile: Stockpile {
            food: 200,
            wood: 200,
            gold: 100,
            stone: 200,
        },
        population_used: 2,
        population_cap: 5,
        age: aoe_protocol::Age::Dark,
        researched: 0,
        recent_deliveries: [0; 4],
        unit_counts: [0; aoe_protocol::UnitKind::ALL.len()],
        held_tiles: 0,
        held_share_ppm: 0,
        food_upkeep_per_minute: 0,
        food_upkeep_shortage: false,
        market_prices: MarketPrices::default(),
    }
}

fn snapshot(units: Vec<GameplayUnitState>) -> M {
    M::Snapshot {
        revision: 1,
        tick: Tick(0),
        units,
        players: vec![player()],
        buildings: Vec::new(),
    }
}

fn tick(n: u64, revision: u64, changed: Vec<GameplayUnitState>) -> M {
    M::Tick {
        revision,
        tick: Tick(n),
        changed_units: changed,
        removals: Vec::new(),
        changed_players: Vec::new(),
        changed_buildings: Vec::new(),
        building_removals: Vec::new(),
    }
}

fn fog(region: TileRect, lit: usize) -> M {
    let mut tiles = vec![0_u8; (region.width() * region.height()) as usize];
    tiles[lit] = 2;
    M::FogState(FogState {
        revision: 1,
        tick: Tick(2),
        region,
        disabled: false,
        tiles,
        overview: Vec::new(),
    })
}

fn home() -> TileCoord {
    TileCoord::new(100, 100)
}

#[test]
fn applied_to_a_recorded_sequence_a_mirror_gives_the_socket_sessions_world_and_events() {
    let recorded = vec![
        snapshot(vec![unit(1, ME, home()), unit(2, ME, home())]),
        tick(
            5,
            1,
            vec![unit(3, ME, home()), unit(40, FOE, TileCoord::new(104, 100))],
        ),
        fog(TileRect::from_xywh(90, 90, 20, 20), 5),
        M::CommandAck {
            sequence: 4,
            result: crate::CommandResult::RejectedInsufficientResources,
            applied_tick: Tick(5),
        },
        tick(8, 1, vec![]),
        // A frame of a subscription that is gone changes nothing.
        tick(9, 7, vec![unit(9, ME, home())]),
    ];
    // What the socket session keeps: a model joined at subscription 1.
    let mut socket = World::from_welcome(&welcome(None)).unwrap();
    socket.subscription = 1;
    let mut mirror = Mirror::new(&welcome(None)).expect("a mirror from the welcome");
    for message in recorded {
        let expected = socket.apply(message.clone());
        assert_eq!(mirror.apply(message.clone()), expected, "{message:?}");
    }
    assert_eq!(format!("{:?}", mirror.world()), format!("{socket:?}"));
    assert_eq!(mirror.world().me(), Some(ME));
    assert_eq!(mirror.world().my_units(None).len(), 3);
    assert_eq!(mirror.world().visible_enemies().len(), 1);
    assert!(
        Mirror::new(&tick(1, 1, vec![])).is_none(),
        "only a Welcome builds one"
    );
}

#[test]
fn the_follow_view_resubscribes_once_the_units_drift_and_never_before_the_minimum_ticks() {
    let far = TileCoord::new(170, 170);
    let mut mirror = Mirror::new(&welcome(None)).unwrap();
    mirror.apply(snapshot(vec![unit(1, ME, far)]));
    // The window opened at the map's centre; the units are far outside it, but
    // the view is held for the minimum number of ticks (20).
    assert_eq!(mirror.take_subscribe(), None);
    mirror.apply(tick(19, 1, vec![]));
    assert_eq!(mirror.take_subscribe(), None);
    mirror.apply(tick(20, 1, vec![]));
    let Some(GameplayClientMessage::Subscribe {
        revision,
        region,
        unit_zoom,
    }) = mirror.take_subscribe()
    else {
        panic!("the drifted view must ask for a new window");
    };
    assert_eq!(revision, 2);
    assert!(!unit_zoom);
    assert!(region.contains(far), "{region:?} lost the units");
    assert_eq!(mirror.take_subscribe(), None, "once per drift");
    // Frames of the old subscription are ignored from now on.
    assert!(mirror.apply(tick(40, 1, vec![unit(9, ME, far)])).is_empty());
    assert_eq!(mirror.world().tick(), 20);
}

#[test]
fn a_mirror_wants_the_chunks_the_fog_knows_and_asks_again_after_a_failed_fetch() {
    let hash = [7_u8; 32];
    let mut mirror = Mirror::new(&welcome(Some(hash))).unwrap();
    mirror.apply(snapshot(vec![unit(1, ME, home())]));
    assert!(
        mirror.wanted_chunks(4).is_empty(),
        "no fog message, no chunk"
    );
    mirror.apply(fog(TileRect::from_xywh(0, 0, 10, 10), 0));
    assert_eq!(mirror.wanted_chunks(4), vec![(hash, (0, 0))]);
    assert!(mirror.wanted_chunks(4).is_empty(), "already requested");
    mirror.insert_chunk((0, 0), Err("unavailable".to_owned()));
    assert_eq!(mirror.wanted_chunks(4), vec![(hash, (0, 0))]);
}

#[test]
fn a_match_without_a_map_hash_wants_no_chunks() {
    let mut mirror = Mirror::new(&welcome(None)).unwrap();
    mirror.apply(snapshot(vec![unit(1, ME, home())]));
    mirror.apply(fog(TileRect::from_xywh(0, 0, 10, 10), 0));
    assert!(mirror.wanted_chunks(4).is_empty());
}
