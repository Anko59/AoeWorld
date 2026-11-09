//! A flat, sparse grid of `u32` values over tile-like coordinates: a
//! row-major directory of 16×16 pages, allocated only where a value is set
//! and released when the page holds none. A read is two array loads instead
//! of a hash probe; out-of-range and negative coordinates read as vacant.
const SHIFT: u32 = 4;
const SIDE: usize = 1 << SHIFT;
const NO_PAGE: u32 = u32::MAX;

#[derive(Debug)]
struct Page {
    values: [u32; SIDE * SIDE],
    /// Entries that are not vacant.
    used: u32,
}

#[derive(Debug)]
pub(super) struct Grid {
    vacant: u32,
    /// Pages per directory row; the directory grows to the farthest page set.
    width: usize,
    directory: Vec<u32>,
    pages: Vec<Page>,
    free: Vec<u32>,
}

impl Grid {
    pub(super) const fn new(vacant: u32) -> Self {
        Self {
            vacant,
            width: 0,
            directory: Vec::new(),
            pages: Vec::new(),
            free: Vec::new(),
        }
    }

    fn split(x: i32, y: i32) -> Option<(usize, usize, usize)> {
        let (x, y) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
        let local = ((y as usize) & (SIDE - 1)) * SIDE + ((x as usize) & (SIDE - 1));
        Some(((x >> SHIFT) as usize, (y >> SHIFT) as usize, local))
    }

    pub(super) fn get(&self, x: i32, y: i32) -> u32 {
        let Some((px, py, local)) = Self::split(x, y) else {
            return self.vacant;
        };
        if px >= self.width {
            return self.vacant;
        }
        match self.directory.get(py * self.width + px) {
            Some(&page) if page != NO_PAGE => self.pages[page as usize].values[local],
            _ => self.vacant,
        }
    }

    /// Writes `value`; a vacant value releases its page once the page is empty.
    /// Coordinates outside the non-negative range are ignored.
    pub(super) fn set(&mut self, x: i32, y: i32, value: u32) {
        let Some((px, py, local)) = Self::split(x, y) else {
            return;
        };
        let vacant = self.vacant;
        if value == vacant && self.get(x, y) == vacant {
            return;
        }
        if px >= self.width {
            self.widen(px + 1);
        }
        let at = py * self.width + px;
        if at >= self.directory.len() {
            self.directory.resize(at + 1, NO_PAGE);
        }
        if self.directory[at] == NO_PAGE {
            let page = Page {
                values: [vacant; SIDE * SIDE],
                used: 0,
            };
            self.directory[at] = match self.free.pop() {
                Some(slot) => {
                    self.pages[slot as usize] = page;
                    slot
                }
                None => {
                    self.pages.push(page);
                    (self.pages.len() - 1) as u32
                }
            };
        }
        let slot = self.directory[at];
        let page = &mut self.pages[slot as usize];
        let old = std::mem::replace(&mut page.values[local], value);
        page.used = page.used + u32::from(old == vacant) - u32::from(value == vacant);
        if page.used == 0 {
            self.directory[at] = NO_PAGE;
            self.free.push(slot);
        }
    }

    /// Every set value with its coordinates, in no particular order.
    #[cfg(test)]
    pub(super) fn entries(&self) -> impl Iterator<Item = (i32, i32, u32)> + '_ {
        self.directory
            .iter()
            .enumerate()
            .filter(|(_, page)| **page != NO_PAGE)
            .flat_map(move |(at, &page)| {
                let (px, py) = (at % self.width, at / self.width);
                let values = &self.pages[page as usize].values;
                values
                    .iter()
                    .enumerate()
                    .filter(move |(_, value)| **value != self.vacant)
                    .map(move |(local, &value)| {
                        let x = (px * SIDE + local % SIDE) as i32;
                        let y = (py * SIDE + local / SIDE) as i32;
                        (x, y, value)
                    })
            })
    }

    /// Re-lays the directory out at least `width` pages wide. Rare.
    fn widen(&mut self, width: usize) {
        let width = width.max(self.width * 2).max(16);
        let rows = self.directory.len().div_ceil(self.width.max(1));
        let mut directory = vec![NO_PAGE; rows * width];
        for (at, &page) in self.directory.iter().enumerate() {
            directory[(at / self.width) * width + at % self.width] = page;
        }
        self.directory = directory;
        self.width = width;
    }
}

#[cfg(test)]
mod tests;
