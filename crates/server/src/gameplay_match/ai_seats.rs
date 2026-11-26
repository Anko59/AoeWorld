//! Rust AI seats played inside the server (`AOE_AI_RUNTIME=rust`).
//!
//! A seat joins its slot like a socket client (`join_slot`, with an in-memory
//! queue instead of a socket), sees the same fog-filtered message stream,
//! keeps an `aoe_sdk::Mirror` of it and sends its orders through the same
//! dispatch as a socket client's requests. The tick calls the seats at two
//! fixed points (see `driver`), so the orders of a given seeded match land on
//! the same ticks at any worker count and whether or not the server is behind.
use crate::{AppState, GameplayService, config::AiRuntime};
use aoe_core::PlayerId;
use aoe_protocol::CommandResult;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

mod driver;

/// One order an in-process seat had applied.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AppliedOrder {
    /// The tick the order was applied on.
    pub tick: u64,
    pub player: PlayerId,
    pub sequence: u64,
    pub result: CommandResult,
    /// The order as the simulation received it.
    pub order: String,
}

/// Decisions made and thread CPU they cost, over the whole match, and the
/// seats' work on the tick's own thread (applying the delivered messages).
#[derive(Default)]
pub(super) struct Stats {
    decisions: AtomicU64,
    cpu_nanos: AtomicU64,
    tick_calls: AtomicU64,
    tick_cpu_nanos: AtomicU64,
    /// Ticks whose user-mode instructions were counted, and those instructions.
    counted_ticks: AtomicU64,
    tick_instructions: AtomicU64,
    /// Wall time the tick's thread spends in each wait, by `Wait`.
    waits: [Counter; 4],
    /// Each seat's own counters (the Python AI's `board.stats`), by session.
    seats:
        std::sync::Mutex<std::collections::BTreeMap<u64, std::collections::BTreeMap<String, u64>>>,
}

impl Stats {
    /// Keeps the latest counters of a seat's brain, for `/admin/perf`.
    pub(super) fn counters(&self, session: u64, stats: &std::collections::BTreeMap<String, u64>) {
        self.seats
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(session, stats.clone());
    }
}

/// The waits of the tick's thread that the seats cause, reported separately.
#[derive(Clone, Copy)]
pub(super) enum Wait {
    /// Applying what was delivered to every seat.
    Feed,
    /// Taking in and asking for map chunks at a decision.
    Chunks,
    /// Joining the decisions of the previous tick.
    Join,
    /// Awaiting the terrain warm-ups the seats' orders launched.
    Settle,
}

/// Times a wait was entered and the wall time it took.
#[derive(Default)]
struct Counter {
    calls: AtomicU64,
    nanos: AtomicU64,
}

impl Stats {
    pub(super) fn wait(&self, wait: Wait, wall: std::time::Duration) {
        let counter = &self.waits[wait as usize];
        counter.calls.fetch_add(1, Ordering::Relaxed);
        counter
            .nanos
            .fetch_add(wall.as_nanos() as u64, Ordering::Relaxed);
    }

    pub(super) fn record(&self, cpu: std::time::Duration) {
        self.decisions.fetch_add(1, Ordering::Relaxed);
        self.cpu_nanos
            .fetch_add(cpu.as_nanos() as u64, Ordering::Relaxed);
    }

    pub(super) fn record_tick(&self, cpu: std::time::Duration, instructions: Option<u64>) {
        self.tick_calls.fetch_add(1, Ordering::Relaxed);
        self.tick_cpu_nanos
            .fetch_add(cpu.as_nanos() as u64, Ordering::Relaxed);
        if let Some(count) = instructions {
            self.counted_ticks.fetch_add(1, Ordering::Relaxed);
            self.tick_instructions.fetch_add(count, Ordering::Relaxed);
        }
    }
}

