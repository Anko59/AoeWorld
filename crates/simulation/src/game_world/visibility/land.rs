//! Clip known holding runs using sparse exploration words, not a viewer-wide scan.
use crate::{GameWorld, HoldingSpan};
use aoe_core::PlayerId;
impl GameWorld {
    /// Last sightings only; no live-world scan or visibility probes per viewer.
    pub(crate) fn remembered_land_buildings(
        &self,
        player: PlayerId,
    ) -> impl Iterator<Item = &crate::GameBuilding> {
        self.vision
            .players
            .get(&player)
            .into_iter()
            .flat_map(|v| v.remembered.values())
            .filter(move |b| b.player != player)
    }
    pub(crate) fn clip_land_span(
        &self,
        player: PlayerId,
        span: HoldingSpan,
        out: &mut Vec<HoldingSpan>,
    ) {
        let Some(vision) = self.vision.players.get(&player) else {
            return;
        };
        let mut x = span.x0;
        while x < span.x1 {
            let end = span.x1.min((x.div_euclid(32) + 1) * 32);
            if let Some(words) = vision
                .explored
                .get(&(x.div_euclid(32), span.y.div_euclid(32)))
            {
                let index = (span.y.rem_euclid(32) * 32 + x.rem_euclid(32)) as usize;
                let mut bits =
                    ((words[index / 64] >> (index % 64)) & ((1u64 << (end - x)) - 1)) as u32;
                let mut offset = x;
                while bits != 0 {
                    let skip = bits.trailing_zeros();
                    bits >>= skip;
                    offset += skip as i32;
                    let count = bits.trailing_ones();
                    out.push(HoldingSpan {
                        x0: offset,
                        x1: offset + count as i32,
                        ..span
                    });
                    if count == 32 {
                        break;
                    }
                    bits >>= count;
                    offset += count as i32;
                }
            }
            x = end;
        }
    }
}
