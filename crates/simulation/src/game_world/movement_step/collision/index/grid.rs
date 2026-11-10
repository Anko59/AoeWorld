//! A flat, sparse grid of `u32` values over tile-like coordinates: a
//! row-major directory bounded by occupied 16×16 pages, allocated where a value is set
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
    /// Page-space origin and row width of the directory; distant first writes
    /// do not allocate the rectangle back to the map origin.
    origin: (usize, usize),
    width: usize,
    directory: Vec<u32>,
    pages: Vec<Page>,
    free: Vec<u32>,
}

impl Grid {
    pub(super) const fn new(vacant: u32) -> Self {
        Self {
            vacant,
            origin: (0, 0),
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
        let Some(at) = self.at(px, py) else {
            return self.vacant;
        };
        match self.directory.get(at) {
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
        self.grow_to(px, py);
        let at = (py - self.origin.1) * self.width + (px - self.origin.0);
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
                let (px, py) = (
                    at % self.width + self.origin.0,
                    at / self.width + self.origin.1,
                );
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

    fn at(&self, x: usize, y: usize) -> Option<usize> {
        let (x, y) = (x.checked_sub(self.origin.0)?, y.checked_sub(self.origin.1)?);
        (x < self.width).then(|| y * self.width + x)
    }

    /// Re-lays the directory only when its occupied bounds grow, amortizing
    /// small steps by doubling the growing dimension. Hot reads remain arrays.
    fn grow_to(&mut self, x: usize, y: usize) {
        if self.directory.is_empty() {
            self.origin = (x, y);
            self.width = 1;
            self.directory.push(NO_PAGE);
            return;
        }
        let rows = self.directory.len() / self.width;
        let mut origin = (x.min(self.origin.0), y.min(self.origin.1));
        let width = (x + 1).max(self.origin.0 + self.width) - origin.0;
        let height = (y + 1).max(self.origin.1 + rows) - origin.1;
        if width == self.width && height == rows {
            return;
        }
        let required = (width, height);
        let width = if width > self.width {
            width.max(self.width * 2)
        } else {
            width
        };
        let height = if height > rows {
            height.max(rows * 2)
        } else {
            height
        };
        // Put spare capacity on the growing side, so successive left/up steps
        // do not each double the rectangle again.
        if x < self.origin.0 {
            origin.0 = origin.0.saturating_sub(width - required.0);
        }
        if y < self.origin.1 {
            origin.1 = origin.1.saturating_sub(height - required.1);
        }
        let mut directory = vec![NO_PAGE; width * height];
        let dx = self.origin.0 - origin.0;
        let dy = self.origin.1 - origin.1;
        for (at, &page) in self.directory.iter().enumerate() {
            directory[(at / self.width + dy) * width + at % self.width + dx] = page;
        }
        self.directory = directory;
        self.origin = origin;
        self.width = width;
    }
}

#[cfg(test)]
mod tests;
