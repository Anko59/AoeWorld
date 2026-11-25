//! The seats' state and the two points where a tick calls them.
//!
//! Determinism rests on where those points sit, not on timing:
//! - `collect` runs in tick `k` after the sessions lock was taken for the
//!   views, so the previous tick's delivery is complete: every seat has
//!   received exactly the messages through the view of tick `k`, in every
//!   mode (overlapped or not). On a decision tick (`k` a multiple of
//!   `tick_hz / decision_hz`) the brains run on worker threads, in parallel.
//! - `flush` runs in tick `k + 1` once its commands are fixed: it waits for
//!   the brains and sends their requests, in seat order, through the socket
//!   path. Those orders enter tick `k + 2`: always two ticks after the view
//!   they were decided from.
//!
//! Map chunks follow the same beat: fetched after one decision, inserted at
//! the next. The order budget is the session's, refilled by game ticks.
use super::Stats;
use crate::{AppState, GameplayService, terrain_cache};
use aoe_ai::{Brain, Config as AiConfig, Seat as OrderSeat};
use aoe_core::{PlayerId, TileCoord};
use aoe_map::CompactChunk;
use aoe_protocol::{GameplayClientMessage, GameplayServerMessage};
use aoe_sdk::{Event, Mirror, Order, ViewPolicy};
use rayon::prelude::*;
use std::time::Duration;
use tokio::{sync::mpsc, task::JoinHandle};

/// Map chunks one seat asks for per decision.
const CHUNKS_PER_DECISION: usize = 16;
/// Messages a seat's queue holds; drained every tick.
const SEAT_QUEUE: usize = 4_096;

type Fetch = JoinHandle<Result<CompactChunk, String>>;
type Decided = Vec<(Seat, Vec<GameplayClientMessage>)>;

pub(super) struct Seat {
    session: u64,
    receiver: mpsc::Receiver<GameplayServerMessage>,
    mirror: Mirror,
    events: Vec<Event>,
    brain: Brain,
    next_sequence: u64,
    fetching: Vec<((i32, i32), Fetch)>,
}

#[derive(Default)]
pub(super) struct Inner {
    seats: Vec<Seat>,
    players: Vec<PlayerId>,
    decision_ticks: u64,
    state: Option<AppState>,
    job: Option<JoinHandle<Decided>>,
}

/// What a brain sends: wire requests, numbered like a client's.
struct Orders {
    next: u64,
    messages: Vec<GameplayClientMessage>,
    refused: Vec<Event>,
    focus: Option<Option<(i32, i32)>>,
}

impl OrderSeat for Orders {
    fn focus_view(&mut self, tile: Option<(i32, i32)>) {
        self.focus = Some(tile);
    }

    fn send(&mut self, order: Order) -> u64 {
        let sequence = self.next;
        self.next += 1;
        match order.message(sequence) {
            Some(message) => self.messages.push(message),
            None => self.refused.push(Event::OrderResult {
                sequence,
                result: aoe_sdk::CommandResult::RejectedInvalidDestination,
            }),
        }
        sequence
    }
}

impl Seat {
    /// Applies what was delivered. Events wait for the next decision, which
    /// takes them all, so they hold at most one decision period's worth.
    fn feed(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            let events = self.mirror.apply(message);
            self.events.extend(events);
        }
    }

    /// Takes in the chunks asked for at the last decision, then asks for more.
    async fn chunks(&mut self, state: &AppState) {
        for (chunk, fetch) in std::mem::take(&mut self.fetching) {
            let result = fetch
                .await
                .map_err(|error| error.to_string())
                .and_then(|r| r);
            self.mirror.insert_chunk(chunk, result);
        }
        for (hash, chunk) in self.mirror.wanted_chunks(CHUNKS_PER_DECISION) {
            let state = state.clone();
            let key = terrain_cache::Key {
                content_hash: hash.iter().map(|byte| format!("{byte:02x}")).collect(),
                x: chunk.0,
                y: chunk.1,
            };
            self.fetching
                .push((chunk, tokio::spawn(async move { fetch(&state, key).await })));
        }
    }
}

