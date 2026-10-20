//! Bounded water-only resource placement sharing the shoreline's water memo.
use super::*;
use crate::WaterKind;

impl<const N: usize, const W: usize> Memo<'_, N, W> {
    pub(in super::super) fn fish(
        &self,
        position: TileCoord,
        terrain: Tile,
    ) -> Result<Option<ResourceNode>, EnvironmentPageError> {
        if !matches!(terrain.water, WaterKind::Lake | WaterKind::Ocean)
            || terrain.material == GroundMaterial::Ford
        {
            return Ok(None);
        }
        let roll = unsigned_noise(
            self.generator.geography_key,
            b"fish",
            position.x,
            position.y,
        ) ^ self.generator.procedural_seed.rotate_left(17);
        let (object, amount) = if terrain.material == GroundMaterial::DeepWater {
            if !roll.is_multiple_of(64) {
                return Ok(None);
            }
            // Depth art alone can classify the centre of a narrow, warped
            // inlet as deep. Certify a twenty-three-tile cross of open water.
            if !self.fishing_water(position)? {
                return Ok(None);
            }
            (
                ObjectKind::DeepFish,
                if (roll >> 8) as u8 % 5 == 4 { 200 } else { 225 },
            )
        } else {
            if !roll.is_multiple_of(5) {
                return Ok(None);
            }
            let mut coast = false;
            for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                let land = TileCoord::new(position.x + dx, position.y + dy);
                if self
                    .base(land)?
                    .is_none_or(|t| !t.passable || t.water != WaterKind::None)
                {
                    continue;
                }
                // Twenty-three wet tiles exclude even warped narrow streams.
                let mut wide = true;
                for step in 1..=22 {
                    let water = TileCoord::new(position.x - dx * step, position.y - dy * step);
                    if !matches!(self.water(water)?, Some(WaterKind::Lake | WaterKind::Ocean)) {
                        wide = false;
                        break;
                    }
                }
                // A jagged bank can point along a stream. Also certify its
                // cross-channel width at the centre of the waterward ray.
                coast |= wide
                    && self.fishing_water(TileCoord::new(
                        position.x - dx * 11,
                        position.y - dy * 11,
                    ))?;
            }
            if !coast {
                return Ok(None);
            }
            (ObjectKind::ShoreFish, 200)
        };
        Ok(Some(ResourceNode {
            id: resource_id(position, 0),
            tile: position,
            kind: ResourceKind::Food,
            object,
            initial_amount: amount,
            visual_variant: (roll >> 8) as u8,
        }))
    }

    fn fishing_water(&self, position: TileCoord) -> Result<bool, EnvironmentPageError> {
        for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            for step in 0..=11 {
                if !matches!(
                    self.water(TileCoord::new(
                        position.x + dx * step,
                        position.y + dy * step,
                    ))?,
                    Some(WaterKind::Lake | WaterKind::Ocean)
                ) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}
