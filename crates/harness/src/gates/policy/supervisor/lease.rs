//! Service-owned lifecycle MODEL: no Docker adapter, filesystem journal or authority.
use super::identity::{ContainerId, ExpectedIdentity, Observation};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
enum Action {
    PersistIntent(ExpectedIdentity),
    Create(ExpectedIdentity),
    Recover(ExpectedIdentity),
    Inspect(ContainerId),
    Stop(ContainerId),
    Kill(ContainerId),
    Remove(ContainerId),
}
impl Action {
    fn name(&self) -> &'static str {
        match self {
            Self::PersistIntent(_) => "persist-intent",
            Self::Create(_) => "create",
            Self::Recover(_) => "recover-owned-labels",
            Self::Inspect(_) => "inspect-exact-id",
            Self::Stop(_) => "stop-exact-id",
            Self::Kill(_) => "kill-exact-id",
            Self::Remove(_) => "remove-exact-id",
        }
    }
}
struct Reply {
    bytes: Vec<u8>,
    success: bool,
    truncated: bool,
}
trait Backend {
    fn now_ms(&self) -> u64;
    fn call(&mut self, action: &Action, timeout_ms: u64) -> Reply;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fault {
    Incomplete,
    Quarantined,
}
#[derive(Serialize)]
struct Receipt {
    authoritative: bool,
    status: &'static str,
    events: Vec<&'static str>,
    workload_cancelled: bool,
}
struct Controller<B> {
    backend: B,
    expected: ExpectedIdentity,
    events: Vec<&'static str>,
}
impl<B: Backend> Controller<B> {
    fn call(&mut self, action: Action, end: u64) -> Result<Reply, Fault> {
        let start = self.backend.now_ms();
        let remaining = end
            .checked_sub(start)
            .filter(|n| *n > 0)
            .ok_or(Fault::Incomplete)?;
        if self.events.len() >= 16 {
            return Err(Fault::Incomplete);
        }
        let timeout = remaining.min(5000);
        self.events.push(action.name());
        let acknowledgement = matches!(
            action,
            Action::PersistIntent(_) | Action::Stop(_) | Action::Kill(_) | Action::Remove(_)
        );
        let reply = self.backend.call(&action, timeout);
        let now = self.backend.now_ms();
        if now < start || now - start >= timeout || reply.truncated || reply.bytes.len() > 4096 {
            return Err(Fault::Incomplete);
        }
        if acknowledgement && reply.success && reply.bytes != b"null" {
            return Err(Fault::Incomplete);
        }
        Ok(reply)
    }
    fn inspect(&mut self, cid: &ContainerId, end: u64) -> Result<bool, Fault> {
        let reply = self.call(Action::Inspect(cid.clone()), end)?;
        if !reply.success {
            return Err(Fault::Incomplete);
        }
        let value: Value = serde_json::from_slice(&reply.bytes).map_err(|_| Fault::Incomplete)?;
        if value == json!({"absent":true}) {
            return Ok(false);
        }
        let observed: Observation = serde_json::from_value(value).map_err(|_| Fault::Incomplete)?;
        if !observed.verify(cid, &self.expected) {
            return Err(Fault::Quarantined);
        }
        Ok(true)
    }
    fn acquire(&mut self, end: u64, cancelled: bool) -> Result<Option<ContainerId>, Fault> {
        if cancelled {
            return Err(Fault::Incomplete);
        }
        let persisted = self.call(Action::PersistIntent(self.expected.clone()), end)?;
        if !persisted.success {
            return Err(Fault::Incomplete);
        }
        let created = self.call(Action::Create(self.expected.clone()), end)?;
        if created.success {
            return ContainerId::observed(&created.bytes)
                .map(Some)
                .map_err(|_| Fault::Quarantined);
        }
        // Only this service's immutable intent is queried; requests supply no selector.
        let recovered = self.call(Action::Recover(self.expected.clone()), end)?;
        if !recovered.success {
            return Err(Fault::Incomplete);
        }
        let ids: Vec<String> =
            serde_json::from_slice(&recovered.bytes).map_err(|_| Fault::Incomplete)?;
        if ids.len() > 16 || ids.len() > 1 {
            return Err(Fault::Quarantined);
        }
        let Some(raw) = ids.first() else {
            return Ok(None);
        };
        let cid = ContainerId::observed(raw.as_bytes()).map_err(|_| Fault::Quarantined)?;
        if !self.inspect(&cid, end)? {
            return Err(Fault::Incomplete);
        }
        Ok(Some(cid))
    }
    fn cleanup(&mut self, cid: &ContainerId, reserve_ms: u64) -> Result<(), Fault> {
        // No workload cancellation token is consulted: cleanup has its own reserve.
        let end = self
            .backend
            .now_ms()
            .checked_add(reserve_ms.min(15000))
            .ok_or(Fault::Incomplete)?;
        if !self.inspect(cid, end)? {
            return Ok(());
        }
        let stopped = self.call(Action::Stop(cid.clone()), end)?;
        if !stopped.success {
            if !self.inspect(cid, end)? {
                return Ok(());
            }
            let killed = self.call(Action::Kill(cid.clone()), end)?;
            if !killed.success {
                return Err(Fault::Incomplete);
            }
        }
        if !self.inspect(cid, end)? {
            return Ok(());
        }
        let removed = self.call(Action::Remove(cid.clone()), end)?;
        if !removed.success || self.inspect(cid, end)? {
            return Err(Fault::Incomplete);
        }
        Ok(())
    }
    fn run(
        &mut self,
        admission_ms: u64,
        cleanup_ms: u64,
        cancelled_before: bool,
        cancelled_after: bool,
    ) -> Receipt {
        let end = self
            .backend
            .now_ms()
            .checked_add(admission_ms)
            .unwrap_or(self.backend.now_ms());
        let result = self
            .acquire(end, cancelled_before)
            .and_then(|cid| match cid {
                Some(cid) => self.cleanup(&cid, cleanup_ms),
                None => Ok(()),
            });
        Receipt {
            authoritative: false,
            status: match result {
                Ok(()) => "COMPLETED_MODEL",
                Err(Fault::Incomplete) => "INCOMPLETE",
                Err(Fault::Quarantined) => "QUARANTINED",
            },
            events: self.events.clone(),
            workload_cancelled: cancelled_before || cancelled_after,
        }
    }
}

mod model;
#[cfg(test)]
use model::ModelDaemon;

/// Deterministic simulations only; this does not authenticate a service.
pub(super) fn preview_models() -> Value {
    model::preview_models()
}

#[cfg(test)]
mod tests;
