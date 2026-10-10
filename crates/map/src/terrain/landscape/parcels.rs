//! Pure historical parcel realization for the composed landscape.
//!
//! Fractions are of SOURCE VALID LAND, never forest or ecology-suitable land.
//! The caller supplies observation/valid-land status and retains all provenance,
//! nodata, lake/ocean/outside coverage and any partial-coverage area weights.
//! Modern cover, forest subtype, fitness and placed trees are not inputs here.
//!
//! A 16-tile lattice follows a permuted space-filling subdivision in each
//! 64-tile neighborhood. Integer coarse shears bend parcel boundaries without
//! pixel rolls. Each query uses two two-node interpolations and fixed-depth
//! Hilbert lookups, no allocation, float, unsafe, world arrays or area scans.
//! Crops and grazing occupy adjacent rank intervals, not independent lotteries.
//!
//! Uniform-source 512-square fixtures approximate requested fractions. This is
//! procedural placement, NOT inferred historical farm geometry. Arbitrary masks
//! correlated with rank, small windows, heterogeneous source percentages and
//! weighted partial coverage cannot promise those ratios. Report crop/grazing
//! area over original source-valid land AND over post-exclusion eligible land,
//! with excluded area separately; do not silently renormalize to forest area.

/// Validated source percentages. Individual values must be in 0..=100.
/// Combined modeled clearing is capped at 100, retaining crop first and using
/// only the remaining capacity for grazing. This is a realization policy, not
/// a repair of source evidence: callers must preserve the uncapped observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFractions {
    crop: u8,
    grazing: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FractionError {
    CropOutOfRange,
    GrazingOutOfRange,
}

impl SourceFractions {
    pub fn new(crop_percent: u8, grazing_percent: u8) -> Result<Self, FractionError> {
        if crop_percent > 100 {
            return Err(FractionError::CropOutOfRange);
        }
        if grazing_percent > 100 {
            return Err(FractionError::GrazingOutOfRange);
        }
        Ok(Self {
            crop: crop_percent,
            grazing: grazing_percent.min(100 - crop_percent),
        })
    }

    pub const fn crop_percent(self) -> u8 {
        self.crop
    }

    /// Modeled fraction after the documented combined-100 cap.
    pub const fn grazing_percent(self) -> u8 {
        self.grazing
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LandUse {
    /// Missing historical observation; never interpret this as numeric zero.
    Unobserved,
    /// Caller reports this location outside the source-valid-land domain.
    Nonland,
    Crop,
    Grazing,
    Uncleared,
}

/// Immutable world-key/seed descriptor; query order and chunk boundaries do not
/// affect results. Coordinates are signed world tile coordinates, not local pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Parcels {
    key: u64,
}

impl Parcels {
    pub fn new(world_key: [u8; 32], seed: u64) -> Self {
        let mut key = mix(seed ^ 0x7061_7263_656c_7639);
        for bytes in world_key.chunks_exact(8) {
            key = mix(key
                ^ u64::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ]));
        }
        Self { key }
    }

    /// `None` means unobserved even if valid land is also unknown/false. Observed
    /// nonland returns Nonland, including observed zero. Apply additional water,
    /// slope, resource and ecological exclusions AFTER this domainwide descriptor.
    pub fn sample(
        &self,
        x: i32,
        y: i32,
        source: Option<SourceFractions>,
        source_valid_land: bool,
    ) -> LandUse {
        let Some(source) = source else {
            return LandUse::Unobserved;
        };
        if !source_valid_land {
            return LandUse::Nonland;
        }
        let rank = self.rank(x, y);
        // Multiply rank rather than rounding percentages to parcel counts.
        // There are 1024 unique ranks in an unwarped aligned 512-square region.
        if rank * 100 < u32::from(source.crop) * 1024 {
            LandUse::Crop
        } else if rank * 100 < u32::from(source.crop + source.grazing) * 1024 {
            LandUse::Grazing
        } else {
            LandUse::Uncleared
        }
    }

    fn rank(&self, x: i32, y: i32) -> u32 {
        // Composition of two shears is a bijection on the integer tile plane:
        // each shear translates one entire row/column. Unlike a 2D displacement
        // warp it does not intrinsically change parcel areas. Finite window edges
        // still move, so clipped 512 windows have approximate, not exact, counts.
        let wx = i64::from(x) + self.shear(1, i64::from(y));
        let wy = i64::from(y) + self.shear(2, wx);
        let (cx, cy) = (wx.div_euclid(16), wy.div_euclid(16));
        let (bx, by) = (cx.div_euclid(4), cy.div_euclid(4));
        let h = self.noise(3, bx, by);
        let (mut px, mut py) = (cx.rem_euclid(4) as u32, cy.rem_euclid(4) as u32);
        if h & 1 != 0 {
            px = 3 - px;
        }
        if h & 2 != 0 {
            py = 3 - py;
        }
        if h & 4 != 0 {
            std::mem::swap(&mut px, &mut py);
        }
        // A cyclic Hilbert interval gives neighbors contiguous areas at parcel
        // scale; wrapped intervals can have two components. Neighborhoods get
        // independent orientations/offsets, never 8x8 parity or a repeated motif.
        let local = (hilbert(px, py, 2) + ((h >> 8) as u32 & 15)) & 15;
        let region = self.noise(4, bx.div_euclid(8), by.div_euclid(8));
        let block = hilbert(bx.rem_euclid(8) as u32, by.rem_euclid(8) as u32, 3);
        // Odd affine permutation yields each low rank once per 512 region,
        // distributing partial-stratum percentages without a per-query scan.
        let low = (block * ((region as u32 & 63) | 1) + ((region >> 8) as u32 & 63)) & 63;
        local * 64 + low
    }

    // Piecewise-linear 64-tile coarse warp, amplitude <=6 tiles. Its maximum
    // slope is 12/64; base parcel sides remain near 16 tiles (not pixel noise).
    fn shear(&self, domain: u64, coordinate: i64) -> i64 {
        let cell = coordinate.div_euclid(64);
        let offset = coordinate.rem_euclid(64);
        let node = |index| (self.noise(domain, index, 0) % 13) as i64 - 6;
        (node(cell) * (64 - offset) + node(cell + 1) * offset).div_euclid(64)
    }

    fn noise(&self, domain: u64, x: i64, y: i64) -> u64 {
        mix(self.key ^ mix(domain) ^ mix(x as u64) ^ mix(y as u64 ^ 0xd1b5_4a32_d192_ed03))
    }
}

// Fixed two/three-level space-filling lookup. Signed coordinates have already
// been reduced with Euclidean division; reflection arithmetic stays in range.
fn hilbert(mut x: u32, mut y: u32, levels: u32) -> u32 {
    let mut rank = 0;
    for level in (0..levels).rev() {
        let side = 1 << level;
        let rx = u32::from(x & side != 0);
        let ry = u32::from(y & side != 0);
        rank += side * side * ((3 * rx) ^ ry);
        // Reduce to this quadrant before reflecting its lower coordinates.
        x &= side - 1;
        y &= side - 1;
        if ry == 0 {
            if rx == 1 {
                x = side - 1 - x;
                y = side - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
    }
    rank
}

fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[path = "parcels/tests.rs"]
#[cfg(test)]
mod tests;