#[derive(Default)]
pub(crate) struct AiSeats {
    inner: tokio::sync::Mutex<driver::Inner>,
    /// Whether any seat exists: the tick skips the seats otherwise.
    active: AtomicBool,
    /// Sessions played in process.
    sessions: std::sync::Mutex<BTreeSet<u64>>,
    log: Arc<std::sync::Mutex<Vec<AppliedOrder>>>,
    stats: Arc<Stats>,
}

impl AiSeats {
    pub(crate) fn sessions(&self) -> BTreeSet<u64> {
        self.sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    /// The applied-order log, kept only in test builds: a live match would
    /// serialize every AI order on the tick path for nobody to read.
    pub(crate) fn log_handle(&self) -> Option<Arc<std::sync::Mutex<Vec<AppliedOrder>>>> {
        cfg!(test).then(|| self.log.clone())
    }

    fn add_session(&self, id: u64) {
        self.sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(id);
    }
}

/// Records an order an in-process seat had applied (test builds only, see
/// `log_handle`).
pub(crate) fn record(log: &std::sync::Mutex<Vec<AppliedOrder>>, order: AppliedOrder) {
    log.lock()
        .unwrap_or_else(|error| error.into_inner())
        .push(order);
}

impl GameplayService {
    /// Every order an in-process seat had applied, in application order.
    #[cfg(test)]
    pub(crate) fn ai_order_log(&self) -> Vec<AppliedOrder> {
        self.ai_seats
            .log
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    /// Makes the session's order budget count game ticks from now on.
    async fn start_command_clock(&self, session: u64) {
        let tick = self.world.read().await.tick().0;
        if let Some(session) = self.sessions.lock().await.get_mut(&session) {
            session.command_tick = Some(tick);
        }
    }

    /// The Rust AI's cost over this match: decisions and their thread CPU
    /// (worker threads), and per tick what it costs the tick's thread: the CPU
    /// of applying the delivery and, as wall time, each of its waits (`feed`,
    /// `chunk_waits`, `join` of the decisions, `settle` of the terrain
    /// warm-ups) with how often it was entered. `instructions_per_tick` is
    /// null where no hardware counter is available.
    pub(crate) fn rust_ai_perf(&self) -> Value {
        let stats = &self.ai_seats.stats;
        let load = |value: &AtomicU64| value.load(Ordering::Relaxed);
        let per = |total: u64, count: u64| (count > 0).then(|| total as f64 / count as f64);
        let ticks = load(&stats.tick_calls);
        let wait = |wait: Wait| {
            let counter = &stats.waits[wait as usize];
            json!({
                "calls": load(&counter.calls),
                "wall_us_per_tick": per(load(&counter.nanos), ticks).map(|ns| ns / 1e3),
            })
        };
        json!({
            "decisions": load(&stats.decisions),
            "cpu_ms": load(&stats.cpu_nanos) as f64 / 1e6,
            "tick_thread": {
                "ticks": load(&stats.tick_calls),
                "cpu_us_per_tick": per(load(&stats.tick_cpu_nanos), load(&stats.tick_calls)).map(|ns| ns / 1e3),
                "instructions_per_tick": per(load(&stats.tick_instructions), load(&stats.counted_ticks)),
                "feed": wait(Wait::Feed),
                "chunk_waits": wait(Wait::Chunks),
                "join": wait(Wait::Join),
                "settle": wait(Wait::Settle),
            },
            "seats": stats.seats.lock().unwrap_or_else(|error| error.into_inner()).values().collect::<Vec<_>>(),
        })
    }
}

/// Seats every non-human slot of the state's current match in process and
/// returns how many; with `Python` it seats none (the Python hosts join over
/// the socket instead).
pub(crate) async fn play_ai_seats(state: &AppState, runtime: AiRuntime) -> usize {
    if runtime != AiRuntime::Rust {
        return 0;
    }
    let gameplay = state.gameplay.read().await.clone();
    gameplay.seat_ai(state.clone()).await
}
