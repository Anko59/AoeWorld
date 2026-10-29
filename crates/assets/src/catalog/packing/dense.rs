//! Bounded MaxRects retry for layouts fragmented by shelf packing.
use super::*;

const MAX_FREE: usize = 4 * MAX_SELECTED_FRAMES + 1;

#[derive(Clone, Copy)]
struct Rect {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
}
impl Rect {
    fn right(self) -> u16 {
        self.x + self.w
    }
    fn bottom(self) -> u16 {
        self.y + self.h
    }
    fn contains(self, other: Self) -> bool {
        self.x <= other.x
            && self.y <= other.y
            && self.right() >= other.right()
            && self.bottom() >= other.bottom()
    }
}

pub(super) struct DensePage {
    free: Vec<Rect>,
}
impl DensePage {
    pub fn new() -> Self {
        Self {
            free: vec![Rect {
                x: START,
                y: START,
                w: PAGE_SIDE - START - 1,
                h: PAGE_SIDE - START - 1,
            }],
        }
    }

    pub fn place(&mut self, width: u16, height: u16) -> Option<(u16, u16)> {
        let (w, h) = (width + GAP, height + GAP);
        let (_, rect) = self
            .free
            .iter()
            .enumerate()
            .filter(|(_, r)| w <= r.w && h <= r.h)
            .min_by_key(|(i, r)| ((r.w - w).min(r.h - h), (r.w - w).max(r.h - h), *i))?;
        let used = Rect {
            x: rect.x,
            y: rect.y,
            w,
            h,
        };
        let mut next = Vec::new();
        let mut added = Vec::new();
        let mut push = |r, fresh| {
            if next.len() == MAX_FREE {
                return None;
            }
            if fresh {
                added.push(next.len());
            }
            next.push(r);
            Some(())
        };
        for &r in &self.free {
            if used.x >= r.right()
                || used.right() <= r.x
                || used.y >= r.bottom()
                || used.bottom() <= r.y
            {
                push(r, false)?;
                continue;
            }
            if used.x > r.x {
                push(
                    Rect {
                        w: used.x - r.x,
                        ..r
                    },
                    true,
                )?;
            }
            if used.right() < r.right() {
                push(
                    Rect {
                        x: used.right(),
                        w: r.right() - used.right(),
                        ..r
                    },
                    true,
                )?;
            }
            if used.y > r.y {
                push(
                    Rect {
                        h: used.y - r.y,
                        ..r
                    },
                    true,
                )?;
            }
            if used.bottom() < r.bottom() {
                push(
                    Rect {
                        y: used.bottom(),
                        h: r.bottom() - used.bottom(),
                        ..r
                    },
                    true,
                )?;
            }
        }
        // Unchanged regions were already mutually maximal. Compare only new
        // split regions against the complete set, retaining the exact stable
        // region order without the cubic old-pair rescans at native-pack size.
        let mut removed = vec![false; next.len()];
        for i in added {
            removed[i] = next
                .iter()
                .enumerate()
                .any(|(j, r)| i != j && r.contains(next[i]) && (!next[i].contains(*r) || j < i));
        }
        let mut i = 0;
        next.retain(|_| {
            let keep = !removed[i];
            i += 1;
            keep
        });
        self.free = next;
        Some((used.x, used.y))
    }
}

pub(super) fn pack(
    frames: &[FrameExtent],
    indices: &[u16],
    placements: &mut [Placement],
    terrain_pages: usize,
    page_count: usize,
) -> Result<(), PackingError> {
    let mut pages: Vec<DensePage> = (0..page_count).map(|_| DensePage::new()).collect();
    for &index in indices {
        let index = usize::from(index);
        let frame = frames[index];
        let (primary, spare) = match frame.domain {
            AtlasDomain::Terrain => (0..terrain_pages, 0..0),
            AtlasDomain::Objects => (terrain_pages..page_count, 0..0),
            AtlasDomain::Shared => (terrain_pages..page_count, 0..terrain_pages),
        };
        let mut placed = false;
        for page in primary.chain(spare) {
            if let Some((x, y)) = pages[page].place(frame.width, frame.height) {
                placements[index] = Placement {
                    page: page as u16,
                    x,
                    y,
                    width: frame.width,
                    height: frame.height,
                };
                placed = true;
                break;
            }
        }
        if !placed {
            return Err(PackingError::DomainOverflow {
                index,
                domain: frame.domain,
            });
        }
    }
    Ok(())
}
