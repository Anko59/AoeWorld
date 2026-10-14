use super::elevation::{AxisPosition, bilinear_height, source_axis_position};
use super::fallback::{fallback_biome, fallback_height};
use super::{GroundMaterial, MapChunkGenerator, Provenance, Tile, WaterKind, surface};
use crate::{
    ENVIRONMENT_PAGE_SAMPLES, EnvironmentPage, EnvironmentPageError, EnvironmentPageKey, PageLayer,
    biome_rules::material_for,
};
use aoe_core::TileCoord;

mod helpers;
mod history;
pub(super) mod rivers;
pub(super) mod water;
mod water_model;

pub(super) use history::sample_land_use_observation;

use helpers::{elevation_value, load_page, page_index, source_coordinate};

/// Undecorated source terrain; preserves page failures and caller cancellation.
pub(super) fn sample_base_tile(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<Tile, EnvironmentPageError> {
    sample_base_tile_with_cliff(generator, tile, cancelled, &|position| {
        cliff_from_corners(generator, position, cancelled)
    })
}

/// Batch callers share the whole-cell decision without changing tile sampling.
pub(super) fn sample_base_tile_with_cliff(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    cancelled: &dyn Fn() -> bool,
    cliff: &dyn Fn(TileCoord) -> Result<Option<super::relief::CliffPiece>, EnvironmentPageError>,
) -> Result<Tile, EnvironmentPageError> {
    let environment = generator
        .provider_environment
        .as_ref()
        .ok_or(EnvironmentPageError::Invalid)?;
    let compression = generator
        .provider_compression
        .ok_or(EnvironmentPageError::Invalid)?;
    let biome = sample_biome_with_provenance(generator, tile, cancelled)?;
    let mut source_corners = None;
    let (
        geographic_height_centimeters,
        mut game_height_level,
        mut surface_kind,
        elevation_provenance,
    ) = if environment.samples_per_axis > 0 {
        let (height, corners) =
            sample_elevation(generator, environment.samples_per_axis, tile, cancelled)?;
        source_corners = Some(corners);
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
    let sample = water::sample(generator, environment, tile, true, cancelled)?;
    let (water, water_provenance) = water::resolve(
        &sample,
        compression,
        &mut game_height_level,
        &mut surface_kind,
    );
    let (hydrology_observation, modern_land_cover_class) =
        (sample.observation, sample.modern_land_cover_class);
    if water == WaterKind::None
        && source_corners.is_some()
        && let Some(piece) = cliff(tile)?
    {
        surface_kind.kind = surface::SurfaceKind::CliffLedge;
        surface_kind.corner_game_height_levels = [piece.top_level; 4];
        game_height_level = piece.top_level;
    }
    let ford = sample.ford && surface_kind.walkable();
    let material = if ford {
        GroundMaterial::Ford
    } else {
        match water {
            WaterKind::None => material_for(biome.0, geographic_height_centimeters),
            WaterKind::River | WaterKind::Lake | WaterKind::Ocean => GroundMaterial::Water,
            WaterKind::Shallow => GroundMaterial::Shore,
        }
    };
    generator
        .river_queries
        .lock()
        .map_err(|_| EnvironmentPageError::Unavailable)?
        .insert_water(tile, water);
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
        passable: (water == WaterKind::None || ford)
            && material != GroundMaterial::Ice
            && surface_kind.walkable(),
    })
}

fn cliff_from_corners(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<super::relief::CliffPiece>, EnvironmentPageError> {
    let samples = generator
        .provider_environment
        .as_ref()
        .ok_or(EnvironmentPageError::Invalid)?
        .samples_per_axis;
    // A one-sample field is constant across every cell; no corner reads.
    if samples <= 1 {
        return Ok(None);
    }
    let compression = generator
        .provider_compression
        .ok_or(EnvironmentPageError::Invalid)?;
    let piece = super::relief::cliff_at(generator, tile, compression, &|x, y| {
        sample_bilinear_elevation(generator, x, y, samples, AxisPosition::Corner, cancelled)
    })?;
    if piece.is_some() && !generator.cliff_cell_is_available(tile, cancelled)? {
        return Ok(None);
    }
    Ok(piece)
}

/// Cliff piece of a tile for the composer; none without an elevation field.
pub(super) fn cliff_piece(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<super::relief::CliffPiece>, EnvironmentPageError> {
    let samples = generator
        .provider_environment
        .as_ref()
        .ok_or(EnvironmentPageError::Invalid)?
        .samples_per_axis;
    if samples == 0 {
        return Ok(None);
    }
    cliff_from_corners(generator, tile, cancelled)
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

/// Base biome with nodata samples filled from the nearest usable class.
fn sample_biome_with_provenance(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<(super::Biome, Provenance), EnvironmentPageError> {
    let axis = generator
        .provider_environment
        .as_ref()
        .ok_or(EnvironmentPageError::Invalid)?
        .vegetation_samples_per_axis();
    match axis {
        Some(axis) => super::vegetation::filled_biome(generator, tile, axis, |position| {
            sample_biome_class(generator, axis, position, cancelled)
        }),
        None => Ok((fallback_biome(generator, tile), Provenance::Fallback)),
    }
}

pub(super) fn sample_biome(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<super::Biome, EnvironmentPageError> {
    sample_biome_with_provenance(generator, tile, cancelled).map(|biome| biome.0)
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
