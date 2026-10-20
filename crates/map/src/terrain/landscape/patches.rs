//! Bounded forest descriptor of the composed landscape. Forest is a union of
//! analytic stands whose number and size follow a per-cell "wildness" (the
//! caller's real-world forest signal, 0..=1000) on a 192-tile lattice:
//!
//! - one optional FOREST per 192-tile cell: three overlapping lobes, main
//!   radius 26..=70 tiles, present only from wildness 350 upward;
//! - small GROVES on a 48-tile lattice: two lobes, radius 5..=18 tiles, whose
//!   presence and size follow the bilinearly interpolated wildness and a
//!   continuous clumping field.
//!
//! Glades and paths belong to LARGE forests only: those whose main radius is
//! at least 48 tiles. Each holds its cell's trail node (a hub glade), one to
//! three interior glades linked to that hub, and the trails joining its node
//! to the four neighboring nodes. They are cleared only inside the
//! large-forest domain (lobes plus an 8-tile apron), so every large forest is
//! crossed by open corridors while smaller forests and groves stay solid.
//!
//! Stands, glades and trails share one continuously warped world coordinate
//! space (64- and 20-tile lattices, at most 15 tiles per axis); nothing is
//! clipped by a cell, chunk or source-sample boundary. Pure integer code:
//! no world-size storage, allocation, float, unsafe or query-order state. The
//! stable mixer is not cryptographic; changing it changes generation.

