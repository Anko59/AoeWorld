//! From what the observer kept to the numbers of a scenario: one report per
//! window the scenario names, and one per game minute (`series`).
mod cost;
mod service;
#[cfg(test)]
mod tests;

pub(super) use cost::{Cost, Efficiency};
pub(super) use service::{Record, Service};

use super::scenarios::Span;
use serde::{Deserialize, Serialize};

/// Ticks in one game minute (ten ticks are one game second).
pub(super) const TICKS_PER_MINUTE: u64 = 600;

/// A named window of the run: ticks `from_tick + 1` through `to_tick`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct WindowReport {
    pub id: String,
    pub from_tick: u64,
    pub to_tick: u64,
    /// What the game did: whole numbers, the same at any worker count.
    pub service: Service,
    /// What it cost: wall time and memory, noisy.
    pub cost: Cost,
    pub efficiency: Efficiency,
}

/// One game minute (the last may be shorter).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Minute {
    pub minute: u64,
    pub from_tick: u64,
    pub to_tick: u64,
    pub service: Service,
    pub cost: Cost,
}

pub(super) fn window(record: Record<'_>, span: &Span) -> Result<WindowReport, String> {
    let service = service::service(record, span.from, span.to)?;
    let cost = cost::cost(record, span.from, span.to);
    let efficiency = cost::efficiency(&cost, &service);
    Ok(WindowReport {
        id: span.id.to_owned(),
        from_tick: span.from,
        to_tick: span.to,
        service,
        cost,
        efficiency,
    })
}

/// The windows a scenario names, over a run of `ticks` ticks.
pub(super) fn windows(record: Record<'_>, spans: &[Span]) -> Result<Vec<WindowReport>, String> {
    spans.iter().map(|span| window(record, span)).collect()
}

/// One entry per game minute of a run of `ticks` ticks.
pub(super) fn series(record: Record<'_>, ticks: u64) -> Result<Vec<Minute>, String> {
    (0..ticks.div_ceil(TICKS_PER_MINUTE))
        .map(|index| {
            let (from, to) = (
                index * TICKS_PER_MINUTE,
                ((index + 1) * TICKS_PER_MINUTE).min(ticks),
            );
            Ok(Minute {
                minute: index + 1,
                from_tick: from,
                to_tick: to,
                service: service::service(record, from, to)?,
                cost: cost::cost(record, from, to),
            })
        })
        .collect()
}
