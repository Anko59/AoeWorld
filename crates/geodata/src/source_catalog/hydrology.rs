use super::*;
use crate::SourceCache;
use std::{sync::Arc, time::Duration};

const HYDROLAKES_URL: &str =
    "https://data.hydrosheds.org/file/HydroLAKES/HydroLAKES_polys_v10.gdb.zip";
const HYDRORIVERS_EUROPE_URL: &str =
    "https://data.hydrosheds.org/file/HydroRIVERS/HydroRIVERS_v10_eu_shp.zip";
const HYDROLAKES_BYTES: u64 = 762_519_774;
const HYDRORIVERS_EUROPE_BYTES: u64 = 67_648_957;
pub const MAX_HYDROLOGY_DOWNLOAD_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub fn worldcover_sources_for_bounds(
    min_latitude: f64,
    max_latitude: f64,
    min_longitude: f64,
    max_longitude: f64,
) -> Result<Vec<KnownSource>, SourceCatalogError> {
    worldcover_sources_for_bounds_impl(
        min_latitude,
        max_latitude,
        min_longitude,
        max_longitude,
        |_| Ok(None),
    )
}

pub(crate) fn worldcover_sources_for_bounds_cached(
    min_latitude: f64,
    max_latitude: f64,
    min_longitude: f64,
    max_longitude: f64,
    cache: &SourceCache,
) -> Result<Vec<KnownSource>, SourceCatalogError> {
    worldcover_sources_for_bounds_impl(
        min_latitude,
        max_latitude,
        min_longitude,
        max_longitude,
        |id| {
            let Some(lock) = cache
                .known_lock(id)
                .map_err(|error| SourceCatalogError::Cache(error.to_string()))?
            else {
                return Ok(None);
            };
            let Some(name) = id.strip_prefix("worldcover-2021-v200:") else {
                return Ok(None);
            };
            let expected_url = format!("{WORLD_COVER_BASE_URL}/{name}");
            if lock.provider != Provider::EsaWorldCover
                || lock.release != "ESA WorldCover 2021 v200"
                || lock.url != expected_url
                || lock.native_resolution != "10 meters"
                || lock.crs != "EPSG:4326"
                || lock.vertical_datum != "not applicable"
                || lock.license_reference != "CC BY 4.0; ESA WorldCover attribution required"
            {
                return Err(SourceCatalogError::Cache(
                    "cached WorldCover metadata differs from the fixed release".to_owned(),
                ));
            }
            Ok(Some(lock.bytes))
        },
    )
}

fn worldcover_sources_for_bounds_impl(
    min_latitude: f64,
    max_latitude: f64,
    min_longitude: f64,
    max_longitude: f64,
    mut cached_bytes: impl FnMut(&str) -> Result<Option<u64>, SourceCatalogError>,
) -> Result<Vec<KnownSource>, SourceCatalogError> {
    let tile_ids = worldcover_tile_ids(min_latitude, max_latitude, min_longitude, max_longitude)?;
    if tile_ids.len() > MAX_WORLDCOVER_TILES {
        return Err(SourceCatalogError::TooManyWorldCoverTiles);
    }
    let mut sources = Vec::new();
    for (latitude, longitude) in tile_ids {
        let name = format!(
            "ESA_WorldCover_10m_2021_v200_{}{}_Map.tif",
            latitude_tag(latitude),
            longitude_tag(longitude)
        );
        let url = format!("{WORLD_COVER_BASE_URL}/{name}");
        let id = format!("worldcover-2021-v200:{name}");
        let bytes = match cached_bytes(&id)? {
            Some(bytes) => bytes,
            None => head_bytes(&url)?,
        };
        sources.push(KnownSource {
            id,
            provider: Provider::EsaWorldCover,
            release: "ESA WorldCover 2021 v200".to_owned(),
            url,
            bytes,
            expected_checksum: ExpectedChecksum::Sha256OnFirstAcquisition,
            native_resolution: "10 meters".to_owned(),
            crs: "EPSG:4326".to_owned(),
            vertical_datum: "not applicable".to_owned(),
            license_reference: "CC BY 4.0; ESA WorldCover attribution required".to_owned(),
        });
    }
    Ok(sources)
}

