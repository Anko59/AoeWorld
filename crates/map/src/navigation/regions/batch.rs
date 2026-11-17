//! Many reachability questions that share an origin or a destination. The
//! answers are exactly `Regions::reachable`'s; only the reachable closure,
//! which visits every gated link, is built once per start region instead of
//! once per question.
use super::*;

impl Regions {
    /// `reachable` from one origin to each of `to`, with the same answers.
    /// The origin's reachable closure, which visits every gated link, is
    /// built once rather than once per goal.
    pub fn reachable_each(
        &mut self,
        ground: &dyn RegionGround,
        player: u32,
        from: TileCoord,
        to: &[TileCoord],
    ) -> Vec<bool> {
        if self.chunks.is_empty() {
            return to
                .iter()
                .map(|&tile| self.reachable(ground, player, from, tile))
                .collect();
        }
        self.flush(ground);
        let roots = self.reachable_roots(player, from);
        to.iter()
            .map(|&tile| {
                self.node(tile).is_some_and(|goal| {
                    let goal = self.global.root(goal);
                    roots.contains(&self.closure_root(player, goal))
                })
            })
            .collect()
    }

    /// Whether every one of `from` reaches `to`, as `reachable` answers
    /// each; it stops at the first that does not. Starts in one region share
    /// one closure: it depends only on the start's region and its gate.
    pub fn reachable_from_all(
        &mut self,
        ground: &dyn RegionGround,
        player: u32,
        from: &[TileCoord],
        to: TileCoord,
    ) -> bool {
        if self.chunks.is_empty() {
            return from
                .iter()
                .all(|&tile| self.reachable(ground, player, tile, to));
        }
        self.flush(ground);
        let Some(goal) = self.node(to) else {
            return from.is_empty();
        };
        let goal = self.closure_root(player, self.global.root(goal));
        let mut reached = BTreeSet::<(u32, u16)>::new();
        for &tile in from {
            let Some(region) = self.region_at(tile) else {
                return false;
            };
            if reached.contains(&region) {
                continue;
            }
            if !self.reachable_roots(player, tile).contains(&goal) {
                return false;
            }
            reached.insert(region);
        }
        true
    }

    /// `reachable` from each of `from` to one destination, with the same
    /// answers. Starts in one region share one closure: it depends only on
    /// the start's region and its gate.
    pub fn reachable_from_each(
        &mut self,
        ground: &dyn RegionGround,
        player: u32,
        from: &[TileCoord],
        to: TileCoord,
    ) -> Vec<bool> {
        if self.chunks.is_empty() {
            return from
                .iter()
                .map(|&tile| self.reachable(ground, player, tile, to))
                .collect();
        }
        self.flush(ground);
        let Some(goal) = self.node(to) else {
            return vec![false; from.len()];
        };
        let goal = self.closure_root(player, self.global.root(goal));
        let mut known = BTreeMap::<(u32, u16), bool>::new();
        from.iter()
            .map(|&tile| {
                let Some(region) = self.region_at(tile) else {
                    return false;
                };
                *known
                    .entry(region)
                    .or_insert_with(|| self.reachable_roots(player, tile).contains(&goal))
            })
            .collect()
    }
}
