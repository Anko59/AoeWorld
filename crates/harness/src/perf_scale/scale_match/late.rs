//! `--late-game`: slot 0 played by a scripted human on the SDK client that
//! keeps its seeded army marching, as the owner's 870 soldiers did. Every
//! `ROUND` it splits the army (by id) into `GROUPS` and sends each a Line
//! formation attack-move far across the map, the direction turning each round
//! (a fixed sequence from the seed). The villagers were set to work by the
//! server's seeding and are left alone.
use super::BoxError;
use aoe_sdk::{Client, ConnectOptions, EntityId, Event, Formation, Order, TileCoord, World};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// Seconds between march rounds.
const ROUND: Duration = Duration::from_secs(45);
/// Groups per round and the gap between their orders (a person's pace).
const GROUPS: usize = 4;
const ORDER_GAP: Duration = Duration::from_secs(2);
/// Straight march distances in tiles, used in turn.
const DISTANCES: [i32; 3] = [300, 500, 700];

pub(super) struct Driver {
    handle: JoinHandle<Value>,
    stop: Arc<AtomicBool>,
}

impl Driver {
    pub(super) fn finish(self) -> Value {
        self.stop.store(true, Ordering::Relaxed);
        self.handle
            .join()
            .unwrap_or_else(|_| json!({"error": "the late-game driver panicked"}))
    }
}

/// Plays slot 0 of the match `info` (the `POST /match` answer) on `address`.
pub(super) fn start(address: &str, info: &Value, seed: u64) -> Driver {
    let url = format!("http://{address}");
    let token = info["slots"][0]["token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let handle = std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => return json!({"error": error.to_string()}),
        };
        runtime
            .block_on(run(&url, &token, seed, &flag))
            .unwrap_or_else(|error| json!({"error": error.to_string()}))
    });
    Driver { handle, stop }
}

/// Own living land soldiers, by id (what a box selection would order).
fn soldiers(world: &World) -> Vec<EntityId> {
    let mut all: Vec<_> = world
        .my_units(None)
        .into_iter()
        .filter(|u| u.is_military() && !u.kind().is_naval())
        .map(|u| u.id())
        .collect();
    all.sort_unstable();
    all
}

/// Where `group` marches in `round`: `DISTANCES` away from its centroid at an
/// angle that turns by the golden angle each round, clamped into the map.
pub(super) fn goal(
    world: &World,
    group: &[EntityId],
    round: u64,
    index: usize,
    seed: u64,
) -> TileCoord {
    let (mut x, mut y, mut n) = (0.0, 0.0, 0.0);
    for id in group {
        if let Some(unit) = world.unit(*id) {
            x += f64::from(unit.tile().x);
            y += f64::from(unit.tile().y);
            n += 1.0;
        }
    }
    let (cx, cy) = (x / f64::max(n, 1.0), y / f64::max(n, 1.0));
    let step = round * GROUPS as u64 + index as u64;
    let angle = (seed as f64 + step as f64 * 2.399_963) % std::f64::consts::TAU;
    let straight = f64::from(DISTANCES[(step % DISTANCES.len() as u64) as usize]);
    let (width, height) = world.map_size();
    TileCoord::new(
        (cx + straight * angle.cos()).clamp(4.0, f64::from(width - 5)) as i32,
        (cy + straight * angle.sin()).clamp(4.0, f64::from(height - 5)) as i32,
    )
}

async fn run(url: &str, token: &str, seed: u64, stop: &AtomicBool) -> Result<Value, BoxError> {
    let options = ConnectOptions {
        decision_hz: 2.0,
        fetch_terrain: false,
        ..ConnectOptions::default()
    };
    let client = Client::connect(url, token, options).await?;
    let started = Instant::now();
    let mut verdicts = BTreeMap::<String, u64>::new();
    let mut ours = std::collections::BTreeSet::new();
    let (mut round, mut orders, mut largest) = (0u64, 0u64, 0usize);
    let mut next_round = Instant::now();
    let mut pending: Vec<(Vec<EntityId>, usize)> = Vec::new();
    let mut next_order = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let decision =
            match tokio::time::timeout(Duration::from_secs(2), client.next_decision()).await {
                Ok(Ok(decision)) => decision,
                Ok(Err(error)) => return Err(error.into()),
                Err(_) => continue,
            };
        for event in &decision.events {
            if let Event::OrderResult { sequence, result } = event
                && ours.remove(sequence)
            {
                *verdicts.entry(format!("{result:?}")).or_default() += 1;
            }
        }
        let world = &decision.world;
        if !world.ready() {
            continue;
        }
        if pending.is_empty() && Instant::now() >= next_round {
            let army = soldiers(world);
            largest = largest.max(army.len());
            let size = army.len().div_ceil(GROUPS).max(1);
            pending = army.chunks(size).map(|c| c.to_vec()).zip(0..).collect();
            pending.reverse();
            next_round = Instant::now() + ROUND;
            round += 1;
        }
        if Instant::now() >= next_order
            && let Some((group, index)) = pending.pop()
        {
            let to = goal(world, &group, round, index, seed);
            ours.insert(client.send(Order::FormationMove {
                units: group,
                formation: Formation::Line,
                to,
                queued: false,
                attack_move: true,
            }));
            orders += 1;
            next_order = Instant::now() + ORDER_GAP;
        }
    }
    client.close().await;
    Ok(json!({
        "rounds": round,
        "formation_orders": orders,
        "largest_army": largest,
        "verdicts": verdicts,
        "wall_s": started.elapsed().as_secs_f64(),
    }))
}
