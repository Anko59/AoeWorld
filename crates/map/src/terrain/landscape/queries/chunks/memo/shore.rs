//! Shore band and water depth from shaped physical water. Modelling choices,
//! not geography: land within one to three tiles of open water is beach
//! (sand) or river bank (textured dirt); lake or ocean water within one or two tiles of
//! land is a shallow rim, medium water reaches five to seven tiles from land
//! and everything beyond is deep. All widths vary smoothly along the shore so
//! no band is a constant-width outline. Rivers keep their kind to the bank:
//! shallow beside land, medium midstream, never deep.
use super::*;
use crate::WaterKind;
use crate::terrain::shoreline;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in super::super) enum Shore {
    None,
    /// Walkable sand beside lake, ocean or shallow water.
    Beach,
    /// Walkable textured ground beside river water.
    Bank,
    /// Lake or ocean water near land.
    Shallow,
    /// River water touching its bank: shallow art, river kind.
    Margin,
    /// Lake or ocean water with no land within the medium reach.
    Deep,
}

/// Widest land band in tiles; the water scan reaches `DEPTH_REACH` tiles.
/// `BAND` holds every offset with its squared distance, nearest first.
const BAND_REACH: i32 = 3;
pub(in super::super) const DEPTH_REACH: i32 = 7;
const BAND_LIMIT: i32 = 50;
static BAND: [(i32, i32, i32); 160] = band();
/// Squared reach of a narrow, ordinary and wide band: one tile, two tiles or
/// a knight's step, three tiles or a (3, 1) step.
const REACH: [i32; 3] = [2, shoreline::RIVER_BANK_MAX_DISTANCE_SQUARED, 10];
/// Squared reach of medium water: five, six or seven tiles from land.
const MEDIUM: [i32; 3] = [25, 36, BAND_LIMIT];
const WIDTH_LATTICE_TILES: i64 = 9;

const fn band() -> [(i32, i32, i32); 160] {
    let mut offsets = [(0, 0, 0); 160];
    let mut count = 0;
    let mut distance = 1;
    while distance <= BAND_LIMIT {
        let mut dy = -DEPTH_REACH;
        while dy <= DEPTH_REACH {
            let mut dx = -DEPTH_REACH;
            while dx <= DEPTH_REACH {
                if dx * dx + dy * dy == distance {
                    offsets[count] = (dx, dy, distance);
                    count += 1;
                }
                dx += 1;
            }
            dy += 1;
        }
        distance += 1;
    }
    assert!(count == 160);
    offsets
}