/// Side of one wildness/trail-node cell.
pub const CELL_TILES: i64 = 192;
/// Smallest main-lobe radius of a LARGE forest: with its satellite lobes a
/// stand of roughly 9,000 tiles, about 100 tiles across. Smaller forests and
/// groves never receive a glade or a path.
pub const LARGE_FOREST_MIN_RADIUS: i64 = 48;
/// Wildness below which a cell never holds a forest, only groves.
pub const FOREST_MIN_WILDNESS: u16 = 350;
const FOREST_MIN_RADIUS: i64 = 30;
const GROVE_TILES: i64 = 48;
const APRON: i64 = 8;
const LARGE_REACH: i64 = 148;
const GROVE_REACH: i64 = 24;
const EDGE_WIDTH: i64 = 2;
const CORE_DENSITY: u16 = 900;
const EDGE_DENSITY: u16 = 650;
const SAVANNA_DENSITY: u16 = 80;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Forest,
    SparseSavanna,
    Treeless,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Zone {
    Core,
    Edge,
    Exterior,
}
/// Which kind of stand covers a tile, before any glade or path is cleared.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stand {
    None,
    Grove,
    Forest,
}
/// Open ground kept inside the large-forest domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Opening {
    None,
    Glade,
    Trail,
}
/// Zero support (for example above the treeline) clears the tile; other
/// values scale stand radii.
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
/// Caller owns ONE mask for water/slope/cliffs, the starting glade and
/// resource approaches: `eligible == false` clears trees AND canopy/floor.
/// `corridor` marks a caller-supplied route that is cleared only where it
/// crosses the large-forest domain, exactly like the descriptor's own trails.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Input {
    pub mode: Mode,
    pub fitness: Fitness,
    pub eligible: bool,
    pub corridor: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DensitySample {
    pub zone: Zone,
    pub stand: Stand,
    pub opening: Opening,
    pub density_per_thousand: u16,
    pub canopy_per_thousand: u16,
    pub forest_floor_per_thousand: u16,
    pub tree: bool,
}
impl DensitySample {
    pub const OPEN: Self = Self {
        zone: Zone::Exterior,
        stand: Stand::None,
        opening: Opening::None,
        density_per_thousand: 0,
        canopy_per_thousand: 0,
        forest_floor_per_thousand: 0,
        tree: false,
    };
}
/// One cell forest: main lobe, two satellite lobes and, when large, glades.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LargeForest {
    pub center: (i64, i64),
    pub radius: i64,
    /// Whether the forest is large enough for glades and paths.
    pub large: bool,
    lobes: [(i64, i64, i64); 3],
    glades: [(i64, i64, i64); 3],
}
#[derive(Clone, Copy, Debug)]
pub struct Patches {
    key: u64,
}
impl Patches {
    pub fn new(world_key: [u8; 32], seed: u64) -> Self {
        let mut key = mix(seed ^ 0x7061_7463_682d_7639);
        for part in world_key.as_chunks::<8>().0 {
            key = mix(key
                ^ u64::from_le_bytes([
                    part[0], part[1], part[2], part[3], part[4], part[5], part[6], part[7],
                ]));
        }
        Self { key }
    }
    /// The cell's forest for a wildness, if that cell holds one.
    pub fn large_forest(&self, cx: i64, cy: i64, wildness: u16) -> Option<LargeForest> {
        let wildness = i64::from(wildness.min(1000));
        let above = wildness - i64::from(FOREST_MIN_WILDNESS);
        let h = self.noise(2, cx, cy);
        if above < 0 || ((h >> 48) % 1000) as i64 >= above * 1000 / 400 {
            return None;
        }
        let radius = FOREST_MIN_RADIUS + above * 36 / 650 + ((h >> 32) % 9) as i64 - 4;
        let large = radius >= LARGE_FOREST_MIN_RADIUS;
        let node = self.node(cx, cy);
        let center = (
            node.0 + (h % 41) as i64 - 20,
            node.1 + ((h >> 8) % 41) as i64 - 20,
        );
        let first = ((h >> 16) % 8) as usize;
        let second = (first + 2 + ((h >> 19) % 5) as usize) % 8;
        let lobe = |direction: usize, percent: u64| {
            let (dx, dy) = DIRECTIONS[direction];
            (
                center.0 + dx * radius * 75 / 100_000,
                center.1 + dy * radius * 75 / 100_000,
                radius * (55 + percent as i64) / 100,
            )
        };
        let glade = |index: u64| {
            if !large || radius < LARGE_FOREST_MIN_RADIUS + 8 * index as i64 {
                return (0, 0, 0);
            }
            let g = mix(h ^ (index + 1).wrapping_mul(0x9e37_79b9));
            let (dx, dy) =
                DIRECTIONS[((first as u64 + 1 + index * 3 + (g >> 12) % 2) % 8) as usize];
            let distance = radius * (25 + (g % 26) as i64) / 100;
            (
                center.0 + dx * distance / 1000,
                center.1 + dy * distance / 1000,
                5 + ((g >> 8) % 4) as i64,
            )
        };
        Some(LargeForest {
            center,
            radius,
            large,
            lobes: [
                (center.0, center.1, radius),
                lobe(first, (h >> 24) % 26),
                lobe(second, (h >> 40) % 26),
            ],
            glades: [glade(0), glade(1), glade(2)],
        })
    }
    /// Forest exterior occupancy is exactly zero. Savanna intentionally exposes
    /// sparse exterior trees with no forest canopy/floor: a separate ecology mode.
    pub fn try_density_sample<E>(
        &self,
        x: i32,
        y: i32,
        input: Input,
        wildness_at: &impl Fn(i64, i64) -> Result<u16, E>,
    ) -> Result<DensitySample, E> {
        let support = i64::from(input.fitness.support_per_thousand.min(1000));
        if !input.eligible || !input.fitness.suitable || support == 0 {
            return Ok(DensitySample::OPEN);
        }
        let (x, y) = (i64::from(x), i64::from(y));
        Ok(match input.mode {
            Mode::Treeless => DensitySample::OPEN,
            Mode::SparseSavanna => {
                // Scattered trees thin out continuously with the wildness.
                let wildness = interpolated_wildness((x, y), wildness_at)?;
                let density = (i64::from(SAVANNA_DENSITY) * support * wildness / 1_000_000) as u16;
                DensitySample {
                    density_per_thousand: density,
                    tree: self.noise(3, x, y) % 1000 < u64::from(density),
                    ..DensitySample::OPEN
                }
            }
            Mode::Forest => {
                let (zone, stand, opening) =
                    self.classify(x, y, input.corridor, support, wildness_at)?;
                let (density, canopy) = match zone {
                    Zone::Core => (CORE_DENSITY, 1000),
                    Zone::Edge => (EDGE_DENSITY, 650),
                    Zone::Exterior => (0, 0),
                };
                DensitySample {
                    zone,
                    stand,
                    opening,
                    density_per_thousand: density,
                    canopy_per_thousand: canopy,
                    forest_floor_per_thousand: canopy,
                    tree: density > 0 && self.noise(4, x, y) % 1000 < u64::from(density),
                }
            }
        })
    }
    /// Infallible form of [`Self::try_sample`].
    pub fn sample(
        &self,
        x: i32,
        y: i32,
        input_at: impl Fn(i32, i32) -> Input,
        wildness_at: impl Fn(i64, i64) -> u16,
    ) -> DensitySample {
        let never = self.try_sample(
            x,
            y,
            |nx, ny| Ok::<_, std::convert::Infallible>(input_at(nx, ny)),
            |cx, cy| Ok(wildness_at(cx, cy)),
        );
        match never {
            Ok(sample) => sample,
            Err(never) => match never {},
        }
    }
    /// Removes isolated raw candidates using the same inputs at eight
    /// neighbors; connected pairs survive, so filtering cannot create new
    /// singletons. At most nine descriptor evaluations; cancellation and
    /// source errors propagate. Callbacks must read BASE ecology/masks and
    /// per-cell wildness only, never decorated terrain or placed objects.
    pub fn try_sample<E>(
        &self,
        x: i32,
        y: i32,
        input_at: impl Fn(i32, i32) -> Result<Input, E>,
        wildness_at: impl Fn(i64, i64) -> Result<u16, E>,
    ) -> Result<DensitySample, E> {
        let input = input_at(x, y)?;
        let mut sample = self.try_density_sample(x, y, input, &wildness_at)?;
        if !sample.tree || input.mode != Mode::Forest {
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
                if self
                    .try_density_sample(nx, ny, input_at(nx, ny)?, &wildness_at)?
                    .tree
                {
                    return Ok(sample);
                }
            }
        }
        sample.tree = false;
        Ok(sample)
    }
    fn classify<E>(
        &self,
        x: i64,
        y: i64,
        corridor: bool,
        support: i64,
        wildness_at: &impl Fn(i64, i64) -> Result<u16, E>,
    ) -> Result<(Zone, Stand, Opening), E> {
        let (wx, wy) = self.warped(x, y);
        let (cx, cy) = (wx.div_euclid(CELL_TILES), wy.div_euclid(CELL_TILES));
        let (mut zone, mut domain, mut glade) = (Zone::Exterior, false, false);
        for my in cy - 1..=cy + 1 {
            for mx in cx - 1..=cx + 1 {
                let node = self.node(mx, my);
                if (wx - node.0).abs() > LARGE_REACH || (wy - node.1).abs() > LARGE_REACH {
                    continue;
                }
                let Some(forest) = self.large_forest(mx, my, wildness_at(mx, my)?) else {
                    continue;
                };
                for (px, py, radius) in forest.lobes {
                    let radius = (radius * support / 1000).max(1);
                    let distance = (wx - px).pow(2) + (wy - py).pow(2);
                    if distance <= (radius - EDGE_WIDTH).max(0).pow(2) {
                        zone = Zone::Core;
                    } else if distance <= radius.pow(2) && zone == Zone::Exterior {
                        zone = Zone::Edge;
                    }
                    domain |= forest.large && distance <= (radius + APRON).pow(2);
                }
                // A glade, or the narrow path linking it to the hub at the node.
                glade |= forest.glades.iter().any(|(px, py, radius)| {
                    *radius > 0
                        && ((wx - px).pow(2) + (wy - py).pow(2) <= radius * radius
                            || near_segment(node, (*px, *py), (wx, wy), 1))
                });
            }
        }
        let stand = if zone == Zone::Exterior {
            Stand::None
        } else {
            Stand::Forest
        };
        if domain {
            if glade {
                return Ok((Zone::Exterior, stand, Opening::Glade));
            }
            if corridor || self.trail(wx, wy) {
                return Ok((Zone::Exterior, stand, Opening::Trail));
            }
        }
        if zone != Zone::Exterior {
            return Ok((zone, stand, Opening::None));
        }
        let (gx, gy) = (wx.div_euclid(GROVE_TILES), wy.div_euclid(GROVE_TILES));
        let mut edge = false;
        for my in gy - 1..=gy + 1 {
            for mx in gx - 1..=gx + 1 {
                let h = self.noise(9, mx, my);
                let anchor = (
                    mx * GROVE_TILES + GROVE_TILES / 2,
                    my * GROVE_TILES + GROVE_TILES / 2,
                );
                let px = anchor.0 + (h % 37) as i64 - 18;
                let py = anchor.1 + ((h >> 8) % 37) as i64 - 18;
                if (wx - px).abs() > GROVE_REACH || (wy - py).abs() > GROVE_REACH {
                    continue;
                }
                let wildness = interpolated_wildness(anchor, wildness_at)?;
                // Groves gather in clumps: a continuous 0.5..1.5 presence factor.
                let clump = 1000 + self.warp(15, 144, 500, anchor.0, anchor.1);
                if ((h >> 48) % 1000) as i64 >= grove_presence(wildness) * clump / 1000 {
                    continue;
                }
                let radius = ((5 + wildness.min(600) / 100 + ((h >> 32) % 8) as i64) * support
                    / 1000)
                    .max(1);
                let (vx, vy) = DIRECTIONS[((h >> 20) % 8) as usize];
                for sign in [-1, 1] {
                    let dx = wx - px - sign * (radius / 3) * vx / 1000;
                    let dy = wy - py - sign * (radius / 3) * vy / 1000;
                    let lobe = if sign < 0 {
                        radius
                    } else {
                        radius * (70 + ((h >> 40) % 31) as i64) / 100
                    };
                    let distance = dx * dx + dy * dy;
                    if distance <= (lobe - EDGE_WIDTH).max(0).pow(2) {
                        return Ok((Zone::Core, Stand::Grove, Opening::None));
                    }
                    edge |= distance <= lobe * lobe;
                }
            }
        }
        Ok(if edge {
            (Zone::Edge, Stand::Grove, Opening::None)
        } else {
            (Zone::Exterior, Stand::None, Opening::None)
        })
    }
    fn noise(&self, domain: u64, x: i64, y: i64) -> u64 {
        mix(self.key ^ mix(domain) ^ mix(x as u64) ^ mix(y as u64 ^ 0xd1b5_4a32_d192_ed03))
    }
}
/// Grove presence per thousand lattice cells: none without wildness, rising
/// with it, then giving way to forests so the most forested regions keep
/// open ground.
fn grove_presence(wildness: i64) -> i64 {
    match wildness {
        ..300 => wildness.max(0),
        300..500 => 300 + (wildness - 300) * 120 / 200,
        _ => 420 - (wildness.min(1000) - 500) * 120 / 500,
    }
}
// Bilinear between the four cell centres around a point: continuous across
// cells, so grove density never steps on the cell lattice.
fn interpolated_wildness<E>(
    point: (i64, i64),
    wildness_at: &impl Fn(i64, i64) -> Result<u16, E>,
) -> Result<i64, E> {
    let (sx, sy) = (point.0 - CELL_TILES / 2, point.1 - CELL_TILES / 2);
    let (cx, cy) = (sx.div_euclid(CELL_TILES), sy.div_euclid(CELL_TILES));
    let (ox, oy) = (sx.rem_euclid(CELL_TILES), sy.rem_euclid(CELL_TILES));
    let at = |dx, dy| wildness_at(cx + dx, cy + dy).map(|value| i64::from(value.min(1000)));
    let north = at(0, 0)? * (CELL_TILES - ox) + at(1, 0)? * ox;
    let south = at(0, 1)? * (CELL_TILES - ox) + at(1, 1)? * ox;
    Ok((north * (CELL_TILES - oy) + south * oy) / (CELL_TILES * CELL_TILES))
}
fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
#[path = "patches/geometry.rs"]
mod geometry;
use geometry::near_segment;
#[path = "patches/tests.rs"]
#[cfg(test)]
mod tests;
