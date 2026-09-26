use super::elevation::{AxisPosition, bilinear_height, source_axis_position};
use super::{
    Biome, GroundMaterial, MapChunkGenerator, ObjectKind, Provenance, ResourceKind, ResourceNode,
    Tile, WaterKind, resources, surface,
};
use crate::{
    ENVIRONMENT_PAGE_SAMPLES, EnvironmentPage, EnvironmentPageError, EnvironmentPageKey,
    HydrologyEvidenceMethod, PageLayer,
    biome_rules::{biome_from_potential_class, material_for, tree_present},
};
use aoe_core::TileCoord;

mod helpers;
mod water_model;

use helpers::{elevation_value, load_page, page_index, source_coordinate};

pub(super) fn sample_tile(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<Tile, EnvironmentPageError> {
    let environment = generator
        .provider_environment
        .as_ref()
        .ok_or(EnvironmentPageError::Invalid)?;
    let compression = generator
        .provider_compression
        .ok_or(EnvironmentPageError::Invalid)?;
    let fallback_height = super::signed_noise(
        generator.geography_key,
        b"relief",
        tile.x.div_euclid(8),
        tile.y.div_euclid(8),
    )
    .saturating_mul(25)
    .saturating_add(
        super::signed_noise(generator.geography_key, b"relief-detail", tile.x, tile.y) / 8,
    );
    let fallback_water = fallback_water(generator, tile);
    let fallback_biome = fallback_biome(generator, tile);

    let biome = if environment.vegetation.is_some() {
        let class = sample_biome_class(generator, environment.samples_per_axis, tile, cancelled)?;
        class
            .and_then(biome_from_potential_class)
            .map(|biome| (biome, Provenance::SourceDerived))
            .unwrap_or((fallback_biome, Provenance::Fallback))
    } else {
        (fallback_biome, Provenance::Fallback)
    };
    let (
        geographic_height_centimeters,
        mut game_height_level,
        mut surface_kind,
        elevation_provenance,
    ) = if environment.samples_per_axis > 0 {
        let (height, corners) =
            sample_elevation(generator, environment.samples_per_axis, tile, cancelled)?;
        (
            height,
            super::quantize_game_height(height, compression),
            surface::from_heights(corners, compression),
            Provenance::SourceDerived,
        )
    } else {
        (
            fallback_height,
            super::quantize_game_height(fallback_height, super::compression_fallback()),
            surface::from_heights([fallback_height; 4], super::compression_fallback()),
            Provenance::Fallback,
        )
    };
    let (mut water, mut water_provenance) = if environment.water.is_some() {
        let coverage = sample_water(generator, environment.samples_per_axis, tile, cancelled)?;
        coverage
            .map(|(ocean, inland)| match ocean {
                1..=50 => (WaterKind::Shallow, Provenance::SourceDerived),
                51..=100 => (WaterKind::Ocean, Provenance::SourceDerived),
                _ => match inland {
                    0 => (WaterKind::None, Provenance::SourceDerived),
                    1..=50 => (WaterKind::Shallow, Provenance::SourceDerived),
                    _ => (WaterKind::Lake, Provenance::SourceDerived),
                },
            })
            .unwrap_or((WaterKind::None, Provenance::SourceDerived))
    } else {
        (fallback_water, Provenance::Fallback)
    };
    let (hydrology_observation, modern_land_cover_class, modeled_water) =
        if let Some(index) = &environment.hydrology_evidence {
            water_model::sample_typed_evidence(generator, index.samples_per_axis, tile, cancelled)?
        } else {
            (None, None, None)
        };
    water_model::apply_to_tile(
        modeled_water,
        compression,
        &mut water,
        &mut game_height_level,
        &mut surface_kind,
        &mut water_provenance,
    );
    if water == WaterKind::Lake
        && modeled_water.is_none()
        && hydrology_observation.is_some_and(|observation| {
            observation.kind == crate::HydrologyKind::River
                && observation.method == HydrologyEvidenceMethod::HydroRiversBufferedCorridor
        })
    {
        water = WaterKind::River;
    }
    let material = match water {
        WaterKind::None => material_for(biome.0, geographic_height_centimeters),
        WaterKind::River | WaterKind::Lake | WaterKind::Ocean => GroundMaterial::Water,
        WaterKind::Shallow => GroundMaterial::Shore,
    };
    Ok(Tile {
        geographic_height_centimeters,
        game_height_level,
        surface: surface_kind,
        material,
        biome: biome.0,
        vegetation_provenance: biome.1,
        water,
        elevation_provenance,
        water_provenance,
        hydrology_observation,
        modern_land_cover_class,
        passable: water == WaterKind::None
            && material != GroundMaterial::Ice
            && surface_kind.walkable(),
    })
}

pub(super) fn resource_at(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    sample: Tile,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<ResourceNode>, EnvironmentPageError> {
    if !sample.passable {
        return Ok(None);
    }
    if super::clearing::contains(generator, tile) {
        return Ok(None);
    }
    let value = super::unsigned_noise(generator.geography_key, b"objects", tile.x, tile.y)
        ^ generator.procedural_seed.rotate_left(17);
    let land_use = if generator
        .provider_environment
        .as_ref()
        .is_some_and(|environment| environment.historical_land_use.is_some())
    {
        sample_land_use(
            generator,
            generator
                .provider_environment
                .as_ref()
                .map(|environment| environment.samples_per_axis)
                .unwrap_or(0),
            tile,
            cancelled,
        )?
    } else {
        None
    };
    let historically_cleared = land_use
        .map(|(crop, grazing, _)| {
            generator.is_tree_suppressed_by_historical_land_use(tile, crop, grazing)
        })
        .unwrap_or(false);
    if !historically_cleared && tree_present(generator.geography_key, tile.x, tile.y, sample.biome)
    {
        return Ok(Some(ResourceNode {
            id: super::resource_id(tile, 0),
            tile,
            kind: ResourceKind::Wood,
            object: ObjectKind::Tree,
            initial_amount: 100,
            visual_variant: (value >> 8) as u8,
        }));
    }
    resources::at_with_access(generator, tile, sample, |neighbor| {
        if neighbor.x < 0
            || neighbor.y < 0
            || neighbor.x >= generator.width_tiles
            || neighbor.y >= generator.width_tiles
        {
            return Ok(false);
        }
        let sample = sample_tile(generator, neighbor, cancelled)?;
        Ok(!occupied_without_access(
            generator, neighbor, sample, cancelled,
        )?)
    })
}

fn occupied_without_access(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    sample: Tile,
    cancelled: &dyn Fn() -> bool,
) -> Result<bool, EnvironmentPageError> {
    if !sample.passable {
        return Ok(true);
    }
    if super::clearing::contains(generator, tile) {
        return Ok(false);
    }
    let land_use = if generator
        .provider_environment
        .as_ref()
        .is_some_and(|environment| environment.historical_land_use.is_some())
    {
        sample_land_use(
            generator,
            generator
                .provider_environment
                .as_ref()
                .map(|environment| environment.samples_per_axis)
                .unwrap_or(0),
            tile,
            cancelled,
        )?
    } else {
        None
    };
    let historically_cleared = land_use
        .map(|(crop, grazing, _)| {
            generator.is_tree_suppressed_by_historical_land_use(tile, crop, grazing)
        })
        .unwrap_or(false);
    if !historically_cleared && tree_present(generator.geography_key, tile.x, tile.y, sample.biome)
    {
        return Ok(true);
    }
    Ok(resources::candidate(generator, tile, sample).is_some())
}

fn sample_elevation(
    generator: &MapChunkGenerator,
    samples: u16,
    tile: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<(i32, [i32; 4]), EnvironmentPageError> {
    let coordinates = [
        (tile.x, tile.y),
        (tile.x.saturating_add(1), tile.y),
        (tile.x.saturating_add(1), tile.y.saturating_add(1)),
        (tile.x, tile.y.saturating_add(1)),
    ];
    if generator.elevation_sampling_recipe != crate::LEGACY_GENERATION_RECIPE_VERSION {
        let height = sample_bilinear_elevation(
            generator,
            tile.x,
            tile.y,
            samples,
            AxisPosition::TileCenter,
            cancelled,
        )?;
        let mut corners = [0; 4];
        for (index, (x, y)) in coordinates.into_iter().enumerate() {
            corners[index] = sample_bilinear_elevation(
                generator,
                x,
                y,
                samples,
                AxisPosition::Corner,
                cancelled,
            )?;
        }
        return Ok((height, corners));
    }
    let mut corners = [0; 4];
    for (index, (x, y)) in coordinates.into_iter().enumerate() {
        let (source_x, source_y) = source_coordinate(x, y, samples, generator.width_tiles)?;
        corners[index] = provider_elevation(generator, source_x, source_y, cancelled)?;
    }
    Ok((corners[0], corners))
}

fn sample_bilinear_elevation(
    generator: &MapChunkGenerator,
    x: i32,
    y: i32,
    samples: u16,
    position: AxisPosition,
    cancelled: &dyn Fn() -> bool,
) -> Result<i32, EnvironmentPageError> {
    let (x0, x1, x_remainder, denominator) =
        source_axis_position(x, samples, generator.width_tiles, position)
            .ok_or(EnvironmentPageError::Invalid)?;
    let (y0, y1, y_remainder, _) =
        source_axis_position(y, samples, generator.width_tiles, position)
            .ok_or(EnvironmentPageError::Invalid)?;
    Ok(bilinear_height(
        [
            provider_elevation(generator, x0, y0, cancelled)?,
            provider_elevation(generator, x1, y0, cancelled)?,
            provider_elevation(generator, x1, y1, cancelled)?,
            provider_elevation(generator, x0, y1, cancelled)?,
        ],
        x_remainder,
        y_remainder,
        denominator,
    ))
}

fn provider_elevation(
    generator: &MapChunkGenerator,
    source_x: u16,
    source_y: u16,
    cancelled: &dyn Fn() -> bool,
) -> Result<i32, EnvironmentPageError> {
    let page = load_page(
        generator,
        EnvironmentPageKey {
            layer: PageLayer::Elevation,
            level: 0,
            x: source_x / u16::from(ENVIRONMENT_PAGE_SAMPLES),
            y: source_y / u16::from(ENVIRONMENT_PAGE_SAMPLES),
        },
        cancelled,
    )?;
    elevation_value(&page, source_x, source_y)
}

fn sample_water(
    generator: &MapChunkGenerator,
    samples: u16,
    tile: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<(u8, u8)>, EnvironmentPageError> {
    let (source_x, source_y) = source_coordinate(tile.x, tile.y, samples, generator.width_tiles)?;
    let page = load_page(
        generator,
        EnvironmentPageKey {
            layer: PageLayer::Water,
            level: 0,
            x: source_x / u16::from(ENVIRONMENT_PAGE_SAMPLES),
            y: source_y / u16::from(ENVIRONMENT_PAGE_SAMPLES),
        },
        cancelled,
    )?;
    let page = match page.as_ref() {
        EnvironmentPage::Water(page) => page,
        _ => return Err(EnvironmentPageError::Corrupt),
    };
    let index = page_index(page.width, page.height, source_x, source_y)?;
    Ok(Some((
        page.ocean_coverage_percent[index],
        page.inland_coverage_percent[index],
    )))
}

fn sample_biome_class(
    generator: &MapChunkGenerator,
    samples: u16,
    tile: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<u8>, EnvironmentPageError> {
    let (source_x, source_y) = source_coordinate(tile.x, tile.y, samples, generator.width_tiles)?;
    let page = load_page(
        generator,
        EnvironmentPageKey {
            layer: PageLayer::Vegetation,
            level: 0,
            x: source_x / u16::from(ENVIRONMENT_PAGE_SAMPLES),
            y: source_y / u16::from(ENVIRONMENT_PAGE_SAMPLES),
        },
        cancelled,
    )?;
    let page = match page.as_ref() {
        EnvironmentPage::Vegetation(page) => page,
        _ => return Err(EnvironmentPageError::Corrupt),
    };
    let index = page_index(page.width, page.height, source_x, source_y)?;
    Ok(Some(page.potential_biome_class[index]))
}

fn sample_land_use(
    generator: &MapChunkGenerator,
    samples: u16,
    tile: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<(u8, u8, u16)>, EnvironmentPageError> {
    let (source_x, source_y) = source_coordinate(tile.x, tile.y, samples, generator.width_tiles)?;
    let page = load_page(
        generator,
        EnvironmentPageKey {
            layer: PageLayer::HistoricalLandUse,
            level: 0,
            x: source_x / u16::from(ENVIRONMENT_PAGE_SAMPLES),
            y: source_y / u16::from(ENVIRONMENT_PAGE_SAMPLES),
        },
        cancelled,
    )?;
    let page = match page.as_ref() {
        EnvironmentPage::HistoricalLandUse(page) => page,
        _ => return Err(EnvironmentPageError::Corrupt),
    };
    let index = page_index(page.width, page.height, source_x, source_y)?;
    Ok(Some((
        page.crop_percent[index],
        page.grazing_percent[index],
        page.population_pressure_per_square_kilometer[index],
    )))
}

fn fallback_water(generator: &MapChunkGenerator, tile: TileCoord) -> WaterKind {
    if super::unsigned_noise(
        generator.geography_key,
        b"water",
        tile.x.div_euclid(16),
        tile.y.div_euclid(16),
    )
    .is_multiple_of(97)
    {
        WaterKind::Lake
    } else if super::unsigned_noise(
        generator.geography_key,
        b"river",
        tile.x.div_euclid(4),
        tile.y.div_euclid(4),
    )
    .is_multiple_of(521)
    {
        WaterKind::River
    } else {
        WaterKind::None
    }
}

fn fallback_biome(generator: &MapChunkGenerator, tile: TileCoord) -> Biome {
    match super::unsigned_noise(
        generator.geography_key,
        b"biome",
        tile.x.div_euclid(32),
        tile.y.div_euclid(32),
    ) % 10
    {
        0 => Biome::Tropical,
        1 => Biome::Boreal,
        2 => Biome::Woodland,
        3 => Biome::Savanna,
        4 => Biome::Steppe,
        5 => Biome::Desert,
        6 => Biome::Tundra,
        7 => Biome::Alpine,
        8 => Biome::Polar,
        _ => Biome::Temperate,
    }
}
