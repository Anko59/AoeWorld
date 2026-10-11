//! Shore band from shaped physical water. Modelling choices, not geography:
//! land within one to three tiles of open water is beach (sand) or river
//! bank (mud), and lake or ocean water within one or two tiles of land is a
//! shallow rim. Both widths vary smoothly along the shore so the band is not
//! a constant-width outline. Rivers keep their kind to the bank so narrow
//! channels stay rivers.
use super::*;
use crate::WaterKind;
use crate::terrain::shoreline;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in super::super) enum Shore {
    None,
    /// Walkable sand beside lake, ocean or shallow water.
    Beach,
    /// Walkable mud beside river water.
    Bank,
    /// Lake or ocean water near land.
    Shallow,
}

/// Widest band in tiles, and its offsets with their squared distance,
/// nearest first.
const BAND_REACH: i32 = 3;
const BAND: [(i32, i32, i32); 36] = band();
/// Squared reach of a narrow, ordinary and wide band: one tile, two tiles or
/// a knight's step, three tiles or a (3, 1) step.
const REACH: [i32; 3] = [2, 5, 10];
const WIDTH_LATTICE_TILES: i64 = 9;

const fn band() -> [(i32, i32, i32); 36] {
    let mut offsets = [(0, 0, 0); 36];
    let mut count = 0;
    let mut distance = 1;
    while distance <= 10 {
        let mut dy = -BAND_REACH;
        while dy <= BAND_REACH {
            let mut dx = -BAND_REACH;
            while dx <= BAND_REACH {
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
    assert!(count == 36);
    offsets
}

impl<const N: usize, const W: usize> Memo<'_, N, W> {
    fn water_index(&self, position: TileCoord) -> Option<usize> {
        let side = self.side + 2 * WATER_HALO;
        let x = i64::from(position.x) - i64::from(self.origin.x) + WATER_HALO;
        let y = i64::from(position.y) - i64::from(self.origin.y) + WATER_HALO;
        ((0..side).contains(&x) && (0..side).contains(&y)).then_some((y * side + x) as usize)
    }

    /// Shaped physical water kind; `None` outside the map.
    fn water(&self, position: TileCoord) -> Result<Option<WaterKind>, EnvironmentPageError> {
        if (self.cancelled)() {
            return Err(EnvironmentPageError::Cancelled);
        }
        let index = self.water_index(position);
        if let Some(index) = index
            && let Some(kind) = self.water.borrow()[index]
        {
            return Ok(Some(kind));
        }
        let known = self
            .index(position)
            .and_then(|index| self.entries.borrow()[index].base)
            .map(|base| base.water);
        let kind = match known {
            Some(kind) => Some(kind),
            None => self
                .generator
                .base_water_with_cancel(position, self.cancelled)?,
        };
        if let Some(index) = index
            && let Some(kind) = kind
        {
            self.water.borrow_mut()[index] = Some(kind);
        }
        Ok(kind)
    }

    pub(in super::super) fn shore(
        &self,
        position: TileCoord,
    ) -> Result<Shore, EnvironmentPageError> {
        let index = self.index(position);
        if let Some(index) = index
            && let Some(shore) = self.shores.borrow()[index]
        {
            return Ok(shore);
        }
        let shore = self.classify(position)?;
        if let Some(index) = index {
            self.shores.borrow_mut()[index] = Some(shore);
        }
        Ok(shore)
    }

    fn classify(&self, position: TileCoord) -> Result<Shore, EnvironmentPageError> {
        let Some(base) = self.base(position)? else {
            return Ok(Shore::None);
        };
        let neighbor =
            |dx: i32, dy: i32| match (position.x.checked_add(dx), position.y.checked_add(dy)) {
                (Some(x), Some(y)) => self.water(TileCoord::new(x, y)),
                _ => Ok(None),
            };
        let key = self.generator.shoreline_key();
        let width = |domain: u64, narrow: i64, wide: i64| {
            let value = shoreline::variation(key, domain, position, WIDTH_LATTICE_TILES);
            REACH[usize::from(value > narrow) + usize::from(value > wide)]
        };
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
                for (dx, dy, distance) in BAND {
                    if distance > reach {
                        break;
                    }
                    match neighbor(dx, dy)? {
                        None | Some(WaterKind::None) => {}
                        Some(WaterKind::River) => return Ok(Shore::Bank),
                        Some(_) => return Ok(Shore::Beach),
                    }
                }
                Ok(Shore::None)
            }
            WaterKind::Lake | WaterKind::Ocean => {
                // One tile, or two where the rim widens; never three.
                let reach = width(7, 0, i64::MAX);
                for (dx, dy, distance) in BAND {
                    if distance > reach {
                        break;
                    }
                    if neighbor(dx, dy)? == Some(WaterKind::None) {
                        return Ok(Shore::Shallow);
                    }
                }
                Ok(Shore::None)
            }
            WaterKind::River | WaterKind::Shallow => Ok(Shore::None),
        }
    }
}
