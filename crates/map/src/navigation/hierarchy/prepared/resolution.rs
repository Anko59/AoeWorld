//! Budgeted exact component refinement. Cached answers never change its work
//! or traversal: replay progress depends on the captured collision alone.
use super::*;

pub(crate) const CHUNK_WORK: u32 = grid::CELLS as u32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResolutionOutcome {
    Destination(Option<TileCoord>),
    Bounded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Resolution {
    atlas: Arc<Atlas>,
    origin: TileCoord,
    destination: TileCoord,
    candidate: TileCoord,
    phase: u8,
    credit: u32,
    stage_work: u32,
    work: u64,
    chunks: BTreeSet<Key>,
    geometric: BTreeSet<(i64, Key)>,
    geometric_seen: BTreeSet<Key>,
    heap: BTreeSet<(i64, TileCoord, Option<Component>)>,
    parents: BTreeMap<Component, Option<Component>>,
    best: (i64, i32, i32),
    best_component: Option<Component>,
    outcome: Option<ResolutionOutcome>,
    /// A completed reverse flood proved these tiles cannot be reached from
    /// the origin. Excluding them from endpoint selection avoids flooding a
    /// whole continent merely to reject a small sealed destination pocket.
    excluded: BTreeSet<TileCoord>,
}

impl Resolution {
    pub(crate) fn overlay(&self) -> &ResourceOverlay {
        self.atlas.overlay()
    }
    pub(crate) fn new(atlas: Arc<Atlas>, origin: TileCoord, destination: TileCoord) -> Self {
        let key = chunk_of(destination);
        let key = (
            key.0.clamp(0, atlas.side - 1),
            key.1.clamp(0, atlas.side - 1),
        );
        Self {
            atlas,
            origin,
            destination,
            candidate: destination,
            phase: 0,
            credit: 0,
            stage_work: 0,
            work: 0,
            chunks: BTreeSet::new(),
            geometric: BTreeSet::from([(
                squared_distance(closest_in_chunk(key, destination), destination),
                key,
            )]),
            geometric_seen: BTreeSet::from([key]),
            heap: BTreeSet::new(),
            parents: BTreeMap::new(),
            best: (squared_distance(origin, destination), origin.y, origin.x),
            best_component: None,
            outcome: None,
            excluded: BTreeSet::new(),
        }
    }

    pub(crate) fn inherit_chunks(&mut self, chunks: BTreeSet<Key>) {
        self.chunks = chunks;
    }

    pub(crate) fn exclude_sealed(&mut self, tiles: BTreeSet<TileCoord>) {
        self.excluded = tiles;
    }

    pub(crate) fn into_search(
        self,
        origin: TileCoord,
        destination: TileCoord,
        maximum: u32,
    ) -> super::super::incremental::Search {
        super::super::incremental::Search::with_preparation(
            origin,
            destination,
            maximum,
            self.atlas,
            self.chunks,
        )
    }

    fn next_key(&self) -> Option<Key> {
        match self.phase {
            0 => Some(chunk_of(self.origin)),
            1 => self
                .geometric
                .first()
                .filter(|entry| entry.0 <= self.best.0)
                .map(|entry| entry.1),
            _ => self.heap.first().map(|entry| chunk_of(entry.1)),
        }
        .filter(|key| self.atlas.contains(*key))
    }

    pub(crate) fn preparation_tile(&self) -> Option<TileCoord> {
        self.next_key().map(|key| grid::tile(key, 0))
    }

    pub(crate) fn retained_entries(&self) -> usize {
        self.chunks.len()
            + self.geometric.len()
            + self.geometric_seen.len()
            + self.heap.len()
            + self.parents.len()
            + self.excluded.len()
    }

    pub(crate) fn touches(&self, area: aoe_core::TileRect) -> bool {
        self.chunks
            .iter()
            .copied()
            .chain(self.next_key())
            .any(|key| {
                let low = grid::tile(key, 0);
                low.x < area.max.x
                    && low.y < area.max.y
                    && low.x + CHUNK_TILES > area.min.x
                    && low.y + CHUNK_TILES > area.min.y
            })
    }

    pub(crate) fn poll(
        &mut self,
        generator: &MapChunkGenerator,
        remaining: &mut u32,
        limits: Option<(u32, usize)>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<ResolutionOutcome>> {
        self.poll_budgeted(generator, remaining, limits, cancelled, &|| true)
    }

    pub(crate) fn poll_budgeted(
        &mut self,
        generator: &MapChunkGenerator,
        remaining: &mut u32,
        limits: Option<(u32, usize)>,
        cancelled: &dyn Fn() -> bool,
        prepare: &dyn Fn() -> bool,
    ) -> Result<Option<ResolutionOutcome>> {
        let _span = generator.measure_navigation(crate::NavigationPhase::Components);
        loop {
            if cancelled() {
                return Err(EnvironmentPageError::Cancelled);
            }
            if let Some(outcome) = self.outcome {
                return Ok(Some(outcome));
            }
            if limits.is_some_and(|(work, entries)| {
                (self.work >= u64::from(work)
                    && !(self.stage_work > 0 && self.credit == self.stage_work))
                    || self.retained_entries() > entries
            }) {
                self.outcome = Some(ResolutionOutcome::Bounded);
                continue;
            }
            if *remaining == 0 {
                return Ok(None);
            }
            if self.credit == 0 {
                let chunk = self
                    .next_key()
                    .is_some_and(|key| !self.chunks.contains(&key));
                self.stage_work = if chunk { CHUNK_WORK } else { 1 };
            }
            let allowed = limits.map_or(u32::MAX, |(maximum, _)| {
                u64::from(maximum)
                    .saturating_sub(self.work)
                    .min(u64::from(u32::MAX)) as u32
            });
            let take = (*remaining).min(self.stage_work - self.credit).min(allowed);
            self.credit += take;
            self.work += u64::from(take);
            *remaining -= take;
            if self.credit < self.stage_work {
                return Ok(None);
            }
            if self
                .next_key()
                .is_some_and(|key| !self.chunks.contains(&key))
                && !prepare()
            {
                return Ok(None);
            }
            self.credit = 0;
            if let Some(key) = self.next_key() {
                self.chunks.insert(key);
            }
            self.step(generator, cancelled)?;
        }
    }

    fn step(&mut self, generator: &MapChunkGenerator, cancelled: &dyn Fn() -> bool) -> Result<()> {
        if self.phase == 0 {
            let key = chunk_of(self.origin);
            if !self.atlas.contains(key) {
                self.outcome = Some(ResolutionOutcome::Destination(None));
                return Ok(());
            }
            let chunk = self.atlas.chunk(generator, key, cancelled)?;
            let label = chunk.label(grid::index(self.origin));
            if label == 0 {
                self.outcome = Some(ResolutionOutcome::Destination(None));
            } else {
                self.best_component = Some((key, label));
                self.phase = 1;
            }
            return Ok(());
        }
        if self.phase == 1 {
            return self.geometric_step(generator, cancelled);
        }
        self.component_step(generator, cancelled)
    }

    fn geometric_step(
        &mut self,
        generator: &MapChunkGenerator,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<()> {
        let Some((_, key)) = self
            .geometric
            .pop_first()
            .filter(|entry| entry.0 <= self.best.0)
        else {
            self.candidate = TileCoord::new(self.best.2, self.best.1);
            self.heap.insert((
                squared_distance(self.origin, self.candidate),
                self.origin,
                None,
            ));
            self.best = (
                squared_distance(self.origin, self.destination),
                self.origin.y,
                self.origin.x,
            );
            self.geometric.clear();
            self.geometric_seen.clear();
            self.phase = 2;
            return Ok(());
        };
        let chunk = self.atlas.chunk(generator, key, cancelled)?;
        // A component unreachable from every boundary entry and the origin
        // cannot be entered. Reject these certified local islands before
        // choosing an endpoint, including when the requested tile is blocked.
        // Keep incoming-only portals and follow directed local links; never
        // assume a one-way drop can be traversed backwards.
        let mut entered = BTreeSet::new();
        if inside(key, self.origin) {
            entered.insert(chunk.label(grid::index(self.origin)));
        }
        entered.extend(chunk.portals().map(|p| chunk.label(grid::index(p.tile))));
        let mut successors: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
        for (from, to) in chunk.links() {
            successors.entry(from).or_default().push(to);
        }
        let mut frontier: Vec<_> = entered.iter().copied().collect();
        while let Some(from) = frontier.pop() {
            for &to in successors.get(&from).into_iter().flatten() {
                if entered.insert(to) {
                    frontier.push(to);
                }
            }
        }
        let closest = closest_in_chunk(key, self.destination);
        let label = chunk.label(grid::index(closest));
        if label != 0 && entered.contains(&label) && !self.excluded.contains(&closest) {
            self.best = self.best.min((
                squared_distance(closest, self.destination),
                closest.y,
                closest.x,
            ));
        } else {
            for (at, label) in chunk.labels().enumerate() {
                if label != 0 && entered.contains(&label) {
                    let tile = grid::tile(key, at);
                    if self.excluded.contains(&tile) {
                        continue;
                    }
                    self.best =
                        self.best
                            .min((squared_distance(tile, self.destination), tile.y, tile.x));
                }
            }
        }
        for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            let next = (key.0 + dx, key.1 + dy);
            if self.atlas.contains(next) && self.geometric_seen.insert(next) {
                self.geometric.insert((
                    squared_distance(closest_in_chunk(next, self.destination), self.destination),
                    next,
                ));
            }
        }
        Ok(())
    }

    fn component_step(
        &mut self,
        generator: &MapChunkGenerator,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<()> {
        let Some((_, entry, parent)) = self.heap.pop_first() else {
            let result = TileCoord::new(self.best.2, self.best.1);
            self.atlas
                .remember(&self.parents, self.best_component, self.destination, result);
            self.outcome = Some(ResolutionOutcome::Destination(Some(result)));
            return Ok(());
        };
        let key = chunk_of(entry);
        if !self.atlas.contains(key) {
            return Ok(());
        }
        let chunk = self.atlas.chunk(generator, key, cancelled)?;
        let label = chunk.label(grid::index(entry));
        debug_assert!(label <= chunk.components());
        let component = (key, label);
        if label == 0 || self.parents.contains_key(&component) {
            return Ok(());
        }
        self.parents.insert(component, parent);
        if inside(key, self.candidate) && chunk.label(grid::index(self.candidate)) == label {
            self.atlas.remember(
                &self.parents,
                Some(component),
                self.destination,
                self.candidate,
            );
            self.outcome = Some(ResolutionOutcome::Destination(Some(self.candidate)));
            return Ok(());
        }
        if squared_distance(closest_in_chunk(key, self.destination), self.destination)
            <= self.best.0
        {
            for (at, local) in chunk.labels().enumerate() {
                if local == label {
                    let tile = grid::tile(key, at);
                    if self.excluded.contains(&tile) {
                        continue;
                    }
                    let next = (squared_distance(tile, self.destination), tile.y, tile.x);
                    if next < self.best {
                        self.best = next;
                        self.best_component = Some(component);
                    }
                }
            }
        }
        for (from, to) in chunk.links() {
            if from == label
                && let Some(at) = chunk.labels().position(|label| label == to)
            {
                let tile = grid::tile(key, at);
                self.heap.insert((
                    squared_distance(tile, self.candidate),
                    tile,
                    Some(component),
                ));
            }
        }
        for portal in chunk.portals() {
            if portal.crossing_cost > 0 && chunk.label(grid::index(portal.tile)) == label {
                self.heap.insert((
                    squared_distance(portal.cross, self.candidate),
                    portal.cross,
                    Some(component),
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn update_hash(&self, hash: &mut blake3::Hasher) {
        // Inherited chunk credit is authoritative progress, unlike the
        // generator's physical caches. Hash its canonical frontier as well.
        hash.update(b"lazy-components");
        hash.update(&(self.excluded.len() as u64).to_le_bytes());
        for tile in &self.excluded {
            hash.update(&tile.x.to_le_bytes());
            hash.update(&tile.y.to_le_bytes());
        }
        hash.update(&self.atlas.stamp());
        for tile in [self.origin, self.destination, self.candidate] {
            hash.update(&tile.x.to_le_bytes());
            hash.update(&tile.y.to_le_bytes());
        }
        hash.update(&[self.phase]);
        hash.update(&self.credit.to_le_bytes());
        hash.update(&self.stage_work.to_le_bytes());
        hash.update(&self.work.to_le_bytes());
        for keys in [&self.chunks, &self.geometric_seen] {
            hash.update(&(keys.len() as u64).to_le_bytes());
            for key in keys {
                hash.update(&key.0.to_le_bytes());
                hash.update(&key.1.to_le_bytes());
            }
        }
        hash.update(&(self.geometric.len() as u64).to_le_bytes());
        for (distance, key) in &self.geometric {
            hash.update(&distance.to_le_bytes());
            hash.update(&key.0.to_le_bytes());
            hash.update(&key.1.to_le_bytes());
        }
        hash.update(&(self.heap.len() as u64).to_le_bytes());
        for (distance, tile, parent) in &self.heap {
            hash.update(&distance.to_le_bytes());
            hash.update(&tile.x.to_le_bytes());
            hash.update(&tile.y.to_le_bytes());
            hash_component(hash, *parent);
        }
        hash.update(&(self.parents.len() as u64).to_le_bytes());
        for (component, parent) in &self.parents {
            hash_component(hash, Some(*component));
            hash_component(hash, *parent);
        }
        hash.update(&self.best.0.to_le_bytes());
        hash.update(&self.best.1.to_le_bytes());
        hash.update(&self.best.2.to_le_bytes());
        hash_component(hash, self.best_component);
        match self.outcome {
            None => {
                hash.update(&[0]);
            }
            Some(ResolutionOutcome::Bounded) => {
                hash.update(&[1]);
            }
            Some(ResolutionOutcome::Destination(None)) => {
                hash.update(&[2]);
            }
            Some(ResolutionOutcome::Destination(Some(tile))) => {
                hash.update(&[3]);
                hash.update(&tile.x.to_le_bytes());
                hash.update(&tile.y.to_le_bytes());
            }
        }
    }
}

fn hash_component(hash: &mut blake3::Hasher, component: Option<Component>) {
    hash.update(&[u8::from(component.is_some())]);
    if let Some((key, label)) = component {
        hash.update(&key.0.to_le_bytes());
        hash.update(&key.1.to_le_bytes());
        hash.update(&label.to_le_bytes());
    }
}
