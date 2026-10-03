use super::*;
use crate::{MapChunkGenerator, WaterKind};
use aoe_core::TileCoord;
use std::collections::BTreeSet;

fn generator(recipe: u16) -> MapChunkGenerator {
    MapChunkGenerator::new([17; 32], 5, 1_024).with_elevation_sampling_recipe(recipe)
}

#[test]
fn canopy_density_has_open_marginal_and_contiguous_core_ranges() {
    let key = [17; 32];
    let mut low = (0_usize, 0_usize);
    let mut high = (0_usize, 0_usize);
    let mut visited = [false; 6];
    for y in (0..2_048).step_by(3) {
        for x in (0..2_048).step_by(3) {
            let field = canopy_strength(key, x, y);
            visited[usize::from(field / 180).min(5)] = true;
            let tree = crate::biome_rules::tree_present_for_recipe(
                key,
                x,
                y,
                Biome::Temperate,
                crate::GENERATION_RECIPE_VERSION,
            );
            if field < 260 {
                low.0 += usize::from(tree);
                low.1 += 1;
            }
            if field >= 740 {
                high.0 += usize::from(tree);
                high.1 += 1;
            }
        }
    }
    assert!(visited.into_iter().all(|bucket| bucket));
    let low_density = low.0 * 100 / low.1;
    let high_density = high.0 * 100 / high.1;
    assert!(
        low_density <= 20,
        "sparse opening density was {low_density}%"
    );
    assert!(
        high_density >= 80,
        "canopy core density was {high_density}%"
    );
    assert!(high_density >= low_density * 4);
}

#[test]
fn openings_are_seeded_irregular_variable_and_confined_to_their_cells() {
    let terrain = generator(crate::GENERATION_RECIPE_VERSION);
    let mut active = 0;
    let mut widths = BTreeSet::new();
    let mut heights = BTreeSet::new();
    let mut jittered = false;
    for cell_y in 0..12 {
        for cell_x in 0..12 {
            let cell = (cell_x, cell_y);
            let Some(shape) = opening_geometry_for_cell(&terrain, cell) else {
                continue;
            };
            active += 1;
            let bounds = shape.vertices.iter().fold(
                (i32::MAX, i32::MAX, i32::MIN, i32::MIN),
                |bounds, vertex| {
                    (
                        bounds.0.min(vertex.x),
                        bounds.1.min(vertex.y),
                        bounds.2.max(vertex.x),
                        bounds.3.max(vertex.y),
                    )
                },
            );
            assert!(bounds.0 >= cell_x * OPENING_GRID_TILES);
            assert!(bounds.1 >= cell_y * OPENING_GRID_TILES);
            assert!(bounds.2 < (cell_x + 1) * OPENING_GRID_TILES);
            assert!(bounds.3 < (cell_y + 1) * OPENING_GRID_TILES);
            widths.insert(bounds.2 - bounds.0);
            heights.insert(bounds.3 - bounds.1);
            let regular_center = TileCoord::new(
                cell_x * OPENING_GRID_TILES + OPENING_GRID_TILES / 2,
                cell_y * OPENING_GRID_TILES + OPENING_GRID_TILES / 2,
            );
            let center = shape_center(&terrain, cell);
            jittered |= center != regular_center;
            assert!(opening_contains(&terrain, center));
        }
    }
    assert!((80..130).contains(&active), "active openings: {active}");
    assert!(widths.len() >= 12, "opening widths: {widths:?}");
    assert!(heights.len() >= 12, "opening heights: {heights:?}");
    assert!(jittered);
    assert!(!opening_contains(&terrain, TileCoord::new(-1, -1)));
}

fn shape_center(generator: &MapChunkGenerator, cell: (i32, i32)) -> TileCoord {
    opening_center(cell_layout(generator, cell), cell)
}