/// A chunk from the same source as the chunk route; a refused admission is
/// retried, so the outcome does not depend on how busy the server is.
async fn fetch(state: &AppState, key: terrain_cache::Key) -> Result<CompactChunk, String> {
    loop {
        match crate::maps::terrain_chunk(state, key.clone(), false).await {
            Err(axum::http::StatusCode::SERVICE_UNAVAILABLE) => {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            other => return other.map_err(|code| code.to_string()),
        }
    }
}

/// CPU time of the calling thread.
#[cfg(unix)]
fn thread_cpu() -> Duration {
    use nix::time::{ClockId, clock_gettime};
    clock_gettime(ClockId::CLOCK_THREAD_CPUTIME_ID).map_or(Duration::ZERO, |time| {
        Duration::new(time.tv_sec().max(0) as u64, time.tv_nsec().max(0) as u32)
    })
}

#[cfg(not(unix))]
fn thread_cpu() -> Duration {
    Duration::ZERO
}

/// A running count of this thread's user-mode instructions; `None` where the
/// kernel or container gives no hardware counter.
fn instructions() -> Option<perf_event::Counter> {
    use perf_event::{Builder, events::Hardware};
    let mut counter = Builder::new(Hardware::INSTRUCTIONS)
        .observe_self()
        .any_cpu()
        .exclude_kernel(true)
        .exclude_hv(true)
        .build()
        .ok()?;
    counter.enable().ok()?;
    Some(counter)
}

fn decide(mut seat: Seat, stats: &Stats) -> (Seat, Vec<GameplayClientMessage>) {
    let started = thread_cpu();
    let mut out = Orders {
        next: seat.next_sequence,
        messages: Vec::new(),
        refused: Vec::new(),
        focus: None,
    };
    let events = std::mem::take(&mut seat.events);
    seat.brain.decide(seat.mirror.world(), &events, &mut out);
    seat.next_sequence = out.next;
    seat.events.extend(out.refused);
    if let Some(tile) = out.focus {
        let tile = tile.map(|(x, y)| TileCoord::new(x, y));
        seat.mirror.focus_view(tile, ViewPolicy::default());
    }
    let mut messages: Vec<_> = seat.mirror.take_subscribe().into_iter().collect();
    messages.extend(out.messages);
    stats.record(thread_cpu().saturating_sub(started));
    (seat, messages)
}

impl GameplayService {
    /// Seats the match's AI slots that are not seated yet; how many were.
    pub(super) async fn seat_ai(&self, state: AppState) -> usize {
        let mut inner = self.ai_seats.inner.lock().await;
        let mut seated = 0;
        for info in self.slot_infos().await.into_iter().filter(|s| !s.human) {
            if inner.players.contains(&info.player) {
                continue;
            }
            let (sender, receiver) = mpsc::channel(SEAT_QUEUE);
            let Ok((session, welcome)) = self.join_slot(info.token, sender).await else {
                continue;
            };
            let Some(mirror) = Mirror::new(&welcome) else {
                continue;
            };
            let hz = f64::from(mirror.world().tick_hz().max(1));
            let rate = AiConfig::default().decision_hz.max(0.05);
            inner.decision_ticks = ((hz / rate).round() as u64).max(1);
            let first = mirror.first_subscribe();
            self.start_command_clock(session).await;
            self.ai_seats.add_session(session);
            self.dispatch_client_message(session, first).await;
            inner.players.push(info.player);
            inner.seats.push(Seat {
                session,
                receiver,
                mirror,
                events: Vec::new(),
                brain: Brain::new(AiConfig::default()),
                next_sequence: 1,
                fetching: Vec::new(),
            });
            seated += 1;
        }
        inner.state = Some(state);
        self.ai_seats.active.store(
            !inner.players.is_empty(),
            std::sync::atomic::Ordering::Release,
        );
        seated
    }

    /// Called by tick `tick`, once the previous delivery is complete.
    pub(crate) async fn collect_ai_seats(&self, tick: u64) {
        if !self
            .ai_seats
            .active
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return;
        }
        let mut inner = self.ai_seats.inner.lock().await;
        // The seats' only work on the tick's thread: applying what was delivered.
        let (cpu, counter) = (thread_cpu(), instructions());
        for seat in &mut inner.seats {
            seat.feed();
        }
        let counted = counter.and_then(|mut c| c.disable().and_then(|()| c.read()).ok());
        self.ai_seats
            .stats
            .record_tick(thread_cpu().saturating_sub(cpu), counted);
        if inner.decision_ticks == 0 || !tick.is_multiple_of(inner.decision_ticks) {
            return;
        }
        let Some(state) = inner.state.clone() else {
            return;
        };
        for seat in &mut inner.seats {
            seat.chunks(&state).await;
        }
        let seats = std::mem::take(&mut inner.seats);
        let stats = self.ai_seats.stats.clone();
        inner.job = Some(tokio::task::spawn_blocking(move || {
            seats
                .into_par_iter()
                .map(|seat| decide(seat, &stats))
                .collect()
        }));
    }

    /// Called by the next tick once its commands are fixed: sends what the
    /// brains decided, in seat order.
    pub(crate) async fn flush_ai_seats(&self) {
        if !self
            .ai_seats
            .active
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return;
        }
        let mut inner = self.ai_seats.inner.lock().await;
        let Some(job) = inner.job.take() else {
            return;
        };
        let Ok(decided) = job.await else {
            tracing::error!("an AI decision failed; its seats stop");
            return;
        };
        let mut sent = false;
        for (seat, messages) in decided {
            let session = seat.session;
            inner.seats.push(seat);
            for message in messages {
                sent = true;
                let _ = self.dispatch_client_message(session, message).await;
            }
        }
        if sent {
            // Orders wait for their terrain; settle it so the tick that takes
            // them does not depend on thread timing.
            self.settle_preparation().await;
        }
    }
}
