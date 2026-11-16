//! Changes to a fog view the client already holds.
use super::*;
use std::ops::Range;

/// A run of rewritten tile bytes goes on across this many unchanged ones:
/// starting another run costs two bytes, carrying an unchanged byte one.
const JOIN: usize = 1;
/// Tiles compared at once while looking for changes: whole tile bytes.
const BLOCK: usize = 64;
const _: () = assert!(BLOCK.is_multiple_of(4));

/// What changed in the fog view of one subscription since the client was last
/// told. The server sends it in place of the whole `FogState`; the client
/// lays it over the view it holds. Tiles travel as in a whole view, four to a
/// byte, but only the runs of bytes that hold a changed tile.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FogDelta {
    pub revision: u64,
    pub tick: Tick,
    /// Runs of rewritten tile bytes, in order: the bytes left as they are
    /// since the previous run, then the bytes of this one.
    #[serde(deserialize_with = "bounded_runs")]
    runs: Vec<(u32, u32)>,
    /// The bytes of every run, end to end.
    #[serde(deserialize_with = "bounded_packed")]
    states: Vec<u8>,
    /// Strategic pixels with their new state, ascending; 0 is black again.
    #[serde(deserialize_with = "bounded_overview")]
    overview: Vec<(u16, u8)>,
}

/// Why a view refused a delta. The view is then exactly as it was.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum FogError {
    #[error("the fog changes are for another view")]
    AnotherView,
    #[error("the fog changes are inconsistent")]
    Malformed,
    #[error("the fog changes reach outside the window")]
    OutsideWindow,
}

impl FogDelta {
    /// Nothing changed: there is nothing to send.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty() && self.overview.is_empty()
    }

    /// Some tile of the window changed, not only the strategic view.
    pub fn changes_tiles(&self) -> bool {
        !self.runs.is_empty()
    }

    /// Bounded and consistent, whatever view it is laid over.
    pub fn valid(&self) -> bool {
        self.revision > 0 && self.extent().is_some() && self.pixels_valid()
    }

    /// One past the last tile byte the runs rewrite, when the runs and their
    /// bytes agree and every state in them exists.
    fn extent(&self) -> Option<usize> {
        let (mut end, mut bytes) = (0_usize, 0_usize);
        for &(skip, len) in &self.runs {
            if len == 0 {
                return None;
            }
            end = end.checked_add(skip as usize)?.checked_add(len as usize)?;
            bytes = bytes.checked_add(len as usize)?;
        }
        let known = self.states.iter().all(|byte| !unknown_state(*byte));
        (bytes == self.states.len() && end <= MAX_PACKED && known).then_some(end)
    }

    fn pixels_valid(&self) -> bool {
        self.overview.iter().all(|(_, state)| *state <= 2)
            && self.overview.windows(2).all(|pair| pair[0].0 < pair[1].0)
    }

    /// Adds the run of tile bytes `run` of the view `tiles`; the previous run
    /// ended at the byte `after`.
    fn push_run(&mut self, after: usize, run: &Range<usize>, tiles: &[u8]) {
        self.runs
            .push(((run.start - after) as u32, run.len() as u32));
        let end = tiles.len().min(4 * run.end);
        self.states
            .extend(tiles[4 * run.start..end].chunks(4).map(pack));
    }
}

impl FogState {
    /// The changes from this view, which a client holds, to `fresh`. `None`
    /// when `fresh` is another subscription, window or fog mode: that view is
    /// sent whole.
    pub fn delta_to(&self, fresh: &FogState) -> Option<FogDelta> {
        let same_view = self.revision == fresh.revision
            && self.region == fresh.region
            && self.disabled == fresh.disabled
            && self.tiles.len() == fresh.tiles.len();
        if !same_view {
            return None;
        }
        let mut delta = FogDelta {
            revision: fresh.revision,
            tick: fresh.tick,
            runs: Vec::new(),
            states: Vec::new(),
            overview: pixel_changes(&self.overview, &fresh.overview),
        };
        // The open run and the end of the one before it, in tile bytes.
        let (mut run, mut after) = (0..0, 0);
        let blocks = self.tiles.chunks(BLOCK).zip(fresh.tiles.chunks(BLOCK));
        for (block, (held, now)) in blocks.enumerate() {
            if held == now {
                continue;
            }
            for (byte, (held, now)) in held.chunks(4).zip(now.chunks(4)).enumerate() {
                if held == now {
                    continue;
                }
                let at = block * (BLOCK / 4) + byte;
                if !run.is_empty() && at - run.end <= JOIN {
                    run.end = at + 1;
                    continue;
                }
                if !run.is_empty() {
                    delta.push_run(after, &run, &fresh.tiles);
                    after = run.end;
                }
                run = at..at + 1;
            }
        }
        if !run.is_empty() {
            delta.push_run(after, &run, &fresh.tiles);
        }
        Some(delta)
    }

