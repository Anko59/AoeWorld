//! Refine crop boundaries and mountain blending bands within the mesh budget.
use super::*;
use appearance::{FARM_FIELD, UNSEEDED_FARM_FIELD, is_farm_field};
use std::collections::{HashMap, hash_map::Entry};

pub(super) struct Patches {
    grid: CellGrid,
    mixed: Vec<bool>,
    refinement: i32,
    crop_patches: Vec<[usize; 2]>,
    crop_refinement: i32,
    crop_tiles: HashMap<[i32; 2], u8>,
}

impl Patches {
    pub(super) fn tiles(
        &self,
        tile: [i32; 2],
        divisions: i32,
    ) -> impl Iterator<Item = ([i32; 2], i32, Option<u8>)> + '_ {
        (0..divisions * divisions).flat_map(move |patch| {
            let origin = [
                tile[0] + patch % divisions * self.grid.size,
                tile[1] + patch / divisions * self.grid.size,
            ];
            let index = grid_index(self.grid, origin);
            let counts = index
                .and_then(|index| self.crop_patches.get(index))
                .copied()
                .unwrap_or_default();
            let full = (self.grid.size as usize).saturating_pow(2);
            let full_crop = counts
                .iter()
                .position(|&count| count == full)
                .map(|state| [FARM_FIELD, UNSEEDED_FARM_FIELD][state]);
            let refinement = if full_crop.is_some() {
                1
            } else if counts != [0; 2] {
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
                (
                    tile,
                    step,
                    full_crop.or_else(|| self.crop_tiles.get(&tile).copied()),
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
    base: usize,
) -> Patches {
    let patch_grid = CellGrid {
        min: grid.min,
        size: grid.size / divisions,
        width: grid.width * divisions as usize,
        height: grid.height * divisions as usize,
    };
    // Store per-patch masks and only the farm tiles inside these bounds.
    let len = patch_grid.width * patch_grid.height;
    let mut masks = vec![0_u32; len];
    let mut crop_patches = Vec::new();
    let mut crop_tiles = HashMap::new();
    if grid.size > 1 {
        for sample in terrain {
            let tile = tile_key(sample.position);
            if is_farm_field(sample.material) {
                if let Some(index) = grid_index(patch_grid, cell_key(tile, patch_grid.size))
                    && let Entry::Vacant(entry) = crop_tiles.entry(tile)
                {
                    entry.insert(sample.material);
                    if crop_patches.is_empty() {
                        crop_patches.resize(len, [0; 2]);
                    }
                    crop_patches[index][usize::from(sample.material == UNSEEDED_FARM_FIELD)] += 1;
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
            let side = patch_grid.size;
            let first = cell_key(tile.map(|value| value.saturating_sub(1)), side);
            let last = cell_key(tile.map(|value| value.saturating_add(1)), side);
            for y in (first[1]..=last[1]).step_by(side as usize) {
                for x in (first[0]..=last[0]).step_by(side as usize) {
                    if let Some(index) = grid_index(patch_grid, [x, y]) {
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
        let counts = crop_patches.get(index).copied().unwrap_or_default();
        let tile = grid_cell(patch_grid, index);
        if grid_index(grid, cell_key(tile, grid.size)).is_some_and(|i| cells[i].is_some()) {
            crop_boundaries += usize::from(counts != [0; 2] && !counts.contains(&full));
            mountain_boundaries += usize::from(counts == [0; 2] && *mixed);
        }
    }
    let (crop_refinement, crop_extra) = refinement(
        patch_grid.size,
        crop_boundaries,
        MAX_SURFACE_TRIANGLES.saturating_sub(base),
    );
    let crop_step = patch_grid.size / crop_refinement;
    if crop_step > 1 {
        let mut coarse = HashMap::<_, u8>::new();
        for (tile, material) in crop_tiles {
            // Keep a planted field when both states share one budgeted patch.
            coarse
                .entry(cell_key(tile, crop_step))
                .and_modify(|old| *old = (*old).min(material))
                .or_insert(material);
        }
        crop_tiles = coarse;
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
