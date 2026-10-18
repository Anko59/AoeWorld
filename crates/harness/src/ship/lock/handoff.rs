//! The ship lock as one ship run shares it across its review: released for the review and
//! queued for again before the re-check and the push. Reacquiring is a returning ship, so
//! it is admitted before every plain waiter (see `lock.rs`, `Holder::rank`).
use super::{ShipLock, acquire_as};
use crate::ship::Result;
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};

/// The ship lock as `ship_with` shares it: let go for the review (remote calls, idle CPU)
/// and taken again, in the same queue, before the re-check and the push. The default
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

    /// Drop the lock and its ticket; false (and silent) when this ship holds none.
    pub(crate) fn release(&self) -> bool {
        let mut held = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let had = held.lock.take().is_some();
        held.released |= had;
        had
    }

    /// Queue for the lock again, only if `release` gave it up. A failed wait leaves it released.
    pub(crate) fn reacquire(&self, root: &Path, command: &str) -> Result<()> {
        let mut held = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if !held.released {
            return Ok(());
        }
        let begun = Instant::now();
        held.lock = Some(acquire_as(root, command, true)?);
        held.released = false;
        eprintln!(
            "ship: lock reacquired for the push (waited {}s)",
            begun.elapsed().as_secs()
        );
        Ok(())
    }
}