    /// Lays `delta` over this view. An error leaves the view exactly as it
    /// was: the changes are for another subscription or this view has fog
    /// switched off, they are inconsistent, or they reach outside the window.
    pub fn apply(&mut self, delta: &FogDelta) -> Result<(), FogError> {
        if self.disabled || self.revision != delta.revision {
            return Err(FogError::AnotherView);
        }
        let end = delta
            .extent()
            .filter(|_| delta.pixels_valid())
            .ok_or(FogError::Malformed)?;
        // The last byte of a window may hold fewer than four tiles.
        let (bytes, spare) = (self.tiles.len().div_ceil(4), self.tiles.len() % 4);
        let past_the_end = end == bytes
            && spare != 0
            && delta
                .states
                .last()
                .is_some_and(|byte| byte >> (2 * spare) != 0);
        if end > bytes || past_the_end {
            return Err(FogError::OutsideWindow);
        }
        let (mut at, mut states) = (0, delta.states.as_slice());
        for &(skip, len) in &delta.runs {
            at += skip as usize;
            let (run, rest) = states.split_at(len as usize);
            for (tiles, byte) in self.tiles[4 * at..].chunks_mut(4).zip(run) {
                for (slot, tile) in tiles.iter_mut().enumerate() {
                    *tile = (byte >> (2 * slot)) & 3;
                }
            }
            at += run.len();
            states = rest;
        }
        lay_pixels(&mut self.overview, &delta.overview);
        self.tick = delta.tick;
        Ok(())
    }
}

/// The pixels whose state differs between two ascending overviews, with
/// their state in `fresh`; a pixel that is no longer listed turns 0.
fn pixel_changes(held: &[(u16, u8)], fresh: &[(u16, u8)]) -> Vec<(u16, u8)> {
    // Sight moves in one place: leave out what both lists share at each end.
    let head = held.iter().zip(fresh).take_while(|(a, b)| a == b).count();
    let (held, fresh) = (&held[head..], &fresh[head..]);
    let tail = held.iter().rev().zip(fresh.iter().rev());
    let tail = tail.take_while(|(a, b)| a == b).count();
    let (held, fresh) = (&held[..held.len() - tail], &fresh[..fresh.len() - tail]);
    let mut changes = Vec::new();
    let mut held = held.iter().copied().peekable();
    for &(pixel, state) in fresh {
        while let Some((gone, _)) = held.next_if(|(at, _)| *at < pixel) {
            changes.push((gone, 0));
        }
        if held.next_if_eq(&(pixel, state)).is_none() {
            held.next_if(|(at, _)| *at == pixel);
            changes.push((pixel, state));
        }
    }
    changes.extend(held.map(|(gone, _)| (gone, 0)));
    changes
}

/// Writes ascending pixel changes into an ascending overview, in one pass
/// over its entries from the first changed pixel on.
fn lay_pixels(overview: &mut Vec<(u16, u8)>, changes: &[(u16, u8)]) {
    let Some(&(first, _)) = changes.first() else {
        return;
    };
    let from = overview.partition_point(|(pixel, _)| *pixel < first);
    let mut held = overview.split_off(from).into_iter().peekable();
    for &(pixel, state) in changes {
        while let Some(entry) = held.next_if(|(at, _)| *at < pixel) {
            overview.push(entry);
        }
        held.next_if(|(at, _)| *at == pixel);
        if state != 0 {
            overview.push((pixel, state));
        }
    }
    overview.extend(held);
}
