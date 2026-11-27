//! What one run of a scenario hands back: the numbers of its windows and of
//! each game minute, the cost of the run as a whole, and the hashes that let
//! two runs be compared.
use super::windows::{Minute, Service, WindowReport};
use aoe_simulation::GameWorld;
use serde::{Deserialize, Serialize};

/// The world at one tick, kept to compare runs.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct Digest {
    pub tick: u64,
    pub hash: String,
    pub units: u64,
    pub buildings: u64,
}

/// One run in one process at one worker count.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Measurement {
    pub scenario: String,
    pub workers: usize,
    pub ticks: u64,
    pub seats: usize,
    /// Seconds to load the package, place the villages, seed the recipe and
    /// seat the AIs.
    pub setup_seconds: f64,
    /// Seconds spent inside the ticks (hashing, sampling and observing are
    /// outside).
    pub tick_seconds: f64,
    /// Seconds of the whole run: start, ticks, observing, hashing, sampling.
    pub native_seconds: f64,
    pub ticks_per_second: f64,
    /// Which tick was the slowest (1 is the first), to tell a warm-up from
    /// the steady state.
    pub slowest_tick: u64,
    pub peak_memory_mib: u64,
    pub windows: Vec<WindowReport>,
    pub series: Vec<Minute>,
    pub digests: Vec<Digest>,
    /// Orders the seats had applied and a hash of their log; `None` when the
    /// run did not record orders.
    pub order_count: Option<u64>,
    pub order_log_hash: Option<String>,
    /// Where the order log is, one line per order.
    pub order_log_file: Option<std::path::PathBuf>,
}

impl Measurement {
    /// The deterministic numbers of the run: what two runs of one match at
    /// any worker counts must agree on.
    pub fn numbers(&self) -> Vec<&Service> {
        self.windows
            .iter()
            .map(|window| &window.service)
            .chain(self.series.iter().map(|minute| &minute.service))
            .collect()
    }
}

pub(super) fn hex(hash: [u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The world's hash and sizes now.
pub(super) fn digest(world: &GameWorld) -> Digest {
    Digest {
        tick: world.tick().0,
        hash: hex(world.canonical_hash()),
        units: world.unit_count() as u64,
        buildings: world.buildings().count() as u64,
    }
}
