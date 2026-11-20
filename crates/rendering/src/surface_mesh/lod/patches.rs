//! Refine crop boundaries and mountain blending bands within the mesh budget.
use super::*;
use appearance::FARM_FIELD;
use std::collections::HashSet;

pub(super) struct Patches {
    grid: CellGrid,
    mixed: Vec<bool>,
    refinement: i32,
    crop_patches: Vec<usize>,
    crop_refinement: i32,
    crop_tiles: HashSet<[i32; 2]>,
}

impl Patches {
    pub(super) fn tiles(
        &self,
        tile: [i32; 2],
        divisions: i32,
    ) -> impl Iterator<Item = ([i32; 2], i32, bool)> + '_ {
        (0..divisions * divisions).flat_map(move |patch| {
            let origin = [
                tile[0] + patch % divisions * self.grid.size,
                tile[1] + patch / divisions * self.grid.size,
            ];
            let index = grid_index(self.grid, origin);
            let count = index
                .and_then(|index| self.crop_patches.get(index))
                .copied()
                .unwrap_or(0);
            let full_crop = count == (self.grid.size as usize).saturating_pow(2);
            let refinement = if full_crop {
                1
            } else if count > 0 {
                self.crop_refinement
            } else if index.is_some_and(|index| self.mixed[index]) {
                self.refinement
            } else {
                1
            };
            let step = self.grid.size / refinement;
            (0..refinement * refinement).map(move |sub| {
                let tile = [
                    origin[0] + sub % refinement * step,
                    origin[1] + sub / refinement * step,
                ];
                (tile, step, full_crop || self.crop_tiles.contains(&tile))
            })
        })
    }
}

pub(super) fn plan(
    terrain: &[SceneTerrain],
    grid: CellGrid,
    divisions: i32,
    cells: &[Option<CellSample>],
    base: usize,
) -> Patches {
    let patch_grid = CellGrid {
        min: grid.min,
        size: grid.size / divisions,
        width: grid.width * divisions as usize,
        height: grid.height * divisions as usize,
    };
    // Store per-patch masks and only the planted tiles inside these bounds.
    let len = patch_grid.width * patch_grid.height;
    let mut masks = vec![0_u32; len];
    let mut crop_patches = Vec::new();
    let mut crop_tiles = HashSet::new();
    if grid.size > 1 {
        for sample in terrain {
            let tile = tile_key(sample.position);
            if sample.material == FARM_FIELD {
                if let Some(index) = grid_index(patch_grid, cell_key(tile, patch_grid.size))
                    && crop_tiles.insert(tile)
                {
                    if crop_patches.is_empty() {
                        crop_patches.resize(len, 0);
                    }
                    crop_patches[index] += 1;
                }
                continue;
            }
            if patch_grid.size == 1 {
                continue;
            }
            let Some((material, _)) = mix::ground(sample) else {
                continue;
            };
            let Some(bit) = 1_u32.checked_shl(u32::from(material)) else {
                continue;
            };
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
    let (mut crop_boundaries, mut mountain_boundaries) = (0, 0);
    let full = (patch_grid.size as usize).saturating_pow(2);
    for (index, mixed) in mixed.iter().enumerate() {
        let count = crop_patches.get(index).copied().unwrap_or(0);
        let tile = grid_cell(patch_grid, index);
        if grid_index(grid, cell_key(tile, grid.size)).is_some_and(|i| cells[i].is_some()) {
            crop_boundaries += usize::from(count > 0 && count < full);
            mountain_boundaries += usize::from(count == 0 && *mixed);
        }
    }
    let (crop_refinement, crop_extra) = refinement(
        patch_grid.size,
        crop_boundaries,
        MAX_SURFACE_TRIANGLES.saturating_sub(base),
    );
    let crop_step = patch_grid.size / crop_refinement;
    if crop_step > 1 {
        crop_tiles = crop_tiles
            .into_iter()
            .map(|tile| cell_key(tile, crop_step))
            .collect();
    }
    let (refinement, _) = refinement(
        patch_grid.size,
        mountain_boundaries,
        MAX_SURFACE_TRIANGLES.saturating_sub(base.saturating_add(crop_extra)),
    );
    Patches {
        grid: patch_grid,
        mixed,
        refinement,
        crop_patches,
        crop_refinement,
        crop_tiles,
    }
}

fn refinement(mut factor: i32, patches: usize, available: usize) -> (i32, usize) {
    loop {
        let extra = (factor as usize)
            .saturating_pow(2)
            .saturating_sub(1)
            .saturating_mul(patches)
            .saturating_mul(2);
        if extra <= available {
            return (factor, extra);
        }
        factor /= 2;
    }
}
