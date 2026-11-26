//! A clock that only moves when the test says so: a pause or a slow request
//! is a jump of its reading, never a wait.
use crate::devserver::warm::Clock;
use std::{sync::Mutex, time::Duration, time::Instant};

pub(crate) struct FakeClock {
    base: Instant,
    jumped: Mutex<Duration>,
    pauses: Mutex<Vec<Duration>>,
}

impl FakeClock {
    pub(crate) fn new() -> Self {
        Self {
            base: Instant::now(),
            jumped: Mutex::new(Duration::ZERO),
            pauses: Mutex::new(Vec::new()),
        }
    }

    /// Time passes: a request that took `time`.
    pub(crate) fn advance(&self, time: Duration) {
        *self.jumped.lock().unwrap() += time;
    }

    /// Every pause asked of the clock, in order.
    pub(crate) fn pauses(&self) -> Vec<Duration> {
        self.pauses.lock().unwrap().clone()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        self.base + *self.jumped.lock().unwrap()
    }
    fn pause(&self, time: Duration) {
        self.pauses.lock().unwrap().push(time);
        self.advance(time);
    }
}
