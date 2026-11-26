//! The land-query budget and caches, and the build-site budget of one decision.
use aoe_sdk::BuildingKind;

use super::Context;
use crate::geometry::dist;
use crate::view::Tile;

impl Context<'_> {
    pub fn land_budget_exhausted(&self) -> bool {
        self.land_queries >= self.land_query_limit
    }

    pub fn land_route(&mut self, origin: Tile, target: Tile) -> Option<Vec<Tile>> {
        let key = (origin, target);
        if let Some(route) = self.land_routes.get(&key) {
            return route.clone();
        }
        if self.board.land_failures.contains_key(&key) || self.land_budget_exhausted() {
            return None;
        }
        if self.late_economy && !(self.world.land_known(origin) && self.world.land_known(target)) {
            self.land_routes.insert(key, None);
            return None;
        }
        self.land_queries += 1;
        let route = self.world.land_route(origin, target);
        if route.is_none() {
            if self.board.land_failures.len() >= self.config.land_cache_entries {
                self.board.land_failures.clear();
            }
            let wait = self.config.land_retry_seconds * u64::from(self.world.tick_hz());
            self.board.land_failures.insert(key, self.tick + wait);
        }
        self.land_routes.insert(key, route.clone());
        route
    }

    /// True when `land_route` returned `None` only because no query was made.
    pub fn land_unanswered(&self, origin: Tile, target: Tile) -> bool {
        let key = (origin, target);
        !self.land_routes.contains_key(&key) && !self.board.land_failures.contains_key(&key)
    }

    /// A short proven land hop, shared by marching and rallying soldiers.
    pub fn land_hop(&mut self, origin: Tile, target: Tile) -> Tile {
        let Some(path) = self.land_route(origin, target) else {
            return origin;
        };
        let (mut length, mut stop) = (0.0, origin);
        for pair in path.windows(2) {
            length += dist(pair[0], pair[1]);
            if length > f64::from(self.config.march_hop_tiles) {
                break;
            }
            stop = pair[1];
        }
        let target = self.unrefused(stop);
        if target == stop || self.land_route(origin, target).is_some() {
            target
        } else {
            origin
        }
    }

    pub fn land_frontier(&mut self, origin: Tile, target: Tile) -> Option<Tile> {
        if self.land_budget_exhausted() || (self.late_economy && !self.world.land_known(origin)) {
            return None;
        }
        self.land_queries += 1;
        self.world.land_frontier(origin, target)
    }

    pub fn find_build_site(
        &mut self,
        kind: BuildingKind,
        near: Tile,
        max_radius: i32,
        gap: i32,
    ) -> Option<Tile> {
        if self.site_queries >= self.config.site_queries_per_decision {
            return None;
        }
        self.site_queries += 1;
        // A site the server refused stays refused: ask for the next nearest,
        // or the one nearest refused site would answer every search.
        let name = format!("{kind:?}");
        let avoid: Vec<Tile> = self
            .board
            .bad_sites
            .iter()
            .filter(|(refused, ..)| *refused == name)
            .map(|&(_, x, y)| (x, y))
            .collect();
        self.world
            .find_build_site(kind, near, max_radius, gap, &avoid)
    }
}
