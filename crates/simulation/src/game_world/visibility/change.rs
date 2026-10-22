//! One observer change applied to one player's sight. A full disc updates
//! every tile of an observer that appears, dies or changes its line of sight;
//! a disc-edge diff updates only the tiles entering and leaving a moved
//! observer. Both leave their sight flips and newly visible tiles in a
//! `Scratch` for the caller's building-watch and resource memory.
use super::state::PlayerVision;
use super::Observer;
use aoe_core::{PlayerId, TileCoord};

/// Working buffers reused across changes; never hashed.
#[derive(Debug, Default)]
pub(super) struct Scratch {
    pub(super) flags: Vec<bool>,
    pub(super) flips: Vec<(TileCoord, bool)>,
    pub(super) entered: Vec<TileCoord>,
}

/// One observer change, in the order the simulation made it.
#[derive(Clone, Copy, Debug)]
pub(super) enum Change {
    /// An observer that appears (`add`) or disappears: its whole disc flips.
    Disc { observer: Observer, add: bool },
    /// A moved observer with the same owner and line of sight.
    Edge { old: Observer, new: Observer },
}

impl Change {
    pub(super) fn player(self) -> PlayerId {
        match self {
            Change::Disc { observer, .. } => observer.player,
            Change::Edge { new, .. } => new.player,
        }
    }
}

impl PlayerVision {
    /// Applies `change` to this player's sight, leaving the sight flips and the
    /// tiles that became visible in `scratch`.
    pub(super) fn apply_change(&mut self, change: Change, size: (i32, i32), scratch: &mut Scratch) {
        match change {
            Change::Disc { observer, add } => self.apply_disc(observer, add, size, scratch),
            Change::Edge { old, new } => self.apply_edge(old, new, size, scratch),
        }
    }

    fn apply_disc(
        &mut self,
        observer: Observer,
        add: bool,
        (width, height): (i32, i32),
        scratch: &mut Scratch,
    ) {
        self.revision += 1;
        let radius = i32::from(observer.los);
        scratch.flips.clear();
        scratch.entered.clear();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let tile = TileCoord::new(observer.center.x + dx, observer.center.y + dy);
                if tile.x < 0 || tile.y < 0 || tile.x >= width || tile.y >= height {
                    continue;
                }
                self.tile_work += 1;
                let flipped = if add {
                    self.raise_visible(tile)
                } else {
                    self.lower_visible(tile)
                };
                if !flipped {
                    continue;
                }
                if add {
                    self.set_explored(tile);
                    scratch.entered.push(tile);
                }
                self.touch_overview(tile, add, width, height);
                scratch.flips.push((tile, add));
            }
        }
    }

    /// Disc-edge update: only the entering and leaving tiles of a moved
    /// observer touch counters, exploration and overview. Flag slots are
    /// offsets from the old center, so both discs share them.
    fn apply_edge(
        &mut self,
        old: Observer,
        new: Observer,
        (width, height): (i32, i32),
        scratch: &mut Scratch,
    ) {
        let Scratch {
            flags,
            flips,
            entered,
        } = scratch;
        let radius = i32::from(old.los);
        let span = (2 * radius + 1) as usize;
        flags.clear();
        flags.resize(span * span, false);
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let tile = TileCoord::new(old.center.x + dx, old.center.y + dy);
                if tile.x >= 0 && tile.y >= 0 && tile.x < width && tile.y < height {
                    flags[((dy + radius) as usize) * span + (dx + radius) as usize] = true;
                }
            }
        }
        let (shift_x, shift_y) = (new.center.x - old.center.x, new.center.y - old.center.y);
        self.revision += 1;
        flips.clear();
        entered.clear();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let tile = TileCoord::new(new.center.x + dx, new.center.y + dy);
                if tile.x < 0 || tile.y < 0 || tile.x >= width || tile.y >= height {
                    continue;
                }
                // The same tile in old-disc coordinates.
                let (ox, oy) = (shift_x + dx, shift_y + dy);
                if ox.abs() <= radius && oy.abs() <= radius {
                    let slot = ((oy + radius) as usize) * span + (ox + radius) as usize;
                    if flags[slot] {
                        flags[slot] = false;
                        continue;
                    }
                }
                self.tile_work += 1;
                if self.raise_visible(tile) {
                    self.set_explored(tile);
                    flips.push((tile, true));
                    entered.push(tile);
                    self.touch_overview(tile, true, width, height);
                }
            }
        }
        for (slot, leaving) in flags.iter().enumerate() {
            if !leaving {
                continue;
            }
            let (dy, dx) = ((slot / span) as i32 - radius, (slot % span) as i32 - radius);
            let tile = TileCoord::new(old.center.x + dx, old.center.y + dy);
            self.tile_work += 1;
            if self.lower_visible(tile) {
                flips.push((tile, false));
                self.touch_overview(tile, false, width, height);
            }
        }
    }
}
