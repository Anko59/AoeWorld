use super::{ShipLock, acquire_stage};
use crate::ship::Result;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

/// The ship lock as `ship_with` shares it: let go for the review (remote calls, idle CPU)
/// and taken again, immediately, before the re-check and the push. The default
/// holds nothing, so releasing and reacquiring are then no-ops (tests, `review-pr`).
#[derive(Clone, Debug, Default)]
pub(crate) struct Handoff(Arc<Mutex<Held>>);

#[derive(Debug, Default)]
struct Held {
    lock: Option<ShipLock>,
    released: bool,
}

impl Handoff {
    pub(crate) fn holding(lock: ShipLock) -> Self {
        Self(Arc::new(Mutex::new(Held {
            lock: Some(lock),
            released: false,
        })))
    }

    /// Drop the lock; false (and silent) when this ship holds none.
    pub(crate) fn release(&self) -> bool {
        let mut held = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let had = held.lock.take().is_some();
        held.released |= had;
        had
    }

    /// Try once after a release. A busy result leaves it released for a later invocation.
    pub(crate) fn reacquire(&self, root: &Path, command: &str) -> Result<()> {
        let mut held = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if !held.released {
            return Ok(());
        }
        held.lock = Some(acquire_stage(root, command, "lock-reacquire")?);
        held.released = false;
        eprintln!("ship: lock reacquired for the push");
        Ok(())
    }
}
