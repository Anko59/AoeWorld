//! Candidate walks for village placement. A seat whose ideal point lies at sea
//! must not pay for every tile of open water between it and the coast: tiles
//! whose whole surroundings are proven ocean or lake are dropped from coarse
//! hydrology (microseconds), and the walk is pulled lazily so only as many
//! tiles are examined as the search consumes.
use super::{MatchError, ring_or_grid};
use aoe_core::{PlayerId, TileCoord};
use aoe_simulation::{EconomyError, GameWorld, GameWorldError, LandMasses, Terrain};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// SplitMix64: a small seeded generator, so a layout is a pure function of its seed.
pub(super) struct Rng(u64);

impl Rng {
    pub(super) fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n > 0`).
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1_u64 << 53) as f64
    }
}

/// A seed for a new match: the clock and a process counter, mixed.
pub(crate) fn fresh_seed() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos() as u64);
    let tick = COUNTER.fetch_add(1, Ordering::Relaxed);
    Rng::new(nanos ^ tick.wrapping_mul(0xD6E8_FEB8_6659_FD93) ^ u64::from(std::process::id()))
        .next()
}

/// The ideal village centre of each slot. Without a seed, the canonical
/// layout. With one, the ring is turned by a random angle (a grid cell's
/// centre is nudged by up to a sixth of the cell) and the slots are dealt the
/// positions in random order, so the human's seat differs from match to match
/// while every position is still used once. The same seed gives the same layout.
pub(super) fn match_ideals(side: i32, count: usize, seed: Option<u64>) -> Vec<TileCoord> {
    let Some(seed) = seed else {
        return ring_or_grid(side, count, std::f64::consts::FRAC_PI_4);
    };
    let mut rng = Rng::new(seed);
    let turn = rng.unit() * std::f64::consts::TAU;
    let mut ideals = ring_or_grid(side, count, turn);
    if count > 8 {
        let columns = (count as f64).sqrt().ceil() as i32;
        let cell = ((side - 2 * (side / 10).max(12)).max(1) / columns).max(6);
        let reach = (cell / 6).max(1) as usize;
        for ideal in &mut ideals {
            ideal.x += rng.below(2 * reach + 1) as i32 - reach as i32;
            ideal.y += rng.below(2 * reach + 1) as i32 - reach as i32;
        }
    }
    for at in (1..ideals.len()).rev() {
        ideals.swap(at, rng.below(at + 1));
    }
    ideals
}

/// A start search reaches 32 tiles from a candidate and the Town Center
/// stands up to five tiles beyond that: open water to this distance leaves no
/// site for the candidate, so skipping it never changes the outcome.
const OPEN_WATER_RADIUS: i32 = 40;
/// The farthest search walks every 16th ring and keeps every 24th dry tile
/// (the nearest dry tiles alone cluster on one coast, where a full kit may not
/// fit), at most 800 of them.
const RING_STEP: i32 = 16;
pub(super) const SPREAD: usize = 24;
const LIMIT: usize = 800;

/// Is `tile` far enough inside open water that no village can start near it?
pub(super) fn open_sea(world: &GameWorld, tile: TileCoord) -> bool {
    world.terrain().open_water_certain(tile, OPEN_WATER_RADIUS)
}

/// The tiles at Chebyshev distance `ring` from `center`, every second one.
pub(super) fn ring_tiles(center: TileCoord, ring: i32) -> Vec<TileCoord> {
    if ring == 0 {
        return vec![center];
    }
    let mut tiles = Vec::new();
    for offset in (-ring..=ring).step_by(2) {
        tiles.push(TileCoord::new(center.x + offset, center.y - ring));
        tiles.push(TileCoord::new(center.x + offset, center.y + ring));
        if offset.abs() != ring {
            tiles.push(TileCoord::new(center.x - ring, center.y + offset));
            tiles.push(TileCoord::new(center.x + ring, center.y + offset));
        }
    }
    tiles
}

/// Widening rings around an ideal point, spread over the walk. Pulled lazily
/// so no borrow of the world outlives a call.
pub(super) struct FarWalk {
    ideal: TileCoord,
    side: i32,
    ring: i32,
    pending: VecDeque<TileCoord>,
    dry: usize,
    yielded: usize,
    /// Tiles offered to the caller's cheap filter, for bounding the work.
    pub(super) examined: usize,
}

impl FarWalk {
    pub(super) fn new(ideal: TileCoord, side: i32) -> Self {
        Self {
            ideal,
            side,
            ring: 0,
            pending: VecDeque::new(),
            dry: 0,
            yielded: 0,
            examined: 0,
        }
    }

    /// The next spread-out tile that `skip` does not reject and `usable`
    /// accepts. `skip` is the cheap test (margin, spacing, proven open water);
    /// `usable` the costly passability test, which only tiles that survive
    /// `skip` ever reach.
    pub(super) fn next(
        &mut self,
        skip: &mut dyn FnMut(TileCoord) -> bool,
        usable: &mut dyn FnMut(TileCoord) -> bool,
    ) -> Option<TileCoord> {
        while self.yielded < LIMIT {
            let Some(tile) = self.pending.pop_front() else {
                if self.ring > self.side {
                    return None;
                }
                self.pending.extend(ring_tiles(self.ideal, self.ring));
                self.ring += RING_STEP;
                continue;
            };
            self.examined += 1;
            if skip(tile) || !usable(tile) {
                continue;
            }
            self.dry += 1;
            if (self.dry - 1).is_multiple_of(SPREAD) {
                self.yielded += 1;
                return Some(tile);
            }
        }
        None
    }
}

/// Gives every seat a village near its ideal point, searching outward in
/// square rings and keeping villages apart. Returns each seat's scout.
pub(super) fn place_villages(
    world: &mut GameWorld,
    count: usize,
    seed: Option<u64>,
    placed_count: &AtomicUsize,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<aoe_core::EntityId>, MatchError> {
    place_villages_observed(
        world,
        count,
        seed,
        placed_count,
        cancelled,
        &mut |_, _, _, _| {},
    )
}

/// `place_villages`, reporting each seat's (index, ideal, chosen, seconds).
pub(super) fn place_villages_observed(
    world: &mut GameWorld,
    count: usize,
    seed: Option<u64>,
    placed_count: &AtomicUsize,
    cancelled: &dyn Fn() -> bool,
    observe: &mut dyn FnMut(usize, TileCoord, TileCoord, f64),
) -> Result<Vec<aoe_core::EntityId>, MatchError> {
    let side = world.config().width_tiles;
    let ideals = match_ideals(side, count, seed);
    // No transport ships: every village must stand on the main continent, or
    // its player could never reach the others. One flood fill, then lookups.
    let masses = world.terrain().land_masses(world.config());
    let reach = if count <= 8 {
        side / 6
    } else {
        side / (2 * (count as f64).sqrt().ceil() as i32)
    }
    .max(8);
    let separation = (reach / 2).max(10);
    let mut placed: Vec<TileCoord> = Vec::new();
    let mut scouts = Vec::new();
    for (index, ideal) in ideals.into_iter().enumerate() {
        let started = std::time::Instant::now();
        // Candidate tiles in widening rings, away from earlier villages. Lazy:
        // on a large map the full set is millions of tiles and the nearest
        // few almost always suffice.
        let candidates = || {
            (0..=reach)
                .step_by(2)
                .flat_map(|ring| ring_tiles(ideal, ring))
                .filter(|tile| usable(side, separation, &placed, *tile))
        };
        // Prefer ground connected to a large walkable region; terrain whose
        // cliffs leave no such place (generated maps) falls back to any site
        // a village fits on. Certifying is costly: a quick look first. Open
        // sea is skipped from coarse hydrology, never by sampling it.
        let player = PlayerId(index as u16);
        let near: Vec<TileCoord> = candidates().take(48).collect();
        let near = dry(world, &masses, near);
        let mut found = settle(world, player, near.into_iter(), true, cancelled)?;
        if found.is_none() {
            let near: Vec<TileCoord> = candidates().take(2_000).collect();
            let near = dry(world, &masses, near);
            found = settle(world, player, near.into_iter(), false, cancelled)?;
        }
        if found.is_none() {
            // The ideal point lies in the sea or on bare cliffs (a country on a
            // square map): take the nearest dry ground anywhere on the map.
            // The walk is lazy and skips open water, so a seat anchored far at
            // sea costs the coast it reaches, not the sea in between.
            let mut walk = FarWalk::new(ideal, side);
            let mut far = Vec::new();
            'region: loop {
                let mut batch = Vec::new();
                while batch.len() < 3 {
                    let tile = walk.next(
                        &mut |tile| {
                            !usable(side, separation, &placed, tile)
                                || open_sea(world, tile)
                                || !Terrain::on_main_land(masses.as_ref(), tile)
                        },
                        &mut |tile| world.tile_passable(tile),
                    );
                    match tile {
                        Some(tile) => batch.push(tile),
                        None => break,
                    }
                }
                if batch.is_empty() {
                    break;
                }
                far.extend(batch.iter().copied());
                // Pass one wants a large walkable region around the tile.
                found = settle(world, player, batch.into_iter(), true, cancelled)?;
                if found.is_some() {
                    break 'region;
                }
            }
            if found.is_none() {
                found = settle(world, player, far.iter().copied(), false, cancelled)?;
            }
        }
        let (tile, scout) = found.ok_or(MatchError::NoStart(index))?;
        let seconds = started.elapsed().as_secs_f64();
        observe(index, ideal, tile, seconds);
        // Villages whose ideal point lies at sea dominate match creation.
        if seconds > 2.0 {
            tracing::info!(
                slot = index,
                ?ideal,
                ?tile,
                seconds,
                "slow village placement"
            );
        }
        placed.push(tile);
        scouts.push(scout);
        placed_count.store(scouts.len(), Ordering::Release);
    }
    Ok(scouts)
}

/// Settles a village for `player` on the first of `tiles` that takes it.
/// `require_region` keeps only sites connected to a large walkable region.
fn settle(
    world: &mut GameWorld,
    player: PlayerId,
    tiles: impl Iterator<Item = TileCoord>,
    require_region: bool,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<(TileCoord, aoe_core::EntityId)>, MatchError> {
    let mut tiles = tiles.peekable();
    while tiles.peek().is_some() {
        if cancelled() {
            return Err(GameWorldError::StartSearchLimit.into());
        }
        // Certify a few sites against the terrain, then try to settle one.
        let batch = {
            let config = world.config();
            let mut certifier = world.terrain().start_certifier(config, cancelled);
            let mut batch = Vec::new();
            while batch.len() < 3 {
                let Some(tile) = tiles.next() else { break };
                if !require_region
                    || certifier
                        .certify(tile)
                        .map_err(GameWorldError::Environment)?
                {
                    batch.push(tile);
                }
            }
            batch
        };
        for tile in batch {
            match world.initialize_village(player, tile) {
                Ok(scout) => return Ok(Some((tile, scout))),
                Err(EconomyError::InvalidFootprint) => {}
                Err(EconomyError::Movement(error)) => return Err(error.into()),
                Err(error) => {
                    tracing::warn!(?tile, %error, slot = player.0, "village refused");
                    return Err(MatchError::NoStart(player.0 as usize));
                }
            }
        }
    }
    Ok(None)
}

/// Is `tile` inside the walkable margin and clear of earlier villages?
fn usable(side: i32, separation: i32, placed: &[TileCoord], tile: TileCoord) -> bool {
    tile.x >= 4
        && tile.y >= 4
        && tile.x < side - 4
        && tile.y < side - 4
        && !placed
            .iter()
            .any(|other| (other.x - tile.x).abs().max((other.y - tile.y).abs()) < separation)
}

/// `tiles` without those lying far inside open water or off the main continent.
fn dry(
    world: &GameWorld,
    masses: &Option<LandMasses>,
    mut tiles: Vec<TileCoord>,
) -> Vec<TileCoord> {
    tiles.retain(|tile| !open_sea(world, *tile) && Terrain::on_main_land(masses.as_ref(), *tile));
    tiles
}
