//! User-only retired instructions and thread CPU time on the simulation thread.
use aoe_simulation::TickPhase;
use perf_event::{Builder, Counter, ReadFormat, events::Hardware};
use std::{
    collections::BTreeMap,
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, ThreadId},
};

#[derive(Clone, Copy)]
pub(super) struct Sample {
    pub instructions: u64,
    pub cpu_ns: u64,
}
impl std::ops::Sub for Sample {
    type Output = Self;
    fn sub(self, start: Self) -> Self {
        Self {
            instructions: self.instructions - start.instructions,
            cpu_ns: self.cpu_ns - start.cpu_ns,
        }
    }
}
pub(super) fn cpu() -> io::Result<u64> {
    let time = nix::time::clock_gettime(nix::time::ClockId::CLOCK_THREAD_CPUTIME_ID)
        .map_err(|e| io::Error::from_raw_os_error(e as i32))?;
    Ok(time.tv_sec() as u64 * 1_000_000_000 + time.tv_nsec() as u64)
}
pub struct Timing {
    pub instructions: u64,
    pub cpu_ns: u64,
    pub phases_cpu_ns: BTreeMap<&'static str, u64>,
    pub phases_instructions: BTreeMap<&'static str, u64>,
    /// Completed composition/proof spans, keyed by their inclusive caller path.
    pub navigation_calls: BTreeMap<String, u64>,
    /// Applied commands and their instructions, keyed `issuer:kind:Result`.
    pub commands: BTreeMap<String, (u64, u64)>,
    /// Refused commands: tick, player, `issuer:kind:Result`, action, instructions.
    pub refused: Vec<(u64, u16, String, String, u64)>,
}
#[derive(Debug)]
enum Source {
    Hardware {
        counter: Mutex<Counter>,
        thread: ThreadId,
    },
    #[cfg(test)]
    Fixture(AtomicU64),
}
#[derive(Debug, Default)]
struct Accumulator {
    instructions: AtomicU64,
    cpu_ns: AtomicU64,
}
type Phases = BTreeMap<&'static str, u64>;
#[derive(Debug, Default)]
struct Calls {
    stack: Vec<&'static str>,
    counts: BTreeMap<String, u64>,
}
#[derive(Debug)]
struct State {
    source: Source,
    values: Box<[Accumulator]>,
    failed: AtomicBool,
    calls: Mutex<Calls>,
}
#[derive(Clone, Debug)]
pub(super) struct Probe(Arc<State>);
fn names() -> impl Iterator<Item = &'static str> {
    crate::gameplay_match::TickProbe::phase_names()
        .iter()
        .copied()
        .chain(["steward_economy"])
        .chain(aoe_simulation::ISSUER_SPANS)
}
pub(super) fn validate(count: u64, enabled: u64, running: u64) -> io::Result<u64> {
    if running == 0 || enabled != running {
        return Err(io::Error::other(
            "instruction counter was unavailable or multiplexed; refusing scaled estimates",
        ));
    }
    Ok(count)
}
impl Probe {
    fn new(source: Source) -> Self {
        Self(Arc::new(State {
            source,
            values: names().map(|_| Accumulator::default()).collect(),
            failed: AtomicBool::new(false),
            calls: Mutex::default(),
        }))
    }
    pub(super) fn open() -> io::Result<Self> {
        let mut counter = Builder::new(Hardware::INSTRUCTIONS)
            .observe_self().any_cpu().inherit(false).exclude_kernel(true).exclude_hv(true)
            .pinned(true).read_format(ReadFormat::TOTAL_TIME_ENABLED | ReadFormat::TOTAL_TIME_RUNNING)
            .build().map_err(|e| io::Error::new(e.kind(), format!("perf_event_open instructions: {e}; run make save-replay-counter-check (Docker PERFMON and host perf_event_paranoid <= 2 required)")))?;
        counter.enable()?;
        Ok(Self::new(Source::Hardware {
            counter: Mutex::new(counter),
            thread: thread::current().id(),
        }))
    }
    #[cfg(test)]
    pub(super) fn fixture() -> Self {
        Self::new(Source::Fixture(AtomicU64::new(0)))
    }
    fn hardware(&self) -> io::Result<std::sync::MutexGuard<'_, Counter>> {
        match &self.0.source {
            Source::Hardware {
                counter,
                thread: owner,
            } if *owner == thread::current().id() => counter
                .lock()
                .map_err(|_| io::Error::other("instruction counter lock poisoned")),
            _ => Err(io::Error::other(
                "instruction counter used outside its simulation thread",
            )),
        }
    }
    pub(super) fn read(&self) -> io::Result<u64> {
        #[cfg(test)]
        if let Source::Fixture(count) = &self.0.source {
            return Ok(count.fetch_add(100, Ordering::Relaxed));
        }
        Ok(self.hardware()?.read_count_and_time()?.count)
    }
    pub(super) fn sample(&self) -> io::Result<Sample> {
        Ok(Sample {
            instructions: self.read()?,
            cpu_ns: cpu()?,
        })
    }
    pub(super) fn finish(&self) -> io::Result<u64> {
        self.check().map_err(io::Error::other)?;
        #[cfg(test)]
        if let Source::Fixture(_) = &self.0.source {
            return self.read();
        }
        let mut counter = self.hardware()?;
        counter.disable()?;
        let value = counter.read_count_and_time()?;
        validate(value.count, value.time_enabled, value.time_running)
    }
    pub(super) fn fail(&self) {
        self.0.failed.store(true, Ordering::Relaxed);
    }
    pub(super) fn check(&self) -> Result<(), &'static str> {
        if self.0.failed.load(Ordering::Relaxed) {
            Err("instruction counter or thread CPU clock read failed")
        } else {
            Ok(())
        }
    }
    pub(super) fn record(&self, name: &'static str, count: Sample) {
        if let Some(at) = names().position(|n| n == name) {
            self.0.values[at]
                .instructions
                .fetch_add(count.instructions, Ordering::Relaxed);
            self.0.values[at]
                .cpu_ns
                .fetch_add(count.cpu_ns, Ordering::Relaxed);
        }
    }
    pub(super) fn drain(&self) -> (Phases, Phases) {
        let instructions = names()
            .zip(
                self.0
                    .values
                    .iter()
                    .map(|c| c.instructions.swap(0, Ordering::Relaxed)),
            )
            .collect();
        let cpu_ns = names()
            .zip(
                self.0
                    .values
                    .iter()
                    .map(|c| c.cpu_ns.swap(0, Ordering::Relaxed)),
            )
            .collect();
        (instructions, cpu_ns)
    }
    fn span(&self, name: &'static str) -> Box<dyn aoe_map::NavigationSpan> {
        if let Ok(mut calls) = self.0.calls.lock() {
            calls.stack.push(name);
        } else {
            self.fail();
        }
        Box::new(Span {
            probe: self.clone(),
            name,
            started: self.sample(),
        })
    }

    pub(super) fn navigation_calls(&self) -> BTreeMap<String, u64> {
        self.0
            .calls
            .lock()
            .map(|calls| calls.counts.clone())
            .unwrap_or_else(|_| {
                self.fail();
                BTreeMap::new()
            })
    }

    fn end_call(&self, name: &'static str) {
        let Ok(mut calls) = self.0.calls.lock() else {
            self.fail();
            return;
        };
        if matches!(name, "terrain_point" | "pocket") {
            let path = calls.stack.join("/");
            *calls.counts.entry(path).or_default() += 1;
        }
        if calls.stack.pop() != Some(name) {
            self.fail();
        }
    }
}
struct Span {
    probe: Probe,
    name: &'static str,
    started: io::Result<Sample>,
}
impl aoe_map::NavigationSpan for Span {}
impl Drop for Span {
    fn drop(&mut self) {
        match (&self.started, self.probe.sample()) {
            (Ok(started), Ok(now)) => self.probe.record(self.name, now - *started),
            _ => self.probe.fail(),
        }
        self.probe.end_call(self.name);
    }
}
impl aoe_map::NavigationProbe for Probe {
    fn begin(&self, phase: aoe_map::NavigationPhase) -> Box<dyn aoe_map::NavigationSpan> {
        self.span(phase.name())
    }
}
impl aoe_simulation::PlanningProbe for Probe {
    fn begin(&self) {}
    fn end(&self) {}
    fn movement_span(&self, name: &'static str) -> Option<Box<dyn aoe_map::NavigationSpan>> {
        Some(self.span(name))
    }
    fn cache_span(&self) -> Option<Box<dyn aoe_map::NavigationSpan>> {
        Some(self.span("route_cache"))
    }
}
pub(super) fn phase_name(phase: TickPhase) -> &'static str {
    match phase {
        TickPhase::Movement => "movement",
        TickPhase::Spacing => "spacing",
        TickPhase::Wildlife => "wildlife",
        TickPhase::Combat => "combat",
        TickPhase::Construction => "construction",
        TickPhase::Gathering => "gathering",
        TickPhase::ResearchTraining => "research_training",
        TickPhase::StewardEconomy => "steward_economy",
        TickPhase::BuildingMemory => "building_memory",
        TickPhase::Changed => "changed",
    }
}

#[path = "tests/timing.rs"]
#[cfg(test)]
mod tests;
