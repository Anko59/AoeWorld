//! Who spends route work and how it ends (diagnostics only: never hashed,
//! never read by the simulation). The cumulative scheduler counters say the
//! fair budget is saturated; these split work and outcomes by unit category
//! and straight distance, name the call sites whose searches fail, and count
//! formation orders ended by the wait bound apart from single units.
use crate::UnitKind;
use aoe_core::TileCoord;
use std::collections::BTreeMap;
use std::panic::Location;

/// Unit categories of a route: villager, soldier, siege, ship.
pub const ROUTE_CATEGORIES: [&str; 4] = ["villager", "soldier", "siege", "ship"];
/// Straight (Chebyshev) distance classes: up to 64, 256, 1,024 tiles, and above.
pub const ROUTE_DISTANCES: [&str; 4] = ["near", "mid", "far", "very_far"];
/// Why a search ended without a route.
pub const ROUTE_FAILURES: [&str; 5] = [
    "unreachable",
    "invalid_destination",
    "search_limit",
    "work_bound",
    "other",
];

fn category(kind: UnitKind) -> usize {
    if kind.is_naval() {
        3
    } else if matches!(
        kind,
        UnitKind::Villager | UnitKind::Vassal | UnitKind::Steward
    ) {
        0
    } else if matches!(
        kind,
        UnitKind::BatteringRam
            | UnitKind::CappedRam
            | UnitKind::SiegeRam
            | UnitKind::Mangonel
            | UnitKind::Onager
    ) {
        2
    } else {
        1
    }
}

fn distance(from: TileCoord, to: TileCoord) -> usize {
    match (from.x - to.x).abs().max((from.y - to.y).abs()) {
        ..=64 => 0,
        65..=256 => 1,
        257..=1_024 => 2,
        _ => 3,
    }
}

/// One cell of the category-by-distance table.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RouteClass {
    pub orders: u64,
    pub issue_work: u64,
    pub fair_work: u64,
    pub lookahead_work: u64,
    pub found: u64,
    pub failed: u64,
    /// Failed searches that spent 1,024 to 4,095 work.
    pub failed_1_4ki: u64,
    pub failed_work: u64,
    pub expired: u64,
}

/// Failed searches of one call site (`file:line` of the order's origin).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RouteCaller {
    pub orders: u64,
    pub failed: u64,
    pub failed_1_4ki: u64,
    pub failed_work: u64,
    /// By `ROUTE_FAILURES`, failures of 1 to 4 Ki work only.
    pub reasons_1_4ki: [u64; 5],
    /// By `ROUTE_CATEGORIES`, failures of 1 to 4 Ki work only.
    pub categories_1_4ki: [u64; 4],
    /// Answered from the route cache as unreachable or invalid (no work).
    pub cached_failures: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RouteAttribution {
    /// `[category][distance]`.
    pub classes: [[RouteClass; 4]; 4],
    pub callers: BTreeMap<String, RouteCaller>,
    /// Formation orders (the carrier's wait) ended by the wait bound, and the
    /// members they took with them; single units ended by it.
    pub group_orders_expired: u64,
    pub group_members_expired: u64,
    pub single_expired: u64,
    /// Formation orders whose carrier's search failed after issue.
    pub group_orders_failed: u64,
    /// Formation orders found / failed / waiting at issue.
    pub group_orders: u64,
    /// Expired orders that never got a search slot.
    pub expired_without_planner: u64,
    /// Formation carriers whose far wait was renewed instead of ending the group.
    pub group_wait_renewals: u64,
    /// Work spent from the reserved far formation pool.
    pub group_pool_work: u64,
    /// Member detours that found no slot and rejoined their group instead.
    pub detours_declined: u64,
    /// Of those: its player had no fast allowance left this tick, or no slot.
    pub detours_without_fast: u64,
    pub detours_at_capacity: u64,
    /// The latest 1 to 4 Ki failures: caller, kind, ends, reason, work.
    pub samples: std::collections::VecDeque<String>,
}

/// Where a search came from, for the per-caller table.
pub(crate) type Caller = Option<&'static Location<'static>>;

fn caller_key(caller: Caller) -> String {
    caller.map_or_else(
        || "unknown".to_owned(),
        |at| {
            let file = at.file().rsplit("src/").next().unwrap_or(at.file());
            format!("{file}:{}", at.line())
        },
    )
}

impl RouteAttribution {
    fn class(&mut self, kind: UnitKind, from: TileCoord, to: TileCoord) -> &mut RouteClass {
        &mut self.classes[category(kind)][distance(from, to)]
    }

    pub(crate) fn order(&mut self, kind: UnitKind, from: TileCoord, to: TileCoord, at: Caller) {
        self.class(kind, from, to).orders += 1;
        self.callers.entry(caller_key(at)).or_default().orders += 1;
    }

    pub(crate) fn work(
        &mut self,
        kind: UnitKind,
        from: TileCoord,
        to: TileCoord,
        fair: u8,
        w: u32,
    ) {
        let class = self.class(kind, from, to);
        let work = u64::from(w);
        match fair {
            0 => class.issue_work += work,
            1 => class.fair_work += work,
            _ => class.lookahead_work += work,
        }
    }

    pub(crate) fn found(&mut self, kind: UnitKind, from: TileCoord, to: TileCoord) {
        self.class(kind, from, to).found += 1;
    }

    pub(crate) fn cached_failure(&mut self, at: Caller) {
        self.callers
            .entry(caller_key(at))
            .or_default()
            .cached_failures += 1;
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn failed(
        &mut self,
        kind: UnitKind,
        from: TileCoord,
        to: TileCoord,
        work: u32,
        reason: usize,
        at: Caller,
    ) {
        let small = (1_024..4_096).contains(&work);
        let class = self.class(kind, from, to);
        class.failed += 1;
        class.failed_work += u64::from(work);
        class.failed_1_4ki += u64::from(small);
        let key = caller_key(at);
        let caller = self.callers.entry(key.clone()).or_default();
        caller.failed += 1;
        caller.failed_work += u64::from(work);
        if small {
            caller.failed_1_4ki += 1;
            caller.reasons_1_4ki[reason.min(4)] += 1;
            caller.categories_1_4ki[category(kind)] += 1;
            if self.samples.len() >= 8 {
                self.samples.pop_front();
            }
            self.samples.push_back(format!(
                "{key} {kind:?} ({},{})->({},{}) {} work {work}",
                from.x,
                from.y,
                to.x,
                to.y,
                ROUTE_FAILURES[reason.min(4)]
            ));
        }
    }

    pub(crate) fn expired(&mut self, kind: UnitKind, from: TileCoord, to: TileCoord) {
        self.class(kind, from, to).expired += 1;
    }
}

/// The failure reason index of a planner poll that ended without a route.
pub(crate) fn failure_reason(poll: &aoe_map::RoutePlannerPoll) -> usize {
    use aoe_map::RoutePlannerPoll as P;
    match poll {
        P::Unreachable => 0,
        P::InvalidDestination => 1,
        P::SearchLimit | P::Complete => 2,
        _ => 4,
    }
}
