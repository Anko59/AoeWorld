//! Sparse explored row runs, used to clip strategic ownership geometry.
use crate::GameWorld;
use aoe_core::PlayerId;
impl GameWorld {
    pub(crate) fn explored_land_runs(&self, player: PlayerId) -> Vec<(i32, i32, i32)> {
        let mut runs = Vec::new();
        let Some(vision) = self.vision.players.get(&player) else {
            return runs;
        };
        for (&(cx, cy), words) in &vision.explored {
            for (word, &bits) in words.iter().enumerate() {
                for half in 0..2 {
                    let mut bits = (bits >> (half * 32)) as u32;
                    let mut offset = 0;
                    while bits != 0 {
                        let skip = bits.trailing_zeros();
                        bits >>= skip;
                        offset += skip;
                        let count = bits.trailing_ones();
                        let x = cx * 32 + offset as i32;
                        runs.push((cy * 32 + (word * 2 + half) as i32, x, x + count as i32));
                        if count == 32 {
                            break;
                        }
                        bits >>= count;
                        offset += count;
                    }
                }
            }
        }
        runs
    }
}
