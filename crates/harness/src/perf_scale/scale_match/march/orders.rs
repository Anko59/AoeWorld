//! The probe's play: scatter, wait for demand, plan, order, observe.
use super::{
    Admin, BoxError, PLAN, Settings,
    track::{Member, Stamp},
    wall,
};
use aoe_sdk::{
    Client, ConnectOptions, EntityId, Event, Formation, Order, TileCoord, UnitKind, World,
};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// Seconds between the formation orders (a person's pace).
const ORDER_GAP: Duration = Duration::from_secs(5);
/// Scheduler snapshots are taken this often (wall seconds).
const SCHEDULER_EVERY: f64 = 30.0;
/// Destination directions tried per group before it is labelled unreachable.
const DIRECTIONS: usize = 8;

/// A small deterministic generator (the probe's choices are reproducible).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn range(&mut self, span: i32) -> i32 {
        (self.next() % (2 * span as u64 + 1)) as i32 - span
    }
}

fn siege(kind: UnitKind) -> bool {
    matches!(kind, UnitKind::BatteringRam | UnitKind::Mangonel)
}

/// What the browser would put in a formation order: own living military,
/// no villagers, Generals or ships.
fn military(world: &World) -> Vec<(EntityId, UnitKind)> {
    let mut all: Vec<_> = world
        .my_units(None)
        .into_iter()
        .filter(|u| u.is_military() && u.kind() != UnitKind::General && !u.kind().is_naval())
        .map(|u| (u.id(), u.kind()))
        .collect();
    all.sort_unstable_by_key(|(id, _)| *id);
    all
}

struct Group {
    size: usize,
    straight: i32,
    siege: bool,
    ids: Vec<EntityId>,
    probe: Value,
    to: TileCoord,
    issued: Option<Stamp>,
    sequence: u64,
    result: Option<String>,
    members: Vec<Member>,
}

fn stamp(world: &World, start: Instant) -> Stamp {
    Stamp {
        tick: world.tick(),
        wall: wall(start),
    }
}

fn centroid(world: &World, ids: &[EntityId]) -> (f64, f64) {
    let (mut x, mut y) = (0.0, 0.0);
    for id in ids {
        if let Some(unit) = world.unit(*id) {
            x += f64::from(unit.tile().x);
            y += f64::from(unit.tile().y);
        }
    }
    let n = ids.len().max(1) as f64;
    (x / n, y / n)
}

/// Picks a destination `straight` tiles away whose route the server's probe
/// proves; the last answer is kept (labelled) when no direction is reachable.
fn destination(admin: &Admin, world: &World, group: &mut Group, turn: usize) {
    let (cx, cy) = centroid(world, &group.ids);
    let (width, height) = world.map_size();
    let units: Vec<u32> = group.ids.iter().map(|id| id.0).collect();
    for attempt in 0..DIRECTIONS {
        let angle = (turn as f64 * 2.399 + attempt as f64 * std::f64::consts::FRAC_PI_4)
            % std::f64::consts::TAU;
        let x = (cx + f64::from(group.straight) * angle.cos()).clamp(4.0, f64::from(width - 5));
        let y = (cy + f64::from(group.straight) * angle.sin()).clamp(4.0, f64::from(height - 5));
        let to = TileCoord::new(x as i32, y as i32);
        let body = json!({"player": 0, "units": units, "to": [to.x, to.y]});
        let answer = admin
            .post("/admin/match/route_probe", &body)
            .unwrap_or_else(|error| json!({"outcome": error.to_string(), "reachable": false}));
        group.to = to;
        let reachable = answer["reachable"].as_bool() == Some(true);
        group.probe = answer;
        group.probe["attempts"] = json!(attempt + 1);
        if reachable {
            return;
        }
    }
}

