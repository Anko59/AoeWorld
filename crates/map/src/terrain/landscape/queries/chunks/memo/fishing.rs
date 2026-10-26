//! Bounded, coordinate-seeded fish placement in seas, lakes and river channels.
use super::*;
use crate::WaterKind;

impl<const N: usize, const W: usize> Memo<'_, N, W> {
    pub(in super::super) fn fish(
        &self,
        position: TileCoord,
        terrain: Tile,
    ) -> Result<Option<ResourceNode>, EnvironmentPageError> {
        if !matches!(
            terrain.water,
            WaterKind::Lake | WaterKind::Ocean | WaterKind::River
        ) || terrain.material == GroundMaterial::Ford
        {
            return Ok(None);
        }
        let roll = unsigned_noise(
            self.generator.geography_key,
            b"fish",
            position.x,
            position.y,
        ) ^ self.generator.procedural_seed.rotate_left(17);
        let mut bank = false;
        for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            bank |= self
                .base(TileCoord::new(position.x + dx, position.y + dy))?
                .is_some_and(|t| t.passable && t.water == WaterKind::None);
        }
        let (object, amount) = if bank {
            if !roll.is_multiple_of(5) {
                return Ok(None);
            }
            (ObjectKind::ShoreFish, 200)
        } else {
            // Rivers keep medium-water art even in their channel. A two-tile
            // wet cross certifies room for fishing ships without demanding a
            // sea-sized 23-tile channel. Ocean/lake deep fish retain depth art.
            if !roll.is_multiple_of(64)
                || (terrain.water != WaterKind::River
                    && terrain.material != GroundMaterial::DeepWater)
                || !self.fishing_water(
                    position,
                    if terrain.water == WaterKind::River {
                        2
                    } else {
                        11
                    },
                )?
            {
                return Ok(None);
            }
            (
                ObjectKind::DeepFish,
                if (roll >> 8) as u8 % 5 == 4 { 200 } else { 225 },
            )
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

    fn fishing_water(&self, position: TileCoord, reach: i32) -> Result<bool, EnvironmentPageError> {
        for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            for step in 1..=reach {
                if !matches!(
                    self.water(TileCoord::new(
                        position.x + dx * step,
                        position.y + dy * step
                    ))?,
                    Some(WaterKind::Lake | WaterKind::Ocean | WaterKind::River)
                ) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}