/// Pure tile selection used by tests and by callers that have already
/// resolved source metadata. Bounds are closed to avoid losing an exact tile
/// boundary sample.
pub fn worldcover_tile_ids(
    min_latitude: f64,
    max_latitude: f64,
    min_longitude: f64,
    max_longitude: f64,
) -> Result<Vec<(i32, i32)>, SourceCatalogError> {
    if ![min_latitude, max_latitude, min_longitude, max_longitude]
        .iter()
        .all(|value| value.is_finite())
        || min_latitude < -90.0
        || max_latitude > 82.75
        || min_latitude >= max_latitude
        || min_longitude < -180.0
        || max_longitude > 180.0
        || min_longitude >= max_longitude
    {
        return Err(SourceCatalogError::InvalidBounds);
    }
    let min_latitude = (min_latitude / 3.0).floor() as i32 * 3;
    let max_latitude = (max_latitude / 3.0).floor() as i32 * 3;
    let min_longitude = (min_longitude / 3.0).floor() as i32 * 3;
    let max_longitude = (max_longitude / 3.0).floor() as i32 * 3;
    let latitude_count = (max_latitude.min(81) - min_latitude).div_euclid(3) + 1;
    let longitude_count = (max_longitude.min(177) - min_longitude).div_euclid(3) + 1;
    if latitude_count <= 0
        || longitude_count <= 0
        || i64::from(latitude_count) * i64::from(longitude_count) > MAX_WORLDCOVER_TILES as i64
    {
        return Err(SourceCatalogError::TooManyWorldCoverTiles);
    }
    let mut tiles = Vec::new();
    for latitude in (min_latitude..=max_latitude.min(81)).step_by(3) {
        for longitude in (min_longitude..=max_longitude.min(177)).step_by(3) {
            tiles.push((latitude, longitude));
        }
    }
    if tiles.is_empty() {
        return Err(SourceCatalogError::InvalidBounds);
    }
    Ok(tiles)
}

/// The official HydroSHEDS regional vector packages. The package URLs are
/// fixed v1.0 release packages. The reviewed SHA-256 values pin these source
/// objects independently of the provider's download endpoint.
pub fn hydrology_vector_sources() -> Vec<KnownSource> {
    vec![
        KnownSource {
            id: "hydrolakes-v1.0-global-gdb".to_owned(),
            provider: Provider::HydroSheds,
            release: "HydroLAKES v1.0".to_owned(),
            url: HYDROLAKES_URL.to_owned(),
            bytes: HYDROLAKES_BYTES,
            expected_checksum: ExpectedChecksum::Sha256([
                0x1c, 0x13, 0x03, 0xa4, 0x88, 0x2c, 0x59, 0x7b, 0x76, 0x9f, 0x4a, 0x2b, 0xea, 0xe6,
                0xc7, 0x28, 0x04, 0xc5, 0x2a, 0xd4, 0x18, 0xa0, 0xb4, 0x07, 0x88, 0x17, 0xcf, 0x10,
                0x62, 0x11, 0x66, 0x43,
            ]),
            native_resolution: "global lakes >= 10 ha".to_owned(),
            crs: "EPSG:4326".to_owned(),
            vertical_datum: "not applicable".to_owned(),
            license_reference: "HydroSHEDS license; CC BY 4.0 product attribution".to_owned(),
        },
        KnownSource {
            id: "hydrorivers-v1.0-eu-shp".to_owned(),
            provider: Provider::HydroSheds,
            release: "HydroRIVERS v1.0 Europe and Middle East".to_owned(),
            url: HYDRORIVERS_EUROPE_URL.to_owned(),
            bytes: HYDRORIVERS_EUROPE_BYTES,
            expected_checksum: ExpectedChecksum::Sha256([
                0x50, 0x0d, 0xa7, 0xd3, 0x6c, 0xee, 0xe0, 0xaa, 0x4c, 0x82, 0xdd, 0x62, 0x5f, 0x5c,
                0x53, 0xd8, 0x0b, 0xa2, 0xf7, 0xac, 0x14, 0xad, 0x02, 0x2c, 0xd3, 0xa6, 0x60, 0x1b,
                0xf5, 0x50, 0x68, 0xe5,
            ]),
            native_resolution: "15 arc-seconds; regional Europe/Middle East".to_owned(),
            crs: "EPSG:4326".to_owned(),
            vertical_datum: "not applicable".to_owned(),
            license_reference: "HydroSHEDS license for scientific, educational, and commercial use"
                .to_owned(),
        },
    ]
}

fn latitude_tag(latitude: i32) -> String {
    if latitude < 0 {
        format!("S{:02}", latitude.unsigned_abs())
    } else {
        format!("N{latitude:02}")
    }
}

fn longitude_tag(longitude: i32) -> String {
    if longitude < 0 {
        format!("W{:03}", longitude.unsigned_abs())
    } else {
        format!("E{longitude:03}")
    }
}

fn head_bytes(url: &str) -> Result<u64, SourceCatalogError> {
    let connector = ureq::native_tls::TlsConnector::new()
        .map_err(|error| SourceCatalogError::Request(error.to_string()))?;
    let agent = ureq::AgentBuilder::new()
        .tls_connector(Arc::new(connector))
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(15))
        .timeout_write(Duration::from_secs(15))
        .build();
    let response = agent
        .head(url)
        .call()
        .map_err(|error| SourceCatalogError::Request(error.to_string()))?;
    response
        .header("Content-Length")
        .ok_or(SourceCatalogError::InvalidSourceSize)
        .and_then(|bytes| {
            bytes
                .parse()
                .map_err(|_| SourceCatalogError::InvalidSourceSize)
        })
}
