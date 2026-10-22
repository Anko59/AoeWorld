//! The server half: real `GameplayService` ticks with many subscribed sessions.
use super::{
    stats::{Series, memory, round},
    world::{BoxError, Player},
};
use aoe_core::{TileCoord, TileRect};
use aoe_protocol::{GameplayServerMessage, encode_gameplay_server};
use aoe_server::{GameplayService, SlotInfo};
use aoe_simulation::GameWorld;
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Instant};
use tokio::sync::mpsc;

const VIEW_TILES: i32 = 96;

fn kind(message: &GameplayServerMessage) -> &'static str {
    match message {
        GameplayServerMessage::Welcome { .. } => "welcome",
        GameplayServerMessage::Snapshot { .. } => "snapshot",
        GameplayServerMessage::Tick { .. } => "tick_delta",
        GameplayServerMessage::FogState(_) => "fog_state",
        GameplayServerMessage::WildlifeState { .. } => "wildlife",
        GameplayServerMessage::Projectiles { .. } => "projectiles",
        GameplayServerMessage::CommandAck { .. } => "command_ack",
        _ => "other",
    }
}

#[derive(Default)]
struct Traffic {
    bytes: BTreeMap<&'static str, u64>,
    messages: BTreeMap<&'static str, u64>,
    encode_ms: Series,
    fanout: Series,
    fanout_build: Series,
    fanout_deliver: Series,
    changed_units: u64,
    per_session: BTreeMap<usize, u64>,
}

fn drain(receivers: &mut [mpsc::Receiver<GameplayServerMessage>], traffic: &mut Traffic) {
    let started = Instant::now();
    for (index, receiver) in receivers.iter_mut().enumerate() {
        while let Ok(message) = receiver.try_recv() {
            if let GameplayServerMessage::Tick { changed_units, .. } = &message {
                traffic.changed_units += changed_units.len() as u64;
            }
            let Ok(bytes) = encode_gameplay_server(&message) else {
                *traffic.messages.entry("encode_failed").or_default() += 1;
                continue;
            };
            let name = kind(&message);
            *traffic.bytes.entry(name).or_default() += bytes.len() as u64;
            *traffic.messages.entry(name).or_default() += 1;
            *traffic.per_session.entry(index).or_default() += bytes.len() as u64;
        }
    }
    traffic.encode_ms.push_ms(started.elapsed().as_nanos());
}

fn region(center: TileCoord, side: i32) -> TileRect {
    let clamp = |v: i32| v.clamp(0, side - VIEW_TILES);
    let min = TileCoord::new(
        clamp(center.x - VIEW_TILES / 2),
        clamp(center.y - VIEW_TILES / 2),
    );
    TileRect::new(min, TileCoord::new(min.x + VIEW_TILES, min.y + VIEW_TILES))
}

async fn ticks(
    service: &GameplayService,
    count: u32,
    receivers: &mut [mpsc::Receiver<GameplayServerMessage>],
    traffic: &mut Traffic,
) -> Series {
    let mut series = Series::default();
    for _ in 0..count {
        let started = Instant::now();
        service.tick().await;
        series.push_ms(started.elapsed().as_nanos());
        let (build, deliver) = service.last_fanout_timing();
        traffic.fanout_build.0.push(build);
        traffic.fanout_deliver.0.push(deliver);
        traffic.fanout.0.push(build + deliver);
        drain(receivers, traffic);
    }
    series
}

async fn session_run(
    service: &GameplayService,
    players: &[Player],
    infos: &[SlotInfo],
    side: i32,
    sessions: u16,
    count: u32,
    label: &str,
) -> Result<Value, BoxError> {
    let mut receivers = Vec::new();
    let mut traffic = Traffic::default();
    let subscribe_started = Instant::now();
    for index in 0..usize::from(sessions) {
        let (sender, receiver) = mpsc::channel(8192);
        // One session per seat, as in a match; extra sessions only watch.
        let id = match infos.get(index) {
            Some(info) => service
                .join_slot(info.token, sender)
                .await
                .map(|(id, _)| id)
                .map_err(|_| "slot token refused")?,
            None => service.register(None, sender).await.0,
        };
        let center = players[index % players.len()].center;
        service.subscribe(id, 1, region(center, side)).await;
        receivers.push(receiver);
    }
    let subscribe_ms = subscribe_started.elapsed().as_secs_f64() * 1e3;
    drain(&mut receivers, &mut traffic);
    let snapshot_bytes = traffic.bytes.get("snapshot").copied().unwrap_or(0);
    let snapshot_count = traffic.messages.get("snapshot").copied().unwrap_or(0);
    let initial = Traffic {
        bytes: std::mem::take(&mut traffic.bytes),
        messages: std::mem::take(&mut traffic.messages),
        ..Traffic::default()
    };
    let series = ticks(service, count, &mut receivers, &mut traffic).await;
    let seconds = f64::from(count) / 20.0;
    let steady_bytes: u64 = traffic.bytes.values().sum();
    let mut busiest: Vec<u64> = traffic.per_session.values().copied().collect();
    busiest.sort_unstable();
    Ok(json!({
        "mode": label,
        "sessions": sessions,
        "subscribe_all_ms": round(subscribe_ms),
        "initial_messages": initial.messages,
        "initial_bytes": initial.bytes,
        "mean_snapshot_bytes": snapshot_bytes.checked_div(snapshot_count).unwrap_or(0),
        "tick": series.summary(),
        "fanout_per_tick": traffic.fanout.summary(),
        "fanout_build_per_tick": traffic.fanout_build.summary(),
        "fanout_deliver_per_tick": traffic.fanout_deliver.summary(),
        "drain_and_encode_per_tick": traffic.encode_ms.summary(),
        "steady_state_messages": traffic.messages,
        "steady_state_bytes": traffic.bytes,
        "steady_bytes_per_second_all_sessions": round(steady_bytes as f64 / seconds),
        "steady_bytes_per_second_per_session_mean": round(steady_bytes as f64 / seconds / f64::from(sessions.max(1))),
        "steady_bytes_per_second_per_session_max": round(busiest.last().copied().unwrap_or(0) as f64 / seconds),
        "mean_changed_units_per_tick_per_session": round(traffic.changed_units as f64 / f64::from(count.max(1)) / f64::from(sessions.max(1))),
    }))
}

pub(super) fn measure(
    world: GameWorld,
    players: &[Player],
    count: u32,
    sessions: u16,
) -> Result<Value, BoxError> {
    let side = world.config().width_tiles;
    let units = world.unit_count();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        // One person and the rest AI processes, the promised match.
        let seats = players
            .iter()
            .enumerate()
            .map(|(index, player)| (player.id, player.primary, index == 0));
        let service = GameplayService::from_scale_world_slots(world, seats);
        let infos = service.slot_infos().await;
        let mut none = Vec::new();
        let mut nothing = Traffic::default();
        let baseline = ticks(&service, count, &mut none, &mut nothing).await;
        let on = session_run(&service, players, &infos, side, sessions, count, "fog_on").await?;
        service.set_fog_disabled(true).await;
        let off = session_run(&service, players, &infos, side, sessions, count, "fog_off").await?;
        Ok(json!({
            "note": "one slot per player (player 0 human, the rest AI); session i controls player i and sees a 96 x 96 tile view of its own base through its own fog",
            "world_units": units,
            "view_tiles": VIEW_TILES,
            "ticks_per_mode": count,
            "baseline_no_sessions_tick": baseline.summary(),
            "with_sessions_fog_on": on,
            "with_sessions_fog_off": off,
            "memory": memory(),
        }))
    })
}
