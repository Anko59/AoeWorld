//! Actual protocol clients for scheduled pressure and lifecycle cases.
use super::{
    ClientResult, LATE_TOLERANCE_US, OFFER_PERIOD_MS, OFFERS_PER_CLIENT, RECONNECT_CYCLES, Result,
    SLOW_READER_PAUSE_MS, Sample,
};
use crate::perf;
use aoe_core::Region;
use aoe_protocol::{ClientMessage, ServerMessage, VERSION, decode_server, encode_client};
use aoe_scenario::NETWORK_PRESSURE;
use aoe_server::{AppState, Config, app};
use futures_util::{SinkExt, StreamExt, future::join_all};
use std::{
    error::Error,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{net::TcpListener, time::timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub(super) fn camera_region(index: u16, slot: u16) -> Region {
    let width = [128, 256, 512][usize::from(slot % 3)];
    let span = NETWORK_PRESSURE.world_size - 512;
    Region {
        x: (i32::from(index) * 157 + i32::from(slot) * 53) % span,
        y: (i32::from(index) * 263 + i32::from(slot) * 71) % span,
        width,
        height: width,
    }
}

async fn scheduled_client(address: SocketAddr, index: u16) -> ClientResult<Sample> {
    let (mut socket, _) = timeout(
        Duration::from_secs(20),
        connect_async(format!("ws://{address}/ws")),
    )
    .await??;
    perf::send(&mut socket, ClientMessage::Hello { version: VERSION }).await?;
    let (hello, hello_size) = perf::receive(&mut socket).await?;
    if !matches!(
        hello,
        ServerMessage::Hello {
            version: VERSION,
            ..
        }
    ) {
        return Err("pressure handshake mismatch".into());
    }
    let (mut sink, mut source) = socket.split();
    let started = Instant::now();
    let sent = Arc::new(AtomicU64::new(0));
    let received = Arc::new(AtomicU64::new(0));
    let send_times: Arc<[AtomicU64; OFFERS_PER_CLIENT as usize]> =
        Arc::new(std::array::from_fn(|_| AtomicU64::new(0)));
    let writer_sent = sent.clone();
    let writer_received = received.clone();
    let writer_times = send_times.clone();
    let writer = async move {
        let mut sample = Sample::default();
        for slot in 0..OFFERS_PER_CLIENT {
            let due = started + Duration::from_millis(OFFER_PERIOD_MS * u64::from(slot));
            tokio::time::sleep_until(due.into()).await;
            let lateness = Instant::now()
                .saturating_duration_since(due)
                .as_micros()
                .min(u128::from(u64::MAX)) as u64;
            sample.offered_subscriptions += 1;
            sample.max_offer_lateness_us = sample.max_offer_lateness_us.max(lateness);
            sample.missed_offer_deadlines += u64::from(lateness > LATE_TOLERANCE_US);
            writer_times[usize::from(slot)]
                .store(started.elapsed().as_micros() as u64, Ordering::Release);
            let bytes = encode_client(&ClientMessage::Subscribe {
                region: camera_region(index, slot),
            })?;
            sink.send(Message::Binary(bytes.into())).await?;
            sample.sent_subscriptions += 1;
            writer_sent.store(sample.sent_subscriptions, Ordering::Release);
            sample.max_response_backlog = sample.max_response_backlog.max(
                sample
                    .sent_subscriptions
                    .saturating_sub(writer_received.load(Ordering::Acquire)),
            );
        }
        Ok::<Sample, Box<dyn Error + Send + Sync>>(sample)
    };
    let reader = async move {
        let mut sample = Sample {
            clients_connected: 1,
            encoded_bytes: hello_size as u64,
            ..Default::default()
        };
        while sample.snapshots < u64::from(OFFERS_PER_CLIENT) {
            let frame = timeout(Duration::from_secs(20), source.next())
                .await?
                .ok_or("pressure connection closed")??;
            let Message::Binary(bytes) = frame else {
                return Err("nonbinary pressure frame".into());
            };
            sample.encoded_bytes += bytes.len() as u64;
            match decode_server(&bytes)? {
                ServerMessage::Snapshot {
                    region,
                    entities,
                    total_entities,
                    ..
                } => {
                    let slot = u16::try_from(sample.snapshots)?;
                    if total_entities != NETWORK_PRESSURE.entities
                        || region != camera_region(index, slot)
                        || entities
                            .iter()
                            .any(|entity| !region.contains(entity.position))
                    {
                        return Err("pressure snapshot population or region mismatch".into());
                    }
                    sample.replicated_entities += entities.len() as u64;
                    sample.max_visible_entities = sample.max_visible_entities.max(entities.len());
                    sample.snapshots += 1;
                    received.store(sample.snapshots, Ordering::Release);
                    let sent_at = send_times[usize::from(slot)].load(Ordering::Acquire);
                    sample
                        .snapshot_latency_us
                        .push((started.elapsed().as_micros() as u64).saturating_sub(sent_at));
                }
                ServerMessage::Delta { .. } => sample.deltas += 1,
                _ => return Err("pressure protocol error".into()),
            }
            sample.max_response_backlog = sample.max_response_backlog.max(
                sent.load(Ordering::Acquire)
                    .saturating_sub(sample.snapshots),
            );
        }
        Ok::<Sample, Box<dyn Error + Send + Sync>>(sample)
    };
    let (writer, reader) = tokio::try_join!(writer, reader)?;
    Ok(Sample {
        clients_connected: reader.clients_connected,
        offered_subscriptions: writer.offered_subscriptions,
        sent_subscriptions: writer.sent_subscriptions,
        snapshots: reader.snapshots,
        deltas: reader.deltas,
        missed_offer_deadlines: writer.missed_offer_deadlines,
        max_offer_lateness_us: writer.max_offer_lateness_us,
        max_response_backlog: writer.max_response_backlog.max(reader.max_response_backlog),
        replicated_entities: reader.replicated_entities,
        max_visible_entities: reader.max_visible_entities,
        encoded_bytes: reader.encoded_bytes,
        snapshot_latency_us: reader.snapshot_latency_us,
        ..Default::default()
    })
}

async fn reconnects(address: SocketAddr) -> ClientResult<u16> {
    for cycle in 0..RECONNECT_CYCLES {
        let (mut socket, _) = connect_async(format!("ws://{address}/ws")).await?;
        perf::send(&mut socket, ClientMessage::Hello { version: VERSION }).await?;
        if !matches!(
            perf::receive(&mut socket).await?.0,
            ServerMessage::Hello { .. }
        ) {
            return Err("reconnect handshake missing".into());
        }
        perf::send(
            &mut socket,
            ClientMessage::Subscribe {
                region: camera_region(cycle + 1, 0),
            },
        )
        .await?;
        if !matches!(
            perf::receive(&mut socket).await?.0,
            ServerMessage::Snapshot { .. }
        ) {
            return Err("reconnect snapshot missing".into());
        }
        socket.close(None).await?;
    }
    Ok(RECONNECT_CYCLES)
}

async fn slow_reader(address: SocketAddr) -> ClientResult<(u16, u16)> {
    let (mut socket, _) = connect_async(format!("ws://{address}/ws")).await?;
    perf::send(&mut socket, ClientMessage::Hello { version: VERSION }).await?;
    if !matches!(
        perf::receive(&mut socket).await?.0,
        ServerMessage::Hello { .. }
    ) {
        return Err("slow reader handshake missing".into());
    }
    perf::send(
        &mut socket,
        ClientMessage::Subscribe {
            region: camera_region(0, 0),
        },
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(SLOW_READER_PAUSE_MS)).await;
    let ServerMessage::Snapshot {
        total_entities,
        entities,
        ..
    } = perf::receive(&mut socket).await?.0
    else {
        return Err("slow reader snapshot missing".into());
    };
    if total_entities != NETWORK_PRESSURE.entities
        || entities.len() < NETWORK_PRESSURE.hotspot_entities as usize
    {
        return Err("slow reader lost hotspot population".into());
    }
    if !matches!(
        perf::receive(&mut socket).await?.0,
        ServerMessage::Delta { .. }
    ) {
        return Err("slow reader delta missing".into());
    }
    Ok((1, 1))
}

pub(super) async fn network() -> Result<Sample> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let config = Config {
        bind: address,
        scenario: NETWORK_PRESSURE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: None,
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    };
    let state = AppState::new(&config, "perf-pressure-local")?;
    let ticker = tokio::spawn(state.clone().run_ticks());
    let server_state = state.clone();
    let server = tokio::spawn(async move { axum::serve(listener, app(server_state)).await });
    let scheduled =
        join_all((0..NETWORK_PRESSURE.players).map(|index| scheduled_client(address, index)));
    let (clients, reconnect_result, slow_result) =
        tokio::join!(scheduled, reconnects(address), slow_reader(address));
    ticker.abort();
    server.abort();
    let mut combined = Sample {
        reconnects: reconnect_result.map_err(|error| error.to_string())?,
        tick_deadline_misses: state.tick_deadline_misses(),
        ..Default::default()
    };
    (combined.slow_reader_snapshots, combined.slow_reader_deltas) =
        slow_result.map_err(|error| error.to_string())?;
    for client in clients {
        let value = client.map_err(|error| error.to_string())?;
        combined.clients_connected += value.clients_connected;
        combined.offered_subscriptions += value.offered_subscriptions;
        combined.sent_subscriptions += value.sent_subscriptions;
        combined.snapshots += value.snapshots;
        combined.deltas += value.deltas;
        combined.missed_offer_deadlines += value.missed_offer_deadlines;
        combined.max_offer_lateness_us = combined
            .max_offer_lateness_us
            .max(value.max_offer_lateness_us);
        combined.max_response_backlog = combined
            .max_response_backlog
            .max(value.max_response_backlog);
        combined.replicated_entities += value.replicated_entities;
        combined.max_visible_entities = combined
            .max_visible_entities
            .max(value.max_visible_entities);
        combined.encoded_bytes += value.encoded_bytes;
        combined
            .snapshot_latency_us
            .extend(value.snapshot_latency_us);
    }
    Ok(combined)
}
