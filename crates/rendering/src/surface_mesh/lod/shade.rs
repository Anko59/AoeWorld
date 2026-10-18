//! Slope shade of every displayed cell from a local plane fit of the heights
//! around it, so one-level terrace steps read as hillsides, not faceted tents.
use super::*;

/// Half-width of the fit window in tiles; coarse cells use at least one cell.
const WINDOW_TILES: u32 = 5;
/// Half-width of the broad window in tiles.
const BROAD_TILES: u32 = 20;

/// What one grid cell contributes: nothing, a cliff, or a mean height.
#[derive(Clone, Copy)]
pub(super) enum Ground {
    Absent,
    Cliff,
    Height(f64),
}

/// Summed-area tables over the cells that carry a height: count, x, y, height,
/// xx, xy, yy, x*height and y*height; then the cliff cells.
struct Tables {
    stride: usize,
    sums: [Vec<f64>; 10],
}

const CLIFFS: usize = 9;

impl Tables {
    fn new(ground: &[Ground], grid: CellGrid) -> Self {
        let stride = grid.width + 1;
        let mut sums = [(); 10].map(|()| vec![0.0; stride * (grid.height + 1)]);
        for y in 0..grid.height {
            let mut row = [0.0; 10];
            for x in 0..grid.width {
                match ground[y * grid.width + x] {
                    Ground::Height(z) => {
                        let (x, y) = (x as f64, y as f64);
                        for (sum, value) in
                            row.iter_mut()
                                .zip([1.0, x, y, z, x * x, x * y, y * y, x * z, y * z])
                        {
                            *sum += value;
                        }
                    }
                    Ground::Cliff => row[CLIFFS] += 1.0,
                    Ground::Absent => {}
                }
                for (table, value) in sums.iter_mut().zip(row) {
                    table[(y + 1) * stride + x + 1] = table[y * stride + x + 1] + value;
                }
            }
        }
        Self { stride, sums }
    }

    fn sum(&self, table: usize, x: [usize; 2], y: [usize; 2]) -> f64 {
        let at = |x: usize, y: usize| self.sums[table][y * self.stride + x];
        at(x[1] + 1, y[1] + 1) - at(x[0], y[1] + 1) - at(x[1] + 1, y[0]) + at(x[0], y[0])
    }

    /// Least-squares plane `z = a + bx + cy` over the cells present in the
    /// inclusive bounds, solving both slopes jointly from the centred normal
    /// equations: exact for a plane however visibility or missing cells clip the
    /// window, with the two axes never mixed. Returns `(b, c)`, in height levels
    /// per cell. A single row or column of cells fits its own axis only;
    /// other collinear windows and single cells have no slope.
    fn plane(&self, x: [usize; 2], y: [usize; 2]) -> [f64; 2] {
        let sum = |table| self.sum(table, x, y);
        let n = sum(0);
        if n < 1.5 {
            return [0.0; 2];
        }
        let (mx, my, mz) = (sum(1) / n, sum(2) / n, sum(3) / n);
        let sxx = sum(4) - n * mx * mx;
        let sxy = sum(5) - n * mx * my;
        let syy = sum(6) - n * my * my;
        let sxz = sum(7) - n * mx * mz;
        let syz = sum(8) - n * my * mz;
        let determinant = sxx * syy - sxy * sxy;
        if determinant > 1e-9 * (sxx * syy).max(1e-12) && determinant > 1e-9 {
            [
                (sxz * syy - syz * sxy) / determinant,
                (syz * sxx - sxz * sxy) / determinant,
            ]
        } else if syy < 1e-9 && sxx > 1e-9 {
            // A single row of cells (a narrow visible band): slope along it.
            [sxz / sxx, 0.0]
        } else if sxx < 1e-9 && syy > 1e-9 {
            [0.0, syz / syy]
        } else {
            [0.0; 2]
        }
    }
}

/// Share of the broad slope removed before shading. A hillside that fills the
/// view then keeps its average brightness, while the hills and valleys on it
/// (the part that reads as relief) keep their contrast.
const BROAD_SHARE: f64 = 0.8;

/// Height gradient (levels per cell) of the plane fitted over a window of
/// `reach` cells around `(x, y)`; `None` when a cliff lies inside it.
fn gradient(tables: &Tables, grid: CellGrid, at: [usize; 2], reach: usize) -> Option<[f64; 2]> {
    let [x, y] = at;
    let columns = [x.saturating_sub(reach), (x + reach).min(grid.width - 1)];
    let rows = [y.saturating_sub(reach), (y + reach).min(grid.height - 1)];
    (tables.sum(CLIFFS, columns, rows) < 0.5).then(|| tables.plane(columns, rows))
}

/// Shade in code steps, one per grid cell; zero where a cell has no height and
/// wherever a cliff lies inside the window (a cliff is not a slope, and its
/// native art carries its own relief). Heights are in levels. The shaded
/// gradient is the local one minus `BROAD_SHARE` of the broad one (about
/// `BROAD_TILES`), so a gentle map shows its hills and a uniform slope stays
/// near neutral; near a cliff only the local gradient remains.
pub(super) fn cell_shades(ground: &[Ground], grid: CellGrid) -> Vec<f32> {
    let cells = |tiles: u32| tiles.div_ceil(grid.size.unsigned_abs()).max(1) as usize;
    let (near, broad) = (cells(WINDOW_TILES), cells(BROAD_TILES).max(2));
    let tables = Tables::new(ground, grid);
    let size = f64::from(grid.size);
    (0..ground.len())
        .map(|index| {
            if !matches!(ground[index], Ground::Height(_)) {
                return 0.0;
            }
            let at = [index % grid.width, index / grid.width];
            let Some(local) = gradient(&tables, grid, at, near) else {
                return 0.0;
            };
            let wide = gradient(&tables, grid, at, broad).unwrap_or([0.0; 2]);
            let [east, south] = [0, 1].map(|axis| (local[axis] - BROAD_SHARE * wide[axis]) / size);
            relief::steps([east, south])
        })
        .collect()
}

/// Shade at every lattice corner of the grid (`width + 1` by `height + 1`): the
/// mean of the existing cells around it, with water and cliff cells counting as
/// unshaded. A corner belongs to all of its faces, so neighbours never differ.
pub(super) fn vertex_shades(ground: &[Ground], cells: &[f32], grid: CellGrid) -> Vec<f32> {
    let stride = grid.width + 1;
    let mut total = vec![0.0_f32; stride * (grid.height + 1)];
    let mut count = vec![0.0_f32; total.len()];
    for (index, cell) in ground.iter().enumerate() {
        if matches!(cell, Ground::Absent) {
            continue;
        }
        let (x, y) = (index % grid.width, index / grid.width);
        for corner in [
            y * stride + x,
            y * stride + x + 1,
            (y + 1) * stride + x,
            (y + 1) * stride + x + 1,
        ] {
            total[corner] += cells[index];
            count[corner] += 1.0;
        }
    }
    total
        .iter()
        .zip(&count)
        .map(|(total, count)| if *count > 0.0 { total / count } else { 0.0 })
        .collect()
}