impl<const N: usize, const W: usize> Memo<'_, N, W> {
    fn water_index(&self, position: TileCoord) -> Option<usize> {
        if Self::TAGGED {
            return Some(tagged_slot(position));
        }
        let side = self.side + 2 * WATER_HALO;
        let x = i64::from(position.x) - i64::from(self.origin.x) + WATER_HALO;
        let y = i64::from(position.y) - i64::from(self.origin.y) + WATER_HALO;
        ((0..side).contains(&x) && (0..side).contains(&y)).then_some((y * side + x) as usize)
    }

    /// Shaped physical water kind; `None` outside the map.
    pub(super) fn water(
        &self,
        position: TileCoord,
    ) -> Result<Option<WaterKind>, EnvironmentPageError> {
        if (self.cancelled)() {
            return Err(EnvironmentPageError::Cancelled);
        }
        let index = self.water_index(position);
        if let Some(index) = index
            && let Some((at, (kind, _))) = self.water.borrow()[index]
            && at == position
        {
            return Ok(Some(kind));
        }
        let known = self
            .index(position)
            .and_then(|index| self.entries.borrow()[index].base)
            .map(|base| (base.water, None));
        let water = match known {
            Some(water) => Some(water),
            None if self.generator.provider.is_none()
                && self.generator.landscape_in_bounds(position) =>
            {
                // `base_water_with_cancel` without a provider, keeping the
                // provenance for this tile's base sample.
                let (kind, provenance) = self.generator.dense_water(position);
                Some((kind, Some(provenance)))
            }
            None => self
                .generator
                .base_water_with_cancel(position, self.cancelled)?
                .map(|kind| (kind, None)),
        };
        if let Some(index) = index
            && let Some(water) = water
        {
            self.water.borrow_mut()[index] = Some((position, water));
        }
        Ok(water.map(|(kind, _)| kind))
    }

    /// The tile's `dense_water` (no provider, in bounds), shared with the
    /// shore scans of its neighbours.
    pub(super) fn dense_water(
        &self,
        position: TileCoord,
    ) -> (WaterKind, crate::terrain::Provenance) {
        let index = self.water_index(position);
        if let Some(index) = index
            && let Some((at, (kind, Some(provenance)))) = self.water.borrow()[index]
            && at == position
        {
            return (kind, provenance);
        }
        let water = self.generator.dense_water(position);
        if let Some(index) = index {
            self.water.borrow_mut()[index] = Some((position, (water.0, Some(water.1))));
        }
        water
    }

    pub(in super::super) fn shore(
        &self,
        position: TileCoord,
    ) -> Result<Shore, EnvironmentPageError> {
        let index = self.index(position);
        if let Some(index) = index
            && let Some(shore) = self.entries.borrow()[index].shore
        {
            return Ok(shore);
        }
        let shore = self.classify(position)?;
        if let Some(index) = index {
            self.put(index, position, |entry| entry.shore = Some(shore));
        }
        Ok(shore)
    }

    /// The water kind `(dx, dy)` from `position`; `window` is its offset
    /// in, and the side of, an untagged water table.
    #[inline(always)]
    fn near_water(
        &self,
        position: TileCoord,
        (wx, wy, side): (i64, i64, i64),
        dx: i32,
        dy: i32,
    ) -> Result<Option<WaterKind>, EnvironmentPageError> {
        if Self::TAGGED {
            if let (Some(x), Some(y)) = (position.x.checked_add(dx), position.y.checked_add(dy)) {
                let near = TileCoord::new(x, y);
                if let Some((at, (kind, _))) = self.water.borrow()[tagged_slot(near)]
                    && at == near
                {
                    return Ok(Some(kind));
                }
            }
        } else {
            // An untagged slot only ever holds the tile its offset names.
            let (x, y) = (wx + i64::from(dx), wy + i64::from(dy));
            if (0..side).contains(&x)
                && (0..side).contains(&y)
                && let Some((_, (kind, _))) = self.water.borrow()[(y * side + x) as usize]
            {
                return Ok(Some(kind));
            }
        }
        let (Some(x), Some(y)) = (position.x.checked_add(dx), position.y.checked_add(dy)) else {
            return Ok(None);
        };
        let near = TileCoord::new(x, y);
        self.water(near)
    }

    fn classify(&self, position: TileCoord) -> Result<Shore, EnvironmentPageError> {
        let Some(base) = self.base(position)? else {
            return Ok(Shore::None);
        };
        if base.material == crate::GroundMaterial::Ford {
            return Ok(Shore::None);
        }
        // Scans read hundreds of known kinds; take them straight from the
        // table and fall back to the full query only for a missing one.
        let side = self.side + 2 * WATER_HALO;
        let wx = i64::from(position.x) - i64::from(self.origin.x) + WATER_HALO;
        let wy = i64::from(position.y) - i64::from(self.origin.y) + WATER_HALO;
        let window = (wx, wy, side);
        let key = self.generator.shoreline_key();
        let step = |domain: u64, narrow: i64, wide: i64| {
            let value = shoreline::variation(key, domain, position, WIDTH_LATTICE_TILES);
            usize::from(value > narrow) + usize::from(value > wide)
        };
        let width = |domain: u64, narrow: i64, wide: i64| REACH[step(domain, narrow, wide)];
        match base.water {
            WaterKind::None => {
                if self.probe_reach
                    && !self
                        .generator
                        .water_possible_near(position, BAND_REACH, self.cancelled)?
                {
                    return Ok(Shore::None);
                }
                let reach = width(6, -350, 350);
                // The bank width matters only once river water is in reach.
                let mut bank = None;
                for (dx, dy, distance) in BAND.iter().copied() {
                    if distance > reach {
                        break;
                    }
                    match self.near_water(position, window, dx, dy)? {
                        None | Some(WaterKind::None) => {}
                        Some(WaterKind::River)
                            if distance <= *bank.get_or_insert_with(|| width(23, 0, i64::MAX)) =>
                        {
                            return Ok(Shore::Bank);
                        }
                        Some(WaterKind::River) => {}
                        Some(_) => return Ok(Shore::Beach),
                    }
                }
                Ok(Shore::None)
            }
            WaterKind::Lake | WaterKind::Ocean => {
                if self.probe_reach
                    && self.generator.open_water_certain_near(
                        position,
                        DEPTH_REACH,
                        self.cancelled,
                    )?
                {
                    return Ok(Shore::Deep);
                }
                // One tile, or two where the rim widens; never three.
                // Both widths vary per tile; read each only when it decides.
                let mut medium = None;
                for (dx, dy, distance) in BAND.iter().copied() {
                    if distance > MEDIUM[0]
                        && distance > *medium.get_or_insert_with(|| MEDIUM[step(8, -350, 350)])
                    {
                        break;
                    }
                    if self.near_water(position, window, dx, dy)? == Some(WaterKind::None) {
                        return Ok(if distance <= width(7, 0, i64::MAX) {
                            Shore::Shallow
                        } else {
                            Shore::None
                        });
                    }
                }
                Ok(Shore::Deep)
            }
            WaterKind::River => {
                for (dx, dy, distance) in BAND.iter().copied() {
                    if distance > REACH[0] {
                        break;
                    }
                    if self.near_water(position, window, dx, dy)? == Some(WaterKind::None) {
                        return Ok(Shore::Margin);
                    }
                }
                Ok(Shore::None)
            }
            WaterKind::Shallow => Ok(Shore::None),
        }
    }
}