async fn wait_ready(client: &Client) -> Result<World, BoxError> {
    let started = Instant::now();
    loop {
        let world = client.world();
        if world.ready() && !military(&world).is_empty() {
            return Ok(world);
        }
        if started.elapsed() > Duration::from_secs(300) {
            return Err("slot 0 saw no army within 300 s".into());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Every soldier walks to a random spot near where it stands, so the
/// groups are later selected scattered, as an owner's army stands.
fn scatter(client: &Client, world: &World, rng: &mut Rng, radius: i32) -> usize {
    let (width, height) = world.map_size();
    let mut sent = 0;
    for (id, _) in military(world) {
        let Some(unit) = world.unit(id) else { continue };
        let at = unit.tile();
        let to = TileCoord::new(
            (at.x + rng.range(radius)).clamp(1, width - 2),
            (at.y + rng.range(radius)).clamp(1, height - 2),
        );
        client.send(Order::Move { unit: id, to });
        sent += 1;
    }
    sent
}

fn form_groups(world: &World, rng: &mut Rng) -> Vec<Group> {
    let mut pool: Vec<_> = military(world);
    // Fisher-Yates: groups are drawn from wherever the soldiers stand.
    for i in (1..pool.len()).rev() {
        pool.swap(i, (rng.next() % (i as u64 + 1)) as usize);
    }
    let (mut engines, mut soldiers): (Vec<_>, Vec<_>) = pool.into_iter().partition(|u| siege(u.1));
    PLAN.iter()
        .map(|&(size, straight, with_siege)| {
            let mut ids = Vec::new();
            if with_siege {
                let take = (size / 2).min(engines.len());
                ids.extend(engines.drain(..take).map(|u| u.0));
            }
            let take = (size - ids.len()).min(soldiers.len());
            ids.extend(soldiers.drain(..take).map(|u| u.0));
            ids.sort_unstable();
            Group {
                size,
                straight,
                siege: with_siege,
                ids,
                probe: Value::Null,
                to: TileCoord::new(0, 0),
                issued: None,
                sequence: 0,
                result: None,
                members: Vec::new(),
            }
        })
        .collect()
}

fn scheduler(admin: &Admin, start: Instant) -> Value {
    let mut value = admin
        .get("/admin/match/routes")
        .unwrap_or_else(|error| json!({"error": error.to_string()}));
    value["wall_s"] = json!(wall(start));
    value
}

pub(super) async fn run(settings: Settings, stop: Arc<AtomicBool>) -> Result<Value, BoxError> {
    let start = Instant::now();
    let options = ConnectOptions {
        decision_hz: 2.0,
        fetch_terrain: false,
        ..ConnectOptions::default()
    };
    let client = Client::connect(&settings.url, &settings.token, options).await?;
    let world = wait_ready(&client).await?;
    let mut rng = Rng(settings.seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let scattered = scatter(&client, &world, &mut rng, settings.scatter);
    let mut timeline = Vec::new();
    while start.elapsed() < settings.march_after && !stop.load(Ordering::Relaxed) {
        if timeline.last().is_none_or(|s: &Value| {
            wall(start) - s["wall_s"].as_f64().unwrap_or(0.0) >= SCHEDULER_EVERY
        }) {
            timeline.push(scheduler(&settings.admin, start));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let world = client.world();
    let at_march = scheduler(&settings.admin, start);
    let mut groups = form_groups(&world, &mut rng);
    for (turn, group) in groups.iter_mut().enumerate() {
        destination(&settings.admin, &client.world(), group, turn);
    }
    let mut next_order = Instant::now();
    let mut issued = 0;
    while !stop.load(Ordering::Relaxed) {
        let decision =
            match tokio::time::timeout(Duration::from_secs(5), client.next_decision()).await {
                Ok(Ok(decision)) => decision,
                Ok(Err(error)) => return Err(error.into()),
                Err(_) => continue,
            };
        let now = stamp(&decision.world, start);
        for event in &decision.events {
            if let Event::OrderResult { sequence, result } = event
                && let Some(group) = groups.iter_mut().find(|g| g.sequence == *sequence)
            {
                group.result = Some(format!("{result:?}"));
            }
        }
        if issued < groups.len() && Instant::now() >= next_order {
            let group = &mut groups[issued];
            let world = &decision.world;
            let speeds = group.probe["speeds_tiles_per_tick"].clone();
            group.members = group
                .ids
                .iter()
                .filter_map(|&id| {
                    let kind = world.unit(id)?.kind();
                    let nominal = speeds[format!("{kind:?}")].as_f64().unwrap_or(0.0);
                    Member::new(world, id, nominal)
                })
                .collect();
            group.sequence = client.send(Order::FormationMove {
                units: group.ids.clone(),
                formation: Formation::Line,
                to: group.to,
                queued: false,
                attack_move: false,
            });
            group.issued = Some(now);
            issued += 1;
            next_order = Instant::now() + ORDER_GAP;
        }
        for group in groups.iter_mut() {
            let Some(order) = group.issued else { continue };
            let goal = (f64::from(group.to.x) + 0.5, f64::from(group.to.y) + 0.5);
            let tolerance = group.probe["extent_tiles"].as_f64().unwrap_or(0.0) + 3.0;
            for member in &mut group.members {
                member.observe(&decision.world, now, order, goal, tolerance);
            }
        }
        if timeline.last().is_none_or(|s: &Value| {
            wall(start) - s["wall_s"].as_f64().unwrap_or(0.0) >= SCHEDULER_EVERY
        }) {
            timeline.push(scheduler(&settings.admin, start));
        }
    }
    let end = stamp(&client.world(), start);
    let at_end = scheduler(&settings.admin, start);
    client.close().await;
    Ok(json!({
        "ai": settings.ai,
        "scattered_orders": scattered,
        "march_after_s": settings.march_after.as_secs_f64(),
        "scheduler_at_march": at_march,
        "scheduler_at_end": at_end,
        "scheduler_timeline": timeline,
        "end": {"tick": end.tick, "wall_s": end.wall},
        "orders": groups.iter().map(|g| super::table::order_json(g.size, g.straight, g.siege,
            &g.ids, &g.probe, g.to, g.issued, g.result.as_deref(), &g.members, end)).collect::<Vec<_>>(),
    }))
}
