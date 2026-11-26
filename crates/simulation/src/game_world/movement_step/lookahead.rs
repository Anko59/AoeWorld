//! Retained planning ahead shares the unused fair-pass work allowance.
use crate::game_world::parallel::par_each_mut;
use crate::game_world::route_scheduler::{REGION_PROOF_STEP, ROUTE_SLICE_WORK};
use crate::{GameWorld, UnitKind};
use aoe_core::{EntityId, PlayerId};
use aoe_map::RoutePlanner;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// One walking unit's planning ahead, polled on a worker.
struct Ahead {
    index: usize,
    planner: RoutePlanner,
    kind: UnitKind,
    player: PlayerId,
    share: u32,
    /// It wanted a cold chunk the worker could not prepare.
    denied: AtomicBool,
}

impl GameWorld {
    /// Plans the leg after the one each moving unit is walking, from what
    /// the waiting units left of the tick budget, so a walking unit does not
    /// pause between legs. A search that fails here is reported when the unit
    /// has used the segments already planned. Each unit's share is fixed
    /// before the searches run on workers (#978: serial, they were 432M of
    /// 2,375M main-thread instructions a tick on the 5x save); a search that
    /// wants a cold chunk finishes alone afterwards, in unit order.
    #[inline(never)] // a symbol of its own for the Callgrind stage counters
    pub(crate) fn plan_routes_ahead(&mut self, movers: &[EntityId], start: usize, budget: u32) {
        let wanting = (0..movers.len())
            .map(|turn| movers[(start + turn) % movers.len()])
            .filter_map(|id| self.lookup.get(&id).copied())
            .filter(|&index| {
                self.units[index].order.is_some()
                    && self.units[index]
                        .planner
                        .as_deref()
                        .is_some_and(RoutePlanner::wants_lookahead)
            })
            .collect::<Vec<_>>();
        if wanting.is_empty() || budget == 0 {
            return;
        }
        // At least one work each, for at most `budget` units.
        let share = (budget / wanting.len() as u32).clamp(1, ROUTE_SLICE_WORK);
        let mut searches = Vec::new();
        for &index in wanting.iter().take(budget as usize) {
            if let Some(planner) = self.take_planner(index) {
                searches.push(Ahead {
                    index,
                    kind: self.units[index].state.kind,
                    player: self.units[index].state.player,
                    planner,
                    share,
                    denied: AtomicBool::new(false),
                });
            }
        }
        let before: Vec<_> = searches
            .iter()
            .map(|s| (s.planner.work(), s.planner.certified_destination()))
            .collect();
        let probe = self.planning_probe.clone();
        let span = probe.as_ref().and_then(|p| p.movement_span("plan_ahead"));
        {
            let world = &*self;
            let none = Mutex::new(0);
            par_each_mut(&mut searches, |search| {
                world.plan_ahead_steps(search, &none)
            });
        }
        for search in &mut searches {
            if search.denied.swap(false, Ordering::Relaxed) {
                let budget = &self.navigation_preparation_budget;
                self.plan_ahead_steps(search, budget);
            }
        }
        drop(span);
        for (search, (work_before, certified_before)) in searches.into_iter().zip(before) {
            let Ahead {
                index,
                planner,
                kind,
                ..
            } = search;
            if certified_before.is_none() && planner.certified_destination().is_some() {
                self.route_metrics
                    .sample("certified-nearest", kind, &planner);
            }
            let spent = planner.work().saturating_sub(work_before);
            self.route_metrics.lookahead_work += u64::from(spent);
            self.demand_route_work(Some(self.units[index].state.id), 2, spent);
            let (from, to) = (planner.origin(), planner.destination());
            self.route_metrics
                .attribution
                .work(kind, from, to, 2, spent);
            self.store_planner(index, Some(planner));
        }
    }

    /// Spends what is left of `search`'s share, answering region proofs
    /// between steps of `REGION_PROOF_STEP`.
    fn plan_ahead_steps(&self, search: &mut Ahead, preparations: &Mutex<u32>) {
        let (kind, player) = (search.kind, search.player);
        loop {
            self.reuse_region_endpoint_proof(&mut search.planner, kind, player);
            let step = search.share.min(REGION_PROOF_STEP);
            let before = search.planner.work();
            self.plan_world_route_ahead_limited(
                &mut search.planner,
                step,
                &|| false,
                kind,
                player,
                preparations,
                Some(&search.denied),
            );
            let spent = search.planner.work().saturating_sub(before);
            search.share -= spent.min(search.share);
            // Short of its step, the planner stopped by itself.
            if spent < step || search.share == 0 || search.denied.load(Ordering::Relaxed) {
                break;
            }
        }
        self.reuse_region_endpoint_proof(&mut search.planner, kind, player);
    }
}