#[test]
fn procedural_paths_bend_between_openings_and_make_dirt_ground() {
    let terrain = generator(crate::GENERATION_RECIPE_VERSION);
    let mut links = 0;
    for cell_y in -2..12 {
        for cell_x in -2..12 {
            for axis in 0..=1 {
                let Some(segment) = trail_segment(&terrain, (cell_x, cell_y), axis) else {
                    continue;
                };
                links += 1;
                assert!(opening_contains(&terrain, segment.start));
                assert!(opening_contains(&terrain, segment.end));
                for point in [segment.start, segment.bend, segment.end] {
                    assert!(procedural_trail_contains(&terrain, point));
                    assert!(clearing::suppresses_objects(
                        &terrain,
                        point,
                        Biome::Temperate
                    ));
                    assert_eq!(
                        material_for_tile(
                            &terrain,
                            point,
                            Biome::Temperate,
                            GroundMaterial::TemperateGrass,
                        ),
                        GroundMaterial::Dirt
                    );
                }
                assert!(near_segment(
                    segment.start,
                    segment.bend,
                    segment.start,
                    segment.half_width
                ));
                assert!(near_segment(
                    segment.bend,
                    segment.end,
                    segment.end,
                    segment.half_width
                ));
            }
        }
    }
    assert!(links > 20, "procedural links: {links}");
    assert!(!clearing::suppresses_objects(
        &terrain,
        TileCoord::new(32, 32),
        Biome::Temperate,
    ));
    assert!(!procedural_trail_contains(
        &generator(RECIPE_FIVE),
        TileCoord::new(96, 96)
    ));
}

const RECIPE_FIVE: u16 = crate::PRIOR_OVERVIEW_GENERATION_RECIPE_VERSION;

#[test]
fn temperate_accents_leave_source_biomes_water_slope_and_passability_intact() {
    let prior = generator(RECIPE_FIVE);
    let current = generator(crate::GENERATION_RECIPE_VERSION);
    let mut found_dirt = false;
    let mut found_dry_grass = false;
    let mut found_forest_floor = false;
    let mut found_grass = false;
    for y in 0..512 {
        for x in 0..512 {
            let coord = TileCoord::new(x, y);
            let old = prior.tile_at(coord).expect("prior tile");
            let new = current.tile_at(coord).expect("current tile");
            assert_eq!(
                old.geographic_height_centimeters,
                new.geographic_height_centimeters
            );
            assert_eq!(old.game_height_level, new.game_height_level);
            assert_eq!(old.surface, new.surface);
            assert_eq!(old.biome, new.biome);
            assert_eq!(old.vegetation_provenance, new.vegetation_provenance);
            assert_eq!(old.water, new.water);
            assert_eq!(old.water_provenance, new.water_provenance);
            assert_eq!(old.elevation_provenance, new.elevation_provenance);
            assert_eq!(old.passable, new.passable);
            if new.biome == Biome::Temperate && new.water == WaterKind::None {
                found_dirt |= new.material == GroundMaterial::Dirt;
                found_dry_grass |= new.material == GroundMaterial::DryGrass;
                found_forest_floor |= new.material == GroundMaterial::ForestFloor;
                found_grass |= new.material == GroundMaterial::TemperateGrass;
                assert_eq!(old.material, GroundMaterial::TemperateGrass);
            }
        }
    }
    assert!(found_dirt);
    assert!(found_dry_grass);
    assert!(found_forest_floor);
    assert!(found_grass);
    assert_eq!(
        material_for_tile(
            &current,
            TileCoord::new(96, 96),
            Biome::Woodland,
            GroundMaterial::ForestFloor,
        ),
        GroundMaterial::ForestFloor
    );
    assert_eq!(
        material_for_tile(
            &prior,
            TileCoord::new(96, 96),
            Biome::Temperate,
            GroundMaterial::TemperateGrass,
        ),
        GroundMaterial::TemperateGrass
    );
}

#[test]
fn recipe_seven_chunks_repeat_across_a_chunk_edge() {
    let first = generator(crate::GENERATION_RECIPE_VERSION);
    let repeated = generator(crate::GENERATION_RECIPE_VERSION);
    for chunk_y in [2, 3] {
        let left = first.chunk(0, chunk_y).expect("left chunk");
        let right = first.chunk(1, chunk_y).expect("right chunk");
        assert_eq!(left, repeated.chunk(0, chunk_y).expect("repeat left"));
        assert_eq!(right, repeated.chunk(1, chunk_y).expect("repeat right"));
        for offset in 0..32 {
            let coord = TileCoord::new(31, chunk_y * 32 + offset);
            assert_eq!(
                first.tile_at(coord),
                repeated.tile_at(coord),
                "west side of seam: {coord:?}"
            );
            let coord = TileCoord::new(32, chunk_y * 32 + offset);
            assert_eq!(
                first.tile_at(coord),
                repeated.tile_at(coord),
                "east side of seam: {coord:?}"
            );
        }
    }
}
