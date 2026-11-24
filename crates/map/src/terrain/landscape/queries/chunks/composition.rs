//! Terrain, scenery and rock placement share one composition memo.
use super::*;

pub(super) struct Composition<'a, const N: usize, const W: usize> {
    pub(super) generator: &'a MapChunkGenerator,
    pub(super) memo: &'a memo::Memo<'a, N, W>,
    pub(super) policy_at: &'a dyn Fn(TileCoord, Tile) -> LandscapePolicy,
    pub(super) reserved_at: &'a dyn Fn(TileCoord) -> Reservations,
    pub(super) cancelled: &'a dyn Fn() -> bool,
    pub(super) memoize_rocks: bool,
}

impl<const N: usize, const W: usize> Composition<'_, N, W> {
    fn policy(&self, position: TileCoord, base: Tile) -> LandscapePolicy {
        let mut policy = (self.policy_at)(position, base);
        // Explicit temperate-summer model policy, not reconstructed climate
        // evidence. Keep all source heights/corners and passability intact.
        if policy.support_per_thousand <= 1000
            && matches!(
                base.biome,
                Biome::Temperate | Biome::Boreal | Biome::Woodland
            )
            && base.geographic_height_centimeters >= 230_000
        {
            policy.support_per_thousand = 0;
        }
        // No forest on snow or bare rock, wherever the snow line is.
        if policy.support_per_thousand <= 1000
            && self
                .generator
                .ground_cover(position, base.geographic_height_centimeters, base.biome)
                .is_some()
        {
            policy.support_per_thousand = 0;
        }
        policy
    }

    pub(super) fn reservations(
        &self,
        position: TileCoord,
    ) -> Result<Reservations, EnvironmentPageError> {
        self.memo.reservations_with(position, || {
            let mut reservations = (self.reserved_at)(position);
            reservations.resource_approach |= self.memo.reserved(position)?;
            reservations.shore |= matches!(
                self.memo.shore(position)?,
                memo::Shore::Beach | memo::Shore::Bank
            );
            Ok(reservations)
        })
    }

    pub(super) fn sample(
        &self,
        at: TileCoord,
    ) -> Result<Option<(LandscapeSample, Tile)>, EnvironmentPageError> {
        let memo = self.memo;
        memo.landscape(at, || {
            self.generator.evaluate_landscape_inner_with_base(
                at,
                &|position, base| self.policy(position, base),
                &|position| self.reservations(position),
                self.cancelled,
                memo,
            )
        })
    }

    pub(super) fn rock_anchor(&self, at: TileCoord) -> Result<bool, EnvironmentPageError> {
        dressing::rock_anchor(
            self.generator,
            self.memo,
            at,
            &|at| self.sample(at),
            &|at| self.reservations(at),
        )
    }

    pub(super) fn rock_coverage(
        &self,
        position: TileCoord,
    ) -> Result<(bool, bool), EnvironmentPageError> {
        // Prefer the centre for the art, then stop at the first covering pile.
        let anchors = || {
            std::iter::once(position)
                .chain(dressing::rock_footprint(position).filter(|at| *at != position))
        };
        let memoize = cfg!(not(target_arch = "wasm32")) && self.memoize_rocks;
        let mut cached = [None; dressing::ROCK_TILES];
        if memoize && let Ok(memo) = self.generator.derived.point.lock() {
            for (slot, at) in cached.iter_mut().zip(anchors()) {
                *slot = memo.get_rock(at);
            }
        }
        let mut updates = [None; dressing::ROCK_TILES];
        let mut coverage = (false, false);
        for ((at, known), update) in anchors().zip(cached).zip(&mut updates) {
            let rock = match known {
                Some(rock) => rock,
                None => {
                    let rock = self.rock_anchor(at)?;
                    *update = Some((at, rock));
                    rock
                }
            };
            if rock {
                coverage = (at == position, true);
                break;
            }
        }
        // Never hold the cache lock during terrain or provider queries.
        if memoize
            && updates.iter().any(Option::is_some)
            && let Ok(mut memo) = self.generator.derived.point.lock()
        {
            for (at, rock) in updates.into_iter().flatten() {
                memo.insert_rock(at, rock);
            }
        }
        Ok(coverage)
    }
}
