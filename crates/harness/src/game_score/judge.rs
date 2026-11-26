//! Whether runs at different worker counts played the same game: the same
//! hash at every hashed tick, the same orders in the same order. When they
//! did not, the first tick where they part and what differs there.
use super::measure::{Digest, Measurement};
use serde::{Deserialize, Serialize};
use std::fs;

/// The first hashed tick where two runs disagree.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct DigestDifference {
    pub tick: u64,
    pub base: Option<Digest>,
    pub other: Option<Digest>,
    /// In words: what the two digests show of the difference.
    pub what: String,
}

/// The first order the two logs disagree on.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct OrderDifference {
    /// Zero-based position in the log.
    pub index: usize,
    pub tick: Option<u64>,
    pub base: Option<String>,
    pub other: Option<String>,
}

/// One run against the base run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct Comparison {
    pub base_workers: usize,
    pub workers: usize,
    pub same_hashes: bool,
    /// `None` when a run kept no order log.
    pub same_orders: Option<bool>,
    pub same_numbers: bool,
    pub first_differing_tick: Option<u64>,
    pub hashes: Option<DigestDifference>,
    pub orders: Option<OrderDifference>,
}

impl Comparison {
    /// The game is the same: hashes and orders agree.
    pub(super) fn holds(&self) -> bool {
        self.same_hashes && self.same_orders != Some(false)
    }
}

/// Every run against the first.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct Verdict {
    pub holds: bool,
    pub against: Vec<Comparison>,
    /// Exact first tick and difference, from a replay that hashes every tick
    /// up to the coarse one (only when the runs differ).
    pub diagnosis: Option<Comparison>,
}

/// The first position where `a` and `b` differ, or where one ends early.
fn first_mismatch<T: PartialEq>(a: &[T], b: &[T]) -> Option<usize> {
    a.iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .or((a.len() != b.len()).then_some(a.len().min(b.len())))
}

fn what_differs(base: &Digest, other: &Digest) -> String {
    if base.tick != other.tick {
        return format!(
            "the runs hashed different ticks ({} and {})",
            base.tick, other.tick
        );
    }
    let mut parts = Vec::new();
    if base.units != other.units {
        parts.push(format!("unit count {} against {}", base.units, other.units));
    }
    if base.buildings != other.buildings {
        parts.push(format!(
            "building count {} against {}",
            base.buildings, other.buildings
        ));
    }
    if parts.is_empty() {
        "same unit and building counts, a different world state".to_owned()
    } else {
        parts.join(", ")
    }
}

pub(super) fn first_difference(base: &[Digest], other: &[Digest]) -> Option<DigestDifference> {
    let index = first_mismatch(base, other)?;
    let (a, b) = (base.get(index), other.get(index));
    let tick = a
        .map_or(u64::MAX, |d| d.tick)
        .min(b.map_or(u64::MAX, |d| d.tick));
    let what = match (a, b) {
        (Some(a), Some(b)) => what_differs(a, b),
        (Some(_), None) => "the other run stopped hashing earlier".to_owned(),
        _ => "the base run stopped hashing earlier".to_owned(),
    };
    Some(DigestDifference {
        tick,
        base: a.cloned(),
        other: b.cloned(),
        what,
    })
}

/// The tick an order line begins with.
fn line_tick(line: &str) -> Option<u64> {
    line.split(' ').next()?.parse().ok()
}

pub(super) fn first_order_difference(base: &[String], other: &[String]) -> Option<OrderDifference> {
    let index = first_mismatch(base, other)?;
    let (a, b) = (base.get(index), other.get(index));
    let tick = |line: Option<&String>| line.and_then(|line| line_tick(line));
    let tick = match (tick(a), tick(b)) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    Some(OrderDifference {
        index,
        tick,
        base: a.cloned(),
        other: b.cloned(),
    })
}

/// `other` against `base`, orders given as lines (`None`: not recorded).
pub(super) fn compare(
    base: &Measurement,
    other: &Measurement,
    orders: (Option<&[String]>, Option<&[String]>),
) -> Comparison {
    let hashes = first_difference(&base.digests, &other.digests);
    let orders = orders
        .0
        .zip(orders.1)
        .map(|(a, b)| first_order_difference(a, b));
    let first_differing_tick = hashes
        .as_ref()
        .map(|difference| difference.tick)
        .into_iter()
        .chain(
            orders
                .iter()
                .flatten()
                .filter_map(|difference| difference.tick),
        )
        .min();
    Comparison {
        base_workers: base.workers,
        workers: other.workers,
        same_hashes: hashes.is_none(),
        same_orders: orders.as_ref().map(Option::is_none),
        same_numbers: base.numbers() == other.numbers(),
        first_differing_tick,
        hashes,
        orders: orders.flatten(),
    }
}

/// The order lines a run recorded.
pub(super) fn read_orders(run: &Measurement) -> Option<Vec<String>> {
    let text = fs::read_to_string(run.order_log_file.as_ref()?).ok()?;
    Some(text.lines().map(str::to_owned).collect())
}

/// Every run against the first; `diagnosis` is filled in by the caller.
pub(super) fn judge(runs: &[Measurement]) -> Verdict {
    let Some((base, rest)) = runs.split_first() else {
        return Verdict {
            holds: true,
            against: Vec::new(),
            diagnosis: None,
        };
    };
    let base_orders = read_orders(base);
    let against: Vec<_> = rest
        .iter()
        .map(|run| {
            let orders = read_orders(run);
            compare(base, run, (base_orders.as_deref(), orders.as_deref()))
        })
        .collect();
    Verdict {
        holds: against.iter().all(Comparison::holds),
        against,
        diagnosis: None,
    }
}
