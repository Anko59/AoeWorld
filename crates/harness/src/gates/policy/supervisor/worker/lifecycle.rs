//! One-shot actual lifecycle with a separate cleanup reserve; no restart watchdog.
use super::{
    Cleanup, Execution, Observation, Operation, Status, TransportObservation,
    config::Loaded,
    journal::{Event, Journal, nonce},
    transport::{self, Action, Backend, Binding, Cid, Docker},
};
use crate::{
    gates::runner::evidence::PrivateOutput,
    process::{Cancellation, CaptureExit, Captured, safe_observation},
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

pub(super) trait Intent {
    fn append(&mut self, event: Event, cid: Option<&Cid>) -> Result<(), &'static str>;
    fn verify(&mut self) -> Result<(), &'static str>;
}
impl Intent for Journal<'_> {
    fn append(&mut self, event: Event, cid: Option<&Cid>) -> Result<(), &'static str> {
        Journal::append(self, event, cid)
    }
    fn verify(&mut self) -> Result<(), &'static str> {
        Journal::verify(self)
    }
}
pub(super) struct Controller<'a, B: Backend, J: Intent, F: FnMut(bool) -> bool> {
    pub(super) backend: B,
    pub(super) journal: J,
    pub(super) binding: Binding,
    pub(super) check: F,
    pub(super) cancel: &'a Cancellation,
    pub(super) observation: Observation,
    pub(super) logs: Option<Captured>,
    last: Duration,
    work_end: Duration,
    total_end: Duration,
}
impl<'a, B: Backend, J: Intent, F: FnMut(bool) -> bool> Controller<'a, B, J, F> {
    pub(super) fn new(
        backend: B,
        journal: J,
        binding: Binding,
        check: F,
        cancel: &'a Cancellation,
        budget: Duration,
    ) -> Option<Self> {
        let start = backend.now();
        let total_end = start.checked_add(budget)?;
        let work_end =
            total_end.checked_sub(Duration::from_secs(u64::from(binding.config.cleanup_s)))?;
        if work_end <= start {
            return None;
        }
        Some(Self {
            backend,
            journal,
            binding,
            check,
            cancel,
            observation: Observation::empty(),
            logs: None,
            last: start,
            work_end,
            total_end,
        })
    }
    fn now(&mut self) -> Result<Duration, Status> {
        let now = self.backend.now();
        if now < self.last {
            return Err(Status::Incomplete);
        }
        self.last = now;
        Ok(now)
    }
    fn call(&mut self, action: Action, cleanup: bool, end: Duration) -> Result<Captured, Status> {
        if !(self.check)(cleanup) || (!cleanup && !self.binding.source_held()) {
            return Err(Status::Quarantined);
        }
        if self.observation.transport.len() >= 48 {
            return Err(Status::Incomplete);
        }
        let start = self.now()?;
        let remaining = end
            .checked_sub(start)
            .filter(|n| !n.is_zero())
            .ok_or(Status::Deadline)?;
        let timeout = if matches!(action, Action::Wait(_)) {
            remaining
        } else {
            remaining.min(Duration::from_secs(u64::from(
                self.binding.config.command_s,
            )))
        };
        let independent = Cancellation::default();
        let cancel = if cleanup { &independent } else { self.cancel };
        let captured = self.backend.call(&self.binding, &action, timeout, cancel);
        self.observation.transport.push(TransportObservation {
            phase: action.phase(),
            capture: safe_observation(&captured),
        });
        let now = self.now()?;
        if now.saturating_sub(start) >= timeout {
            return Err(Status::Deadline);
        }
        Ok(captured)
    }
    fn recorded(&mut self, event: Event, cid: Option<&Cid>) -> bool {
        let ok = self.journal.append(event, cid).is_ok();
        self.observation.journal_retained &= ok;
        ok
    }
    fn inspect(
        &mut self,
        cid: &Cid,
        cleanup: bool,
        end: Duration,
    ) -> Result<transport::State, Status> {
        let captured = self.call(Action::Inspect(cid.clone()), cleanup, end)?;
        let value = transport::object(&captured).ok_or(Status::Incomplete)?;
        transport::inspect_matches(&value, cid, &self.binding).ok_or(Status::Quarantined)
    }
    fn acquire(&mut self) -> Result<Cid, (Status, Option<Cid>)> {
        if self.cancel.cancelled() {
            return Err((Status::Cancelled, None));
        }
        let image = self
            .call(Action::ImageInspect, false, self.work_end)
            .map_err(|s| (s, None))?;
        if !transport::object(&image).is_some_and(|v| transport::image_matches(&v, &self.binding)) {
            return Err((Status::Unavailable, None));
        }
        if self.cancel.cancelled() {
            return Err((Status::Cancelled, None));
        }
        if self.journal.append(Event::Intent, None).is_err() {
            return Err((Status::Unavailable, None));
        }
        self.observation.journal_retained = true;
        let created = self.call(Action::Create, false, self.work_end);
        // Recovery runs independently of workload cancellation, inside total reserve.
        let cid = match created {
            Ok(capture) if transport::successful(&capture) => match Cid::parse(&capture.stdout) {
                Some(cid) => cid,
                None => return self.recover(Status::Quarantined),
            },
            Ok(capture) => return self.recover(capture_status(&capture)),
            Err(status) => return self.recover(status),
        };
        if !self.recorded(Event::Created, Some(&cid)) {
            return Err((Status::Incomplete, Some(cid)));
        }
        match self.inspect(&cid, false, self.work_end) {
            Ok(state) if !state.running => Ok(cid),
            Ok(_) => Err((Status::Quarantined, Some(cid))),
            Err(status) => Err((status, Some(cid))),
        }
    }
    fn recover(&mut self, original: Status) -> Result<Cid, (Status, Option<Cid>)> {
        let recovered = match self.call(Action::Recover, true, self.total_end) {
            Ok(capture) => capture,
            Err(_) => {
                self.observation.cleanup = Cleanup::Quarantined;
                return Err((Status::Quarantined, None));
            }
        };
        let ids = match transport::recover(&recovered) {
            Some(ids) => ids,
            None => {
                self.observation.cleanup = Cleanup::Quarantined;
                return Err((Status::Quarantined, None));
            }
        };
        let Some(cid) = ids.first().cloned() else {
            // Zero now is not proof a timed-out daemon request cannot create later.
            self.observation.cleanup = Cleanup::Incomplete;
            return Err((original, None));
        };
        if self.inspect(&cid, true, self.total_end).is_err() {
            self.observation.cleanup = Cleanup::Quarantined;
            return Err((Status::Quarantined, None));
        }
        self.recorded(Event::Created, Some(&cid));
        Err((original, Some(cid)))
    }
    fn workload(&mut self, cid: &Cid) -> Status {
        if self.cancel.cancelled() {
            return Status::Cancelled;
        }
        let started = match self.call(Action::Start(cid.clone()), false, self.work_end) {
            Ok(c) => c,
            Err(s) => return s,
        };
        if !transport::successful(&started) || Cid::parse(&started.stdout).as_ref() != Some(cid) {
            return capture_status(&started);
        }
        if !self.recorded(Event::Started, Some(cid)) {
            return Status::Incomplete;
        }
        let now = match self.now() {
            Ok(now) => now,
            Err(s) => return s,
        };
        let end = now
            .saturating_add(Duration::from_secs(u64::from(
                self.binding.config.workload_s,
            )))
            .min(self.work_end);
        let waited = match self.call(Action::Wait(cid.clone()), false, end) {
            Ok(c) => c,
            Err(s) => return s,
        };
        let Some(exit) = transport::exit_code(&waited) else {
            return capture_status(&waited);
        };
        // Docker wait status is NOT the worker exit; validate against real inspect state.
        self.observation.container_exit_code = Some(exit);
        let state = match self.inspect(cid, false, self.work_end) {
            Ok(state) => state,
            Err(s) => return s,
        };
        if state.running || state.exit_code != exit {
            return Status::Quarantined;
        }
        if !self.recorded(Event::WorkloadObserved, Some(cid)) {
            return Status::Incomplete;
        }
        let logs = match self.call(Action::Logs(cid.clone()), false, self.work_end) {
            Ok(c) => c,
            Err(s) => return s,
        };
        let logs_ok = transport::successful(&logs);
        let log_status = capture_status(&logs);
        self.logs = Some(logs);
        if !logs_ok {
            return if exit != 0 {
                Status::Failed
            } else {
                log_status
            };
        }
        if exit == 0 {
            Status::CompletedNonAuthoritative
        } else {
            Status::Failed
        }
    }
    fn absent(&mut self, cid: &Cid, end: Duration) -> Result<bool, Status> {
        let captured = self.call(Action::Query(cid.clone()), true, end)?;
        let ids = transport::recover(&captured).ok_or(Status::Incomplete)?;
        if ids.is_empty() {
            return Ok(true);
        }
        if ids.first() != Some(cid) {
            return Err(Status::Quarantined);
        }
        Ok(false)
    }
    fn cleanup(&mut self, cid: &Cid) -> Cleanup {
        self.recorded(Event::CleanupAttempt, Some(cid));
        let result = (|| -> Result<(), Status> {
            let now = self.now()?;
            let end = now
                .saturating_add(Duration::from_secs(u64::from(
                    self.binding.config.cleanup_s,
                )))
                .min(self.total_end);
            if self.absent(cid, end)? {
                return Ok(());
            }
            let state = self.inspect(cid, true, end)?;
            if state.running {
                let stopped = self.call(Action::Stop(cid.clone()), true, end);
                if !stopped.as_ref().is_ok_and(transport::successful) {
                    if self.absent(cid, end)? {
                        return Ok(());
                    }
                    if self.inspect(cid, true, end)?.running {
                        let killed = self.call(Action::Kill(cid.clone()), true, end)?;
                        if !transport::successful(&killed) {
                            return Err(Status::Incomplete);
                        }
                    }
                }
            }
            if self.absent(cid, end)? {
                return Ok(());
            }
            self.inspect(cid, true, end)?;
            if self.logs.is_none() {
                // Cancellation/deadline tails are sampled independently; failure cannot block removal.
                if let Ok(logs) = self.call(Action::Logs(cid.clone()), true, end) {
                    self.logs = Some(logs);
                }
            }
            let removed = self.call(Action::Remove(cid.clone()), true, end)?;
            if !transport::successful(&removed) || !self.absent(cid, end)? {
                return Err(Status::Incomplete);
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.recorded(Event::VerifiedAbsent, Some(cid));
                Cleanup::VerifiedAbsent
            }
            Err(Status::Quarantined) => {
                self.recorded(Event::Incomplete, Some(cid));
                Cleanup::Quarantined
            }
            Err(_) => {
                self.recorded(Event::Incomplete, Some(cid));
                Cleanup::Incomplete
            }
        }
    }
    pub(super) fn run(mut self) -> Execution {
        let start = self.backend.now();
        let (status, cid) = match self.acquire() {
            Ok(cid) => (self.workload(&cid), Some(cid)),
            Err((status, cid)) => (status, cid),
        };
        self.observation.status = status;
        self.observation.workload_status = status;
        if let Some(cid) = cid {
            self.observation.cleanup = self.cleanup(&cid);
        }
        if matches!(
            self.observation.cleanup,
            Cleanup::Incomplete | Cleanup::Quarantined
        ) && self.observation.status == Status::CompletedNonAuthoritative
        {
            self.observation.status = Status::Incomplete;
        }
        let template_ok = (self.check)(false);
        self.observation.template_endpoint_unchanged = template_ok;
        self.observation.journal_retained &= self.journal.verify().is_ok();
        if (!template_ok || !self.observation.journal_retained)
            && self.observation.status == Status::CompletedNonAuthoritative
        {
            self.observation.status = Status::Incomplete;
        }
        self.observation.controller_duration_ms = self
            .backend
            .now()
            .saturating_sub(start)
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        Execution {
            observation: self.observation,
            logs: self.logs,
        }
    }
}
fn capture_status(capture: &Captured) -> Status {
    match capture.exit {
        CaptureExit::Deadline => Status::Deadline,
        CaptureExit::Cancelled => Status::Cancelled,
        CaptureExit::Success
        | CaptureExit::Failed(_)
        | CaptureExit::Start(_)
        | CaptureExit::Monitor(_) => Status::Incomplete,
    }
}
pub(super) fn execute(
    source: &Path,
    operation: Operation,
    output: &PrivateOutput,
    budget: Duration,
    cancel: &Cancellation,
    seal: &str,
) -> Execution {
    let unavailable = || Execution {
        observation: Observation::empty(),
        logs: None,
    };
    let mut loaded = match Loaded::load() {
        Ok(loaded) => loaded,
        Err(_) => return unavailable(),
    };
    let nonce = match nonce() {
        Ok(nonce) => nonce,
        Err(_) => return unavailable(),
    };
    let binding = match Binding::new(source, loaded.config.clone(), operation, nonce.clone()) {
        Some(b) => b,
        None => return unavailable(),
    };
    let journal = match Journal::new(output, nonce, seal, &loaded.digest, &binding.operation) {
        Ok(j) => j,
        Err(_) => return unavailable(),
    };
    let backend = Docker {
        start: Instant::now(),
    };
    match Controller::new(
        backend,
        journal,
        binding,
        |cleanup| {
            if cleanup {
                loaded.verify_transport().is_ok()
            } else {
                loaded.verify().is_ok()
            }
        },
        cancel,
        budget,
    ) {
        Some(controller) => controller.run(),
        None => unavailable(),
    }
}
#[cfg(test)]
mod tests;
