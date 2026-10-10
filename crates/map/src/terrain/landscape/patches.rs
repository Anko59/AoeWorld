//! Bounded ecological patch descriptor used by the composed landscape.
//! Pure bounded integer analytic clusters: two overlapping lobes per macrocell,
//! 25-cell lookup, sparse macrocell activation modulated at 192 tiles, irregular
//! centres/radii/asymmetric lobes/eight orientations and smooth 96-tile warp. No world-size storage,
//! allocation, float, unsafe, geometry/height/source changes or query-order state.
//! The stable mixer is not cryptographic; changing it changes generation and
//! its golden digests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Region {
    Sparse,
    Moderate,
    Heavy,
    Exceptional,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Forest(Region),
    SparseSavanna,
    Treeless,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Zone {
    Core,
    Edge,
    Exterior,
}
/// Ecological support scales radius, not dense-core occupancy. Full-support
/// fixture targets do not promise coverage under reduced/heterogeneous fitness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fitness {
    pub suitable: bool,
    pub support_per_thousand: u16,
}
impl Fitness {
    pub const FULL: Self = Self {
        suitable: true,
        support_per_thousand: 1000,
    };
}
/// Caller owns ONE mask for historical parcels, water/slope/cliffs, open routes
/// and resource approaches. False clears trees AND canopy/floor. Realize source
/// crop+grazing fractions over SOURCE VALID LAND upstream, not over existing
/// trees or the smaller ecology-suitable subset. Keep that denominator distinct.
/// Nodata is not zero clearing; modern cover is not historical PNV evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Input {
    pub mode: Mode,
    pub fitness: Fitness,
    pub eligible: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DensitySample {
    pub zone: Zone,
    pub density_per_thousand: u16,
    pub canopy_per_thousand: u16,
    pub forest_floor_per_thousand: u16,
    pub tree: bool,
}
impl DensitySample {
    const OPEN: Self = Self {
        zone: Zone::Exterior,
        density_per_thousand: 0,
        canopy_per_thousand: 0,
        forest_floor_per_thousand: 0,
        tree: false,
    };
}
/// Validated bounds prove 25-cell lookup complete: lobe <=44, offset <=13,
/// centre jitter <=14, warp <=6. No cell ownership clips analytic boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Parameters {
    pub radii: [u8; 4],
    pub radius_jitter: [u8; 4],
    pub activation: [u16; 4],
    pub edge_width: u8,
    pub core_density: u16,
    pub edge_density: u16,
    pub savanna_density: u16,
}
impl Default for Parameters {
    fn default() -> Self {
        Self {
            radii: [13, 19, 23, 28],
            radius_jitter: [2, 2, 2, 1],
            activation: [950, 950, 980, 1000],
            edge_width: 2,
            core_density: 900,
            edge_density: 650,
            savanna_density: 80,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParameterError {
    Radius,
    Activation,
    Edge,
    Density,
}
#[derive(Clone, Copy, Debug)]
pub struct Patches {
    key: u64,
    parameters: Parameters,
}
impl Patches {
    pub fn new(
        world_key: [u8; 32],
        seed: u64,
        parameters: Parameters,
    ) -> Result<Self, ParameterError> {
        if parameters.radii.iter().any(|r| !(8..=36).contains(r))
            || parameters.radii.windows(2).any(|r| r[0] >= r[1])
            || parameters.radius_jitter.iter().any(|j| *j > 2)
        {
            return Err(ParameterError::Radius);
        }
        if parameters
            .activation
            .iter()
            .any(|v| !(600..=1000).contains(v))
            || parameters.activation.windows(2).any(|v| v[0] > v[1])
        {
            return Err(ParameterError::Activation);
        }
        if !(1..=4).contains(&parameters.edge_width) {
            return Err(ParameterError::Edge);
        }
        if !(850..=950).contains(&parameters.core_density)
            || !(400..=800).contains(&parameters.edge_density)
            || parameters.savanna_density > 150
        {
            return Err(ParameterError::Density);
        }
        let mut key = mix(seed ^ 0x7061_7463_682d_7639);
        for part in world_key.chunks_exact(8) {
            key = mix(key
                ^ u64::from_le_bytes([
                    part[0], part[1], part[2], part[3], part[4], part[5], part[6], part[7],
                ]));
        }
        Ok(Self { key, parameters })
    }
    /// Forest exterior occupancy is exactly zero. Savanna intentionally exposes
    /// sparse exterior trees with no forest canopy/floor: a separate ecology mode.
    pub fn density_sample(&self, x: i32, y: i32, input: Input) -> DensitySample {
        if !input.eligible || !input.fitness.suitable || input.fitness.support_per_thousand == 0 {
            return DensitySample::OPEN;
        }
        match input.mode {
            Mode::Treeless => DensitySample::OPEN,
            Mode::SparseSavanna => {
                let density = (u32::from(self.parameters.savanna_density)
                    * u32::from(input.fitness.support_per_thousand.min(1000))
                    / 1000) as u16;
                DensitySample {
                    density_per_thousand: density,
                    tree: self.noise(3, i64::from(x), i64::from(y)) % 1000 < u64::from(density),
                    ..DensitySample::OPEN
                }
            }
            Mode::Forest(region) => {
                let zone = self.zone(i64::from(x), i64::from(y), region, input.fitness);
                let (density, canopy) = match zone {
                    Zone::Core => (self.parameters.core_density, 1000),
                    Zone::Edge => (self.parameters.edge_density, 650),
                    Zone::Exterior => (0, 0),
                };
                DensitySample {
                    zone,
                    density_per_thousand: density,
                    canopy_per_thousand: canopy,
                    forest_floor_per_thousand: canopy,
                    tree: density > 0
                        && self.noise(2, i64::from(x), i64::from(y)) % 1000 < u64::from(density),
                }
            }
        }
    }
    /// Removes isolated raw candidates using the same inputs at eight neighbors.
    /// Connected pairs survive, so filtering cannot create new singletons.
    /// No further tree-only exclusion may be applied after this descriptor.
    pub fn sample(&self, x: i32, y: i32, input_at: impl Fn(i32, i32) -> Input) -> DensitySample {
        match self.try_sample(x, y, |nx, ny| {
            Ok::<_, std::convert::Infallible>(input_at(nx, ny))
        }) {
            Ok(sample) => sample,
            Err(never) => match never {},
        }
    }
    /// At most nine descriptor evaluations; cancellation/source errors propagate.
    /// Neighbor callbacks must read BASE ecology/masks, never decorated tile_at,
    /// placed-tree/resource access, or another recursive descriptor evaluation.
    pub fn try_sample<E>(
        &self,
        x: i32,
        y: i32,
        input_at: impl Fn(i32, i32) -> Result<Input, E>,
    ) -> Result<DensitySample, E> {
        let input = input_at(x, y)?;
        let mut sample = self.density_sample(x, y, input);
        if !sample.tree || !matches!(input.mode, Mode::Forest(_)) {
            return Ok(sample);
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (Some(nx), Some(ny)) = (x.checked_add(dx), y.checked_add(dy)) else {
                    continue;
                };
                if self.density_sample(nx, ny, input_at(nx, ny)?).tree {
                    return Ok(sample);
                }
            }
        }
        sample.tree = false;
        Ok(sample)
    }
    fn zone(&self, x: i64, y: i64, region: Region, fitness: Fitness) -> Zone {
        let index = match region {
            Region::Sparse => 0,
            Region::Moderate => 1,
            Region::Heavy => 2,
            Region::Exceptional => 3,
        };
        const DIRECTIONS: [(i64, i64); 8] = [
            (1000, 0),
            (707, 707),
            (0, 1000),
            (-707, 707),
            (-1000, 0),
            (-707, -707),
            (0, -1000),
            (707, -707),
        ];
        let wx = x + self.warp(4, x, y);
        let wy = y + self.warp(5, x, y);
        let (cx, cy) = (wx.div_euclid(64), wy.div_euclid(64));
        let mut edge = false;
        for my in cy - 2..=cy + 2 {
            for mx in cx - 2..=cx + 2 {
                let h = self.noise(1, mx, my);
                // Coarse cluster propensity changes activation at 192 tiles;
                // only analytic lobes determine membership, not this cell edge.
                let bias = (self.noise(6, mx.div_euclid(3), my.div_euclid(3)) % 201) as i64 - 100;
                let activation = (i64::from(self.parameters.activation[index]) + bias).min(1000);
                if ((h >> 48) % 1000) as i64 >= activation {
                    continue;
                }
                let px = mx * 64 + 32 + (h % 29) as i64 - 14;
                let py = my * 64 + 32 + ((h >> 8) % 29) as i64 - 14;
                let jitter = i64::from(self.parameters.radius_jitter[index]);
                let radius = ((i64::from(self.parameters.radii[index])
                    + ((h >> 32) % (2 * jitter + 1) as u64) as i64
                    - jitter)
                    * i64::from(fitness.support_per_thousand.min(1000))
                    / 1000)
                    .max(1);
                let (vx, vy) = DIRECTIONS[((h >> 20) % 8) as usize];
                for sign in [-1, 1] {
                    let dx = wx - px - sign * (radius / 3) * vx / 1000;
                    let dy = wy - py - sign * (radius / 3) * vy / 1000;
                    let lobe = if sign < 0 {
                        radius
                    } else {
                        radius * (85 + ((h >> 40) % 31) as i64) / 100
                    };
                    let inner = (lobe - i64::from(self.parameters.edge_width)).max(0);
                    let distance = dx * dx + dy * dy;
                    if distance <= inner * inner {
                        return Zone::Core;
                    }
                    edge |= distance <= lobe * lobe;
                }
            }
        }
        if edge { Zone::Edge } else { Zone::Exterior }
    }
    // Bilinear integer warp: 96-tile lattice, amplitude <=6 tiles. Euclidean
    // division makes signed world-coordinate seams and chunk halos identical.
    fn warp(&self, domain: u64, x: i64, y: i64) -> i64 {
        let (cx, cy) = (x.div_euclid(96), y.div_euclid(96));
        let (ox, oy) = (x.rem_euclid(96), y.rem_euclid(96));
        let node = |dx, dy| (self.noise(domain, cx + dx, cy + dy) % 193) as i64 - 96;
        let north = node(0, 0) * (96 - ox) + node(1, 0) * ox;
        let south = node(0, 1) * (96 - ox) + node(1, 1) * ox;
        (north * (96 - oy) + south * oy).div_euclid(147456)
    }
    fn noise(&self, domain: u64, x: i64, y: i64) -> u64 {
        mix(self.key ^ mix(domain) ^ mix(x as u64) ^ mix(y as u64 ^ 0xd1b5_4a32_d192_ed03))
    }
}
fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
#[path = "patches/tests.rs"]
#[cfg(test)]
mod tests;
