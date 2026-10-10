use super::elevation::{AxisPosition, bilinear_height, source_axis_position};
use super::fallback::{fallback_biome, fallback_height, fallback_water, water_from_coverage};
use super::{GroundMaterial, MapChunkGenerator, Provenance, Tile, WaterKind, surface};
use crate::{
    ENVIRONMENT_PAGE_SAMPLES, EnvironmentPage, EnvironmentPageError, EnvironmentPageKey,
    HydrologyEvidenceMethod, PageLayer,
    biome_rules::{biome_from_potential_class, material_for},
};
use aoe_core::TileCoord;

mod helpers;
mod history;
mod water_model;

pub(super) use history::sample_land_use_observation;

use helpers::{elevation_value, load_page, page_index, source_coordinate};

/// Undecorated source terrain; preserves page failures and caller cancellation.
pub(super) fn sample_base_tile(
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
    let biome = if let Some(axis) = environment.vegetation_samples_per_axis() {
        let class = sample_biome_class(generator, axis, tile, cancelled)?;
        class
            .and_then(biome_from_potential_class)
            .map(|biome| (biome, Provenance::SourceDerived))
            .unwrap_or_else(|| (fallback_biome(generator, tile), Provenance::Fallback))
    } else {
        (fallback_biome(generator, tile), Provenance::Fallback)
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
        let height = fallback_height(generator, tile);
        (
            height,
            super::quantize_game_height(height, super::compression_fallback()),
            surface::from_heights([height; 4], super::compression_fallback()),
            Provenance::Fallback,
        )
    };
    let (mut water, mut water_provenance) = if let Some(axis) = environment.water_samples_per_axis()
    {
        let coverage = sample_water(generator, axis, tile, cancelled)?;
        coverage
            .map(|(ocean, inland)| water_from_coverage(ocean, inland))
            .unwrap_or((WaterKind::None, Provenance::SourceDerived))
    } else {
        (fallback_water(generator, tile), Provenance::Fallback)
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
        corners[index] =
            sample_bilinear_elevation(generator, x, y, samples, AxisPosition::Corner, cancelled)?;
    }
    Ok((height, corners))
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

/// Raw potential-vegetation class; page failures and cancellation propagate.
pub(super) fn sample_biome_class(
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
