//! Pure, bounded startup placement for the three-page gameplay atlas.
//!
//! This module allocates only owned placement metadata, never pixel storage.
//! The caller owns exactly `ATLAS_BYTES` RGBA8 bytes in page-major order and
//! initializes `WHITE_TEXEL` to opaque white. Nearest filtering, no mipmaps.
//! Frames/masks/anchors remain in semantic input order; page IDs are runtime
//! addresses, not source-manifest page IDs. No painter-order regrouping.
//! Every native placement starts at x/y >= 2. Page 2 row 1 is therefore already
//! reserved; the optional CPU terrain lookup codec does not change packing.

use std::fmt;

/// CPU-only descriptor codec for the already reserved page-2 row 1.
pub mod terrain_lookup;

pub const PAGE_SIDE: u16 = 2048;
pub const PAGE_COUNT: usize = 3;
pub const PAGE_BYTES: usize = PAGE_SIDE as usize * PAGE_SIDE as usize * 4;
pub const ATLAS_BYTES: usize = PAGE_COUNT * PAGE_BYTES; // 50,331,648 bytes.
pub const MAX_SELECTED_FRAMES: usize = 2048;
pub const MAX_FRAME_EXTENT: u16 = PAGE_SIDE - 4;
const START: u16 = 2;
const GAP: u16 = 1;

/// Terrain owns pages 0–1. All units, resources, objects and shadows own page 2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtlasDomain {
    Terrain,
    Objects,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameExtent {
    pub domain: AtlasDomain,
    pub width: u16,
    pub height: u16,
}

/// Pixel rectangle, excluding gutters. All fields use runtime pixel addresses.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Placement {
    pub page: u16,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// Explicit address for solid/grid/ring sprites. All pages reserve the origin;
/// only this page needs a white texel unless the caller explicitly uses others.
pub const WHITE_TEXEL: Placement = Placement {
    page: 2,
    x: 0,
    y: 0,
    width: 1,
    height: 1,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackingError {
    TooManyFrames {
        count: usize,
        limit: usize,
    },
    ZeroExtent {
        index: usize,
    },
    OversizedExtent {
        index: usize,
        width: u16,
        height: u16,
    },
    DomainOverflow {
        index: usize,
        domain: AtlasDomain,
    },
}

impl fmt::Display for PackingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyFrames { count, limit } => {
                write!(f, "selected frame count {count} exceeds {limit}")
            }
            Self::ZeroExtent { index } => write!(f, "frame {index} has a zero extent"),
            Self::OversizedExtent {
                index,
                width,
                height,
            } => write!(
                f,
                "frame {index} extent {width}x{height} exceeds {MAX_FRAME_EXTENT}"
            ),
            Self::DomainOverflow { index, domain } => {
                write!(f, "frame {index} overflows the {domain:?} atlas domain")
            }
        }
    }
}

impl std::error::Error for PackingError {}

/// Count is checked first, then every extent in semantic order, before any
/// output allocation or sorting. Empty input is valid and allocates no pixels.
/// Returns exactly one placement per input, at its original semantic index.
/// Packing failure is atomic: no partial placements escape the helper.
pub fn pack_frames(frames: &[FrameExtent]) -> Result<Vec<Placement>, PackingError> {
    pack_with_allocator(frames, |count| vec![Placement::default(); count])
}

// Private allocation seam lets native tests prove validation precedes metadata
// allocation, without a global allocator or unsafe instrumentation.
fn pack_with_allocator(
    frames: &[FrameExtent],
    allocate: impl FnOnce(usize) -> Vec<Placement>,
) -> Result<Vec<Placement>, PackingError> {
    validate(frames)?;
    let mut placements = allocate(frames.len());
    // Fixed 4 KiB index scratch: bounded startup-only insertion sort, avoiding
    // generic quicksort's WASM code size and a second heap allocation.
    let mut indices = [0_u16; MAX_SELECTED_FRAMES];
    for index in 0..frames.len() {
        let mut slot = index;
        while slot > 0 && precedes(frames, index, usize::from(indices[slot - 1])) {
            indices[slot] = indices[slot - 1];
            slot -= 1;
        }
        indices[slot] = index as u16;
    }
    let mut cursors = [Cursor::new(); PAGE_COUNT];
    for &index in &indices[..frames.len()] {
        let index = usize::from(index);
        let frame = frames[index];
        let pages = match frame.domain {
            AtlasDomain::Terrain => 0..2,
            AtlasDomain::Objects => 2..3,
        };
        let mut placed = false;
        for page in pages {
            if let Some((x, y)) = cursors[page].place(frame.width, frame.height) {
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
    Ok(placements)
}

fn validate(frames: &[FrameExtent]) -> Result<(), PackingError> {
    if frames.len() > MAX_SELECTED_FRAMES {
        return Err(PackingError::TooManyFrames {
            count: frames.len(),
            limit: MAX_SELECTED_FRAMES,
        });
    }
    for (index, frame) in frames.iter().enumerate() {
        if frame.width == 0 || frame.height == 0 {
            return Err(PackingError::ZeroExtent { index });
        }
        if frame.width > MAX_FRAME_EXTENT || frame.height > MAX_FRAME_EXTENT {
            return Err(PackingError::OversizedExtent {
                index,
                width: frame.width,
                height: frame.height,
            });
        }
    }
    Ok(())
}

fn precedes(frames: &[FrameExtent], a: usize, b: usize) -> bool {
    let (a_frame, b_frame) = (frames[a], frames[b]);
    a_frame.height > b_frame.height
        || (a_frame.height == b_frame.height && a_frame.width > b_frame.width)
        || (a_frame.height == b_frame.height && a_frame.width == b_frame.width && a < b)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Cursor {
    x: u16,
    y: u16,
    row_height: u16,
}

impl Cursor {
    const fn new() -> Self {
        Self {
            x: START,
            y: START,
            row_height: 0,
        }
    }

    // Called only with validated extents. Arithmetic uses u32 even for failed
    // trials: near-bottom wrapping can exceed PAGE_SIDE but cannot overflow.
    fn place(&mut self, width: u16, height: u16) -> Option<(u16, u16)> {
        let (mut x, mut y, mut row_height) = (
            u32::from(self.x),
            u32::from(self.y),
            u32::from(self.row_height),
        );
        let (width, height) = (u32::from(width), u32::from(height));
        let (gap, side) = (u32::from(GAP), u32::from(PAGE_SIDE));
        if x + width + gap >= side {
            x = u32::from(START);
            y += row_height + gap;
            row_height = 0;
        }
        if x + width + gap >= side || y + height + gap >= side {
            return None; // Do not commit a failed row wrap.
        }
        *self = Self {
            x: (x + width + gap) as u16,
            y: y as u16,
            row_height: row_height.max(height) as u16,
        };
        Some((x as u16, y as u16))
    }
}

#[cfg(test)]
mod tests;
