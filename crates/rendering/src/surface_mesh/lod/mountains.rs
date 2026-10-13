//! Keep the source-tile blending band on coarse mountain ground.
use super::*;

pub(super) struct Patches {
    grid: CellGrid,
    mixed: Vec<bool>,
    refinement: i32,
}

impl Patches {
    pub(super) fn tiles(
        &self,
        tile: [i32; 2],
        divisions: i32,
    ) -> impl Iterator<Item = ([i32; 2], i32)> + '_ {
        (0..divisions * divisions).flat_map(move |patch| {
            let origin = [
                tile[0] + patch % divisions * self.grid.size,
                tile[1] + patch / divisions * self.grid.size,
            ];
            let refinement = if grid_index(self.grid, origin).is_some_and(|i| self.mixed[i]) {
                self.refinement
            } else {
                1
            };
            let step = self.grid.size / refinement;
            (0..refinement * refinement).map(move |sub| {
                (
                    [
                        origin[0] + sub % refinement * step,
                        origin[1] + sub / refinement * step,
                    ],
                    step,
                )
            })
        })
    }
}

pub(super) fn plan(
    terrain: &[SceneTerrain],
    grid: CellGrid,
    divisions: i32,
    cells: &[Option<CellSample>],
) -> Patches {
    let patch_grid = CellGrid {
        min: grid.min,
        size: grid.size / divisions,
        width: grid.width * divisions as usize,
        height: grid.height * divisions as usize,
    };
    // Bounded by the displayed patch budget, never the resident tile count.
    let mut masks = vec![0_u32; patch_grid.width * patch_grid.height];
    if patch_grid.size > 1 {
        for sample in terrain {
            let Some((material, _)) = mix::ground(sample) else {
                continue;
            };
            let Some(bit) = 1_u32.checked_shl(u32::from(material)) else {
                continue;
            };
            let tile = tile_key(sample.position);
            // Include the incident-cell halo at each patch's boundary. An
            // interior border also marks its patch even if no corner sees it.
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (Some(x), Some(y)) = (tile[0].checked_add(dx), tile[1].checked_add(dy))
                    else {
                        continue;
                    };
                    if let Some(index) = grid_index(patch_grid, cell_key([x, y], patch_grid.size)) {
                        masks[index] |= bit;
                    }
                }
            }
        }
    }
    let mixed = masks
        .into_iter()
        .map(|mask| mask & ((1 << 4) | (1 << 7)) != 0 && mask.count_ones() > 1)
        .collect::<Vec<_>>();
    let mut marked = 0;
    for (index, mask) in mixed.iter().enumerate() {
        let tile = grid_cell(patch_grid, index);
        if *mask && grid_index(grid, cell_key(tile, grid.size)).is_some_and(|i| cells[i].is_some())
        {
            marked += 1;
        }
    }
    let base = cells.iter().flatten().count() * (divisions * divisions) as usize * 2
        + budget::skirt_reserve(cells, grid);
    let mut refinement = patch_grid.size;
    let extra = |refinement: i32| {
        (refinement as usize)
            .saturating_mul(refinement as usize)
            .saturating_sub(1)
            .saturating_mul(marked)
            .saturating_mul(2)
    };
    while refinement > 1 && base.saturating_add(extra(refinement)) > MAX_SURFACE_TRIANGLES {
        refinement /= 2;
    }
    Patches {
        grid: patch_grid,
        mixed,
        refinement,
    }
}
