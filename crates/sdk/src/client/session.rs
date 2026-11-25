//! The connection task: a session per socket, rejoined with the slot token when lost.
use super::*;

/// Reconnection attempts after a lost connection, with growing pauses.
const RECONNECT_ATTEMPTS: usize = 20;

/// Why a session ended, and whether the slot may be rejoined.
struct Ended {
    reason: String,
    retry: bool,
    /// The order that was being written when the connection failed.
    unsent: Vec<u64>,
}

/// An order that never reached the server gets a failure result, never silence.
fn not_sent(shared: &Shared, sequence: u64) {
    shared.lock().events.push(Event::OrderResult {
        sequence,
        result: crate::model::CommandResult::RejectedDisconnected,
    });
}

pub(super) async fn run(
    socket: Socket,
    shared: Arc<Shared>,
    mut commands: mpsc::UnboundedReceiver<Command>,
    endpoint: Endpoint,
    options: ConnectOptions,
    token: ResumeToken,
) {
    let mut socket = socket;
    let reason = loop {
        let ended = session(socket, &shared, &mut commands, &endpoint, &options).await;
        if !ended.retry {
            break ended.reason;
        }
        // The connection was lost (a slow server tick, a reset, a replaced
        // session): rejoin with the slot token and take a fresh snapshot.
        shared.drops.fetch_add(1, Ordering::Relaxed);
        let lost_at = Instant::now();
        for sequence in ended.unsent {
            not_sent(&shared, sequence);
        }
        let rejoined = rejoin(&shared, &mut commands, &endpoint, &options, token).await;
        let away = lost_at.elapsed().as_micros() as u64;
        shared.reconnect_micros.fetch_add(away, Ordering::Relaxed);
        match rejoined {
            Some(next) => {
                shared.rejoins.fetch_add(1, Ordering::Relaxed);
                socket = next;
            }
            None => break format!("{}; the slot could not be rejoined", ended.reason),
        }
    };
    shared.lock().closed = Some(reason);
    shared.notify.notify_waiters();
}

async fn rejoin(
    shared: &Shared,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    endpoint: &Endpoint,
    options: &ConnectOptions,
    token: ResumeToken,
) -> Option<Socket> {
    for attempt in 0..RECONNECT_ATTEMPTS {
        tokio::time::sleep(Duration::from_millis(250 * (1 << attempt.min(4)))).await;
        // Commands issued while away: a new view policy is kept for the
        // rejoined session, orders (which refer to a session that is gone)
        // report that they were not sent.
        while let Ok(command) = commands.try_recv() {
            match command {
                Command::Close => return None,
                Command::View(next) => shared.lock().mirror.set_policy(next),
                Command::Order(sequence, _) => not_sent(shared, sequence),
            }
        }
        let Ok((mut socket, welcome)) = join(endpoint, token, options.connect_timeout).await else {
            continue;
        };
        let Some(mut mirror) = Mirror::new(&welcome) else {
            continue;
        };
        mirror.set_policy(shared.lock().mirror.policy());
        if send_frame(&mut socket, &mirror.first_subscribe())
            .await
            .is_err()
        {
            continue;
        }
        shared.lock().mirror = mirror;
        shared.notify.notify_waiters();
        return Some(socket);
    }
    None
}

async fn session(
    mut socket: Socket,
    shared: &Arc<Shared>,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    endpoint: &Endpoint,
    options: &ConnectOptions,
) -> Ended {
    let (chunk_tx, mut chunk_rx) = mpsc::unbounded_channel::<((i32, i32), Result<CompactChunk>)>();
    let mut in_flight = 0_usize;
    let (mut window_start, mut window_sent) = (Instant::now(), 0_usize);
    let mut unsent = Vec::new();
    let (reason, retry) = loop {
        tokio::select! {
            frame = socket.next() => {
                let message = match frame {
                    Some(Ok(Message::Binary(bytes))) => match decode_gameplay_server(&bytes) {
                        Ok(message) => message,
                        Err(error) => break (format!("undecodable server message: {error}"), true),
                    },
                    Some(Ok(Message::Close(_))) | None => break ("server closed the connection".to_owned(), true),
                    Some(Ok(_)) => continue,
                    Some(Err(error)) => break (format!("websocket error: {error}"), true),
                };
                let reset = matches!(message, GameplayServerMessage::WorldReset { .. });
                let subscribe;
                let mut wanted = Vec::new();
                {
                    let mut state = shared.lock();
                    let events = state.mirror.apply(message);
                    state.events.extend(events);
                    let overflow = state.events.len().saturating_sub(MAX_BUFFERED_EVENTS);
                    state.events.drain(..overflow);
                    subscribe = state.mirror.take_subscribe();
                    if options.fetch_terrain {
                        wanted = state.mirror.wanted_chunks(MAX_CHUNKS_IN_FLIGHT.saturating_sub(in_flight));
                    }
                }
                shared.notify.notify_waiters();
                for (hash, chunk) in wanted {
                    in_flight += 1;
                    let (endpoint, sender) = (endpoint.clone(), chunk_tx.clone());
                    tokio::spawn(async move {
                        let path = format!("/maps/{}/chunks/{}/{}", hex(&hash), chunk.0, chunk.1);
                        let _ = sender.send((chunk, endpoint.get_json::<CompactChunk>(&path).await));
                    });
                }
                if let Some(subscribe) = subscribe
                    && let Err(error) = send_frame(&mut socket, &subscribe).await
                {
                    break (error.to_string(), true);
                }
                if reset {
                    break ("the server started a new world".to_owned(), false);
                }
            }
            command = commands.recv() => match command {
                None | Some(Command::Close) => break ("closed by the client".to_owned(), false),
                Some(Command::View(next)) => {
                    shared.lock().mirror.set_policy(next);
                }
                Some(Command::Order(sequence, order)) => {
                    if window_start.elapsed() >= Duration::from_secs(1) {
                        (window_start, window_sent) = (Instant::now(), 0);
                    }
                    if window_sent >= options.orders_per_second {
                        tokio::time::sleep_until(window_start + Duration::from_secs(1)).await;
                        (window_start, window_sent) = (Instant::now(), 0);
                    }
                    window_sent += 1;
                    let Some(message) = order.message(sequence) else {
                        shared.lock().events.push(Event::OrderResult {
                            sequence,
                            result: crate::model::CommandResult::RejectedInvalidDestination,
                        });
                        continue;
                    };
                    if let Err(error) = send_frame(&mut socket, &message).await {
                        unsent.push(sequence);
                        break (error.to_string(), true);
                    }
                }
            },
            fetched = chunk_rx.recv() => {
                let Some((chunk, result)) = fetched else { continue };
                in_flight = in_flight.saturating_sub(1);
                shared
                    .lock()
                    .mirror
                    .insert_chunk(chunk, result.map_err(|error| error.to_string()));
            }
        }
    };
    Ended {
        reason,
        retry,
        unsent,
    }
}
