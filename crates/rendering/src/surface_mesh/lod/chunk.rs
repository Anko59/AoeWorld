use super::*;

/// The base mesh needs neighboring height samples and the cliff support ring.
/// A refined patch also needs the broad relief fit and its incident vertices.
pub fn chunk_surface_halo(size: i32, refined: bool) -> i32 {
    if refined {
        ((20_u32.div_ceil(size as u32) as i32).max(2) + 1) * size
    } else {
        2 * size
    }
}

/// Build one aligned map chunk, with its surroundings supplying edge data.
/// Faces retain world coordinates and can be projected for any camera.
pub fn chunk_surface_triangles(
    terrain: &[SceneTerrain],
    region: TileRect,
    size: i32,
    refined: bool,
) -> Vec<ProjectedSurfaceTriangle> {
    let halo = chunk_surface_halo(size, refined);
    let bounds = [
        [region.min.x - halo, region.min.y - halo],
        [region.max.x + halo - 1, region.max.y + halo - 1],
    ];
    let Some(grid) = visible_grid(bounds, size) else {
        return Vec::new();
    };
    let (cells, bank_cells) = sampled_cells(terrain, &vec![true; terrain.len()], grid);
    let (tops, skirts) = budget::surface_counts(&cells, grid);
    let divisions = if size == 1 { 1 } else { 2 };
    let samples = MeshSamples {
        grid,
        cells,
        bank_cells,
        tops,
        skirts,
        divisions,
    };
    mesh_surfaces(
        terrain,
        &Camera::new([0.0; 2], [0.0; 2]),
        samples,
        Some(region),
        refined,
    )
}

// A unit-sized textured subdivision can use the source shore exactly while
// sharing the coarser cell's geometry. Limit this scratch lookup to wet patches.
pub(super) struct ShoreMaterials<'a> {
    keys: crate::WorldKeyIndex,
    samples: Vec<&'a SceneTerrain>,
}

impl<'a> ShoreMaterials<'a> {
    pub(super) fn new(terrain: &'a [SceneTerrain], region: TileRect) -> Option<Self> {
        let sources = terrain.iter().filter(|sample| {
            let [x, y] = tile_key(sample.position);
            region.contains(TileCoord::new(x, y))
        });
        if !sources.clone().any(Self::wet) {
            return None;
        }
        let mut shore = Self {
            keys: crate::WorldKeyIndex::default(),
            samples: Vec::new(),
        };
        for sample in sources {
            if shore.keys.entry(tile_key(sample.position)) == shore.samples.len() {
                shore.samples.push(sample);
            }
        }
        Some(shore)
    }

    fn wet(sample: &SceneTerrain) -> bool {
        sample.surface.water != 0 || appearance::water_material(sample.material)
    }

    pub(super) fn preserve(&self, tile: [i32; 2], sample: &mut SceneTerrain) {
        if let Some(source) = self.keys.get(tile).map(|index| self.samples[index])
            && (Self::wet(source) || Self::wet(sample))
        {
            sample.material = source.material;
            sample.appearance = source.appearance;
            sample.surface.water = source.surface.water;
        }
    }
}
