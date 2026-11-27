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
//!
//! What the seats cost the tick's thread is waiting, and each wait is as
//! short as determinism allows (`Stats`, `Wait`, reported by `/admin/perf`):
//! - `feed`: applying the delivery. It must happen on the tick, before the
//!   decision reads the mirror.
//! - `chunk_waits`: a seat whose chunks have all arrived takes them in on the
//!   tick's thread, without waiting. One whose chunks are still loading
//!   hands that step to its decision job (`chunks_late`): the step still runs
//!   before the decision, in the same order, so the mirror the brain reads is
//!   the same, but the wait is on a worker, not on the tick.
//! - `join`: the decisions of tick `k` are awaited by tick `k + 1`, after tick
//!   `k`'s fan-out and delivery have overlapped them. It cannot move later in
//!   the tick: the subscriptions the seats send shape the views this tick
//!   builds, and the orders must be queued before the next tick's intake.
//! - `settle`: the terrain warm-ups of the orders just sent must be done
//!   before the next intake, or the tick that applies an order would depend on
//!   thread timing. The wait starts as late as that allows (`collect`, after
//!   the world stepped) so it overlaps the tick's own work.
use super::{Stats, Wait};
use crate::{AppState, GameplayService, gameplay::replay::counter, terrain_cache};
use aoe_ai::{Brain, Config as AiConfig, Seat as OrderSeat};
use aoe_core::{PlayerId, TileCoord};
use aoe_map::CompactChunk;
use aoe_protocol::{GameplayClientMessage, GameplayServerMessage};
use aoe_sdk::{Event, Mirror, Order, ViewPolicy};
use rayon::prelude::*;
use std::time::{Duration, Instant};
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
    /// The chunk step of this decision is left to the decision job.
    chunks_late: bool,
}

#[derive(Default)]
pub(super) struct Inner {
    seats: Vec<Seat>,
    players: Vec<PlayerId>,
    decision_ticks: u64,
    state: Option<AppState>,
    job: Option<JoinHandle<Decided>>,
    /// The terrain warm-ups launched by the orders the last flush sent.
    settling: Option<JoinHandle<()>>,
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

    /// Whether every chunk asked for at the last decision has been loaded.
    fn chunks_arrived(&self) -> bool {
        self.fetching.iter().all(|(_, fetch)| fetch.is_finished())
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

/// A chunk from the same source as the chunk route. A busy preparation queue
/// makes it wait its turn rather than be refused, so every fetch completes in
/// bounded time and the outcome does not depend on how busy the server is.
async fn fetch(state: &AppState, key: terrain_cache::Key) -> Result<CompactChunk, String> {
    crate::maps::terrain_chunk_queued(state, key)
        .await
        .map_err(|code| code.to_string())
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

fn decide(
    mut seat: Seat,
    stats: &Stats,
    chunks: &(tokio::runtime::Handle, AppState),
) -> (Seat, Vec<GameplayClientMessage>) {
    if std::mem::take(&mut seat.chunks_late) {
        // The chunks were still loading on the tick: wait for them here, on a
        // worker, then ask for more, exactly as the tick would have.
        chunks.0.block_on(seat.chunks(&chunks.1));
    }
    let started = thread_cpu();
    let mut out = Orders {
        next: seat.next_sequence,
        messages: Vec::new(),
        refused: Vec::new(),
        focus: None,
    };
    let events = std::mem::take(&mut seat.events);
    seat.brain.decide(seat.mirror.world(), &events, &mut out);
    stats.counters(seat.session, &seat.brain.board.stats);
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
                chunks_late: false,
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
        let stats = self.ai_seats.stats.clone();
        // The orders of the last flush launched terrain warm-ups: they must be
        // done before the next tick takes its commands. Awaited here, after the
        // world stepped, so they ran during the tick's own work.
        if let Some(settling) = inner.settling.take() {
            let started = Instant::now();
            let _ = settling.await;
            stats.wait(Wait::Settle, started.elapsed());
        }
        // The seats' work on the tick's thread: applying what was delivered.
        let (started, cpu, counter) = (Instant::now(), thread_cpu(), counter::running());
        for seat in &mut inner.seats {
            seat.feed();
        }
        let counted = counter.and_then(|mut c| c.disable().and_then(|()| c.read()).ok());
        stats.record_tick(thread_cpu().saturating_sub(cpu), counted);
        stats.wait(Wait::Feed, started.elapsed());
        if inner.decision_ticks == 0 || !tick.is_multiple_of(inner.decision_ticks) {
            return;
        }
        let Some(state) = inner.state.clone() else {
            return;
        };
        let started = Instant::now();
        for seat in &mut inner.seats {
            if seat.chunks_arrived() {
                seat.chunks(&state).await;
            } else {
                seat.chunks_late = true;
            }
        }
        stats.wait(Wait::Chunks, started.elapsed());
        let seats = std::mem::take(&mut inner.seats);
        let chunks = (tokio::runtime::Handle::current(), state);
        let job_stats = stats.clone();
        inner.job = Some(tokio::task::spawn_blocking(move || {
            seats
                .into_par_iter()
                .map(|seat| decide(seat, &job_stats, &chunks))
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
        let started = Instant::now();
        let joined = job.await;
        self.ai_seats.stats.wait(Wait::Join, started.elapsed());
        let Ok(decided) = joined else {
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
            // Orders wait for their terrain; it is settled before the tick
            // that takes them so that tick does not depend on thread timing.
            // Only the warm-ups launched by these orders (and what was
            // already running) are awaited, and `collect` awaits them, not
            // this call: they run meanwhile.
            inner.settling = Some(tokio::spawn(self.launched_preparation().settled()));
        }
    }
}
