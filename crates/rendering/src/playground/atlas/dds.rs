//! Native planar cells and deterministic view-resolution mip ranges.
use super::TerrainTopology;
impl TerrainTopology {
    #[inline(never)]
    pub fn dds_frames(self) -> Option<usize> {
        let Self::Dds {
            mut size, period, ..
        } = self
        else {
            return None;
        };
        if size
            .iter()
            .any(|&n| !(16..=4096).contains(&n) || !n.is_power_of_two())
            || period.iter().any(|&n| n == 0 || n > 256)
        {
            return None;
        }
        let mut count = 0;
        loop {
            count += usize::from(size[0].div_ceil(256)) * usize::from(size[1].div_ceil(256));
            if size[0].max(size[1]) <= 16 {
                return Some(count);
            }
            size = size.map(|n| (n / 2).max(1));
        }
    }
    #[inline(never)]
    pub(crate) fn active(self, frames: usize) -> Option<std::ops::Range<usize>> {
        let Self::Dds {
            mut size, level, ..
        } = self
        else {
            return Some(0..frames);
        };
        if self.dds_frames() != Some(frames) {
            return None;
        }
        let mut start = 0;
        for i in 0..=level {
            let count = usize::from(size[0].div_ceil(256)) * usize::from(size[1].div_ceil(256));
            if i == level {
                return Some(start..start + count);
            }
            start += count;
            size = size.map(|n| (n / 2).max(1));
        }
        None
    }
    #[inline(never)]
    pub(crate) fn grid(self) -> Option<([u16; 2], [u16; 2])> {
        let Self::Dds {
            size,
            period,
            level,
        } = self
        else {
            return None;
        };
        Some((
            size.map(|n| {
                (n.checked_shr(u32::from(level)).unwrap_or(0))
                    .max(1)
                    .div_ceil(256)
            }),
            period,
        ))
    }
    #[inline(never)]
    pub(crate) fn planar(self, world: [f64; 2]) -> Option<(usize, [f64; 2], [f64; 2])> {
        let (grid, period) = self.grid()?;
        let scale = [0, 1].map(|i| f64::from(grid[i]) / f64::from(period[i]));
        let p = [0, 1].map(|i| world[i].rem_euclid(f64::from(period[i])) * scale[i]);
        let index = (p[1].floor() as usize).min(usize::from(grid[1]) - 1) * usize::from(grid[0])
            + (p[0].floor() as usize).min(usize::from(grid[0]) - 1);
        Some((index, p.map(|n| n - n.floor()), scale))
    }
}
