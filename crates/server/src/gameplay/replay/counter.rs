//! The calling thread's user-mode instruction counter. Only Linux has one
//! (`perf_event_open`); elsewhere `Counter` has no value, so opening it reports
//! that there is no hardware counter and nothing can read one.
#[cfg(not(target_os = "linux"))]
pub(crate) use absent::{Counter, pinned, running};
#[cfg(target_os = "linux")]
pub(crate) use perf::{Counter, pinned, running};

#[cfg(target_os = "linux")]
mod perf {
    pub(crate) use perf_event::Counter;
    use perf_event::{Builder, ReadFormat, events::Hardware};
    use std::io;

    /// Counting now, and always scheduled: a multiplexed reading shows in its times.
    pub(crate) fn pinned() -> io::Result<Counter> {
        let mut counter = Builder::new(Hardware::INSTRUCTIONS)
            .observe_self().any_cpu().inherit(false).exclude_kernel(true).exclude_hv(true)
            .pinned(true).read_format(ReadFormat::TOTAL_TIME_ENABLED | ReadFormat::TOTAL_TIME_RUNNING)
            .build().map_err(|e| io::Error::new(e.kind(), format!("perf_event_open instructions: {e}; run make save-replay-counter-check (Docker PERFMON and host perf_event_paranoid <= 2 required)")))?;
        counter.enable()?;
        Ok(counter)
    }

    /// Counting now; `None` where the kernel or container gives no hardware counter.
    pub(crate) fn running() -> Option<Counter> {
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
}

#[cfg(not(target_os = "linux"))]
mod absent {
    use std::io;

    #[derive(Debug)]
    pub(crate) enum Counter {}

    /// What a Linux counter returns with its enabled and running times.
    pub(crate) struct CountAndTime {
        pub(crate) count: u64,
        pub(crate) time_enabled: u64,
        pub(crate) time_running: u64,
    }

    impl Counter {
        pub(crate) fn disable(&mut self) -> io::Result<()> {
            match *self {}
        }
        pub(crate) fn read(&mut self) -> io::Result<u64> {
            match *self {}
        }
        pub(crate) fn read_count_and_time(&mut self) -> io::Result<CountAndTime> {
            match *self {}
        }
    }

    pub(crate) fn pinned() -> io::Result<Counter> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no hardware instruction counter on this system: replays are counted on Linux",
        ))
    }

    pub(crate) fn running() -> Option<Counter> {
        None
    }
}
