//! Shore band from shaped physical water. Modelling choices, not geography:
//! land within `BAND` of open water is beach (sand) or river bank (mud), and
//! lake or ocean water touching land is a one-tile shallow rim. Rivers keep
//! their kind to the bank so narrow channels stay rivers.
use super::*;
use crate::WaterKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in super::super) enum Shore {
    None,
    /// Walkable sand beside lake, ocean or shallow water.
    Beach,
    /// Walkable mud beside river water.
    Bank,
    /// Lake or ocean water touching land.
    Shallow,
}

/// Band reach in tiles, and its offsets nearest first: squared distance at
/// most five (two tiles straight or a knight's step).
const BAND_REACH: i32 = 2;
const BAND: [(i32, i32); 20] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (-1, 1),
    (1, -1),
    (-1, -1),
    (2, 0),
    (-2, 0),
    (0, 2),
    (0, -2),
    (2, 1),
    (-2, 1),
    (2, -1),
    (-2, -1),
    (1, 2),
    (-1, 2),
    (1, -2),
    (-1, -2),
];

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
            |(dx, dy): (i32, i32)| match (position.x.checked_add(dx), position.y.checked_add(dy)) {
                (Some(x), Some(y)) => self.water(TileCoord::new(x, y)),
                _ => Ok(None),
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
                for offset in BAND {
                    match neighbor(offset)? {
                        None | Some(WaterKind::None) => {}
                        Some(WaterKind::River) => return Ok(Shore::Bank),
                        Some(_) => return Ok(Shore::Beach),
                    }
                }
                Ok(Shore::None)
            }
            WaterKind::Lake | WaterKind::Ocean => {
                for offset in &BAND[..8] {
                    if neighbor(*offset)? == Some(WaterKind::None) {
                        return Ok(Shore::Shallow);
                    }
                }
                Ok(Shore::None)
            }
            WaterKind::River | WaterKind::Shallow => Ok(Shore::None),
        }
    }
}
