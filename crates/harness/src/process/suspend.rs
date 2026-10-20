//! Host suspend detection. CLOCK_BOOTTIME keeps counting while the machine sleeps and
//! CLOCK_MONOTONIC does not, so their difference (the boot-time gap) grows by the length
//! of every suspend since boot. Deadlines count monotonic time, so a run frozen by a
//! suspend never reaches its deadline; comparing the gap with the value recorded when
//! the run started sees the suspend instead.
pub(crate) mod containers;
#[cfg(test)]
mod tests;

use std::{
    io,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Growth of the boot-time gap that counts as a suspend.
pub(crate) const THRESHOLD: Duration = Duration::from_secs(60);
/// Label on the containers the harness starts: the gap in whole seconds at their start.
/// Make's `DOCKER_RUN` sets it from [`GAP_VARIABLE`], which supervised children inherit.
pub(crate) const LABEL: &str = "aoe.harness-boot-gap";
/// Environment variable with the gap, in whole seconds, at the start of a supervised child.
pub(crate) const GAP_VARIABLE: &str = "AOE_HARNESS_BOOT_GAP";

/// Both clocks read at one instant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Reading {
    pub(crate) boot: Duration,
    pub(crate) monotonic: Duration,
}

impl Reading {
    /// Time the machine has spent suspended since boot.
    pub(crate) fn gap(self) -> Duration {
        self.boot.saturating_sub(self.monotonic)
    }
}

/// Where readings come from: [`read`] in production, an injected sequence in tests.
pub(crate) type Clock<'a> = dyn Fn() -> io::Result<Reading> + 'a;

/// Names a file holding whole seconds that test builds add to the boot clock. Production
/// builds do not compile this override, even when the variable is set in their environment.
#[cfg(test)]
pub(crate) const SIMULATED_GAP_FILE: &str = "AOE_HARNESS_SIMULATED_GAP_FILE";

#[cfg(test)]
fn simulated_gap() -> Duration {
    std::env::var_os(SIMULATED_GAP_FILE)
        .and_then(|file| std::fs::read_to_string(file).ok())
        .and_then(|text| text.trim().parse().ok())
        .map_or(Duration::ZERO, Duration::from_secs)
}

/// The process clocks. Only test builds can add a simulated suspend.
pub(crate) fn read() -> io::Result<Reading> {
    let reading = read_real()?;
    #[cfg(test)]
    let reading = Reading {
        boot: reading.boot + simulated_gap(),
        ..reading
    };
    Ok(reading)
}

#[cfg(target_os = "linux")]
pub(crate) fn read_real() -> io::Result<Reading> {
    use nix::time::{ClockId, clock_gettime};
    let span = |clock| -> io::Result<Duration> {
        let time = clock_gettime(clock).map_err(io::Error::from)?;
        Ok(Duration::new(
            u64::try_from(time.tv_sec()).unwrap_or(0),
            u32::try_from(time.tv_nsec()).unwrap_or(0),
        ))
    };
    Ok(Reading {
        boot: span(ClockId::CLOCK_BOOTTIME)?,
        monotonic: span(ClockId::CLOCK_MONOTONIC)?,
    })
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn read_real() -> io::Result<Reading> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "suspend detection needs CLOCK_BOOTTIME (Linux)",
    ))
}

/// Whether the gap moved by more than the threshold between `recorded` and `now`.
pub(crate) fn moved(recorded: Duration, now: Duration) -> bool {
    now.abs_diff(recorded) > THRESHOLD
}

/// The gap when a run started. Without a readable clock the run is never judged interrupted.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Watch {
    start: Option<Duration>,
}

impl Watch {
    pub(crate) fn start(clock: &Clock<'_>) -> Self {
        Self {
            start: clock().ok().map(Reading::gap),
        }
    }

    /// The gap at start in whole seconds, for [`GAP_VARIABLE`] and [`LABEL`].
    pub(crate) fn gap_seconds(&self) -> Option<u64> {
        self.start.map(|gap| gap.as_secs())
    }

    /// Whether a suspend has interrupted the run since it started.
    pub(crate) fn interrupted(&self, clock: &Clock<'_>) -> bool {
        self.start
            .zip(clock().ok())
            .is_some_and(|(start, now)| moved(start, now.gap()))
    }
}

/// The wall clock as the UTC text used in messages, e.g. `2026-10-17 07:10:00Z`.
pub(crate) fn wall_time() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    crate::ship::lock::utc(seconds)
}
