use crate::Provider;
use serde::Deserialize;
use std::{io::Read, sync::Arc};

const POTENTIAL_BIOME_RECORD_URL: &str = "https://zenodo.org/api/records/3526620";
const BIOME_RASTER: &str = "pnv_biome.type_biome00k_c_250m_s0..0cm_2000..2017_v0.2.tif";
const BIOME_CLASSES: &str = "pnv_biome.type_biome00k_c_250m_s0..0cm_2000..2017_v0.2.tif.csv";
const ETOPO_60S_SURFACE_URL: &str = "https://www.ngdc.noaa.gov/mgg/global/relief/ETOPO2022/data/60s/60s_surface_elev_gtif/ETOPO_2022_v1_60s_N90W180_surface.tif";
const NATURAL_EARTH_10M_LAND_URL: &str =
    "https://naciscdn.org/naturalearth/10m/physical/ne_10m_land.zip";
const HYDE_DATASET_URL: &str = "https://archaeology.datastations.nl/api/datasets/:persistentId/?persistentId=doi:10.17026/DANS-25G-GEZ3";
const HYDE_BASELINE: &str = "HYDE3_2_1-baseline.zip";
const HYDE_SUPPLEMENTARY: &str = "HYDE3_2_1-general_supplementary.zip";
const HYDE_README: &str = "readme_release_HYDE3.2.1.txt";
pub(crate) const WORLD_COVER_BASE_URL: &str =
    "https://esa-worldcover.s3.eu-central-1.amazonaws.com/v200/2021/map";
pub const MAX_WORLDCOVER_TILES: usize = 32;

/// A provider-verified digest, or an explicit trust-on-first-use policy for
/// immutable fixed release URLs where the publisher exposes no digest.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "algorithm", content = "digest")]
pub enum ExpectedChecksum {
    /// The fixed allowlisted HTTPS release has no published cryptographic
    /// digest. This is not a checksum match: its first exact-size transfer is
    /// pinned to SHA-256 in the content address and known-source lock, which
    /// is re-hashed before every later use and carried in package provenance.
    Sha256OnFirstAcquisition,
    Md5([u8; 16]),
    Sha1([u8; 20]),
    Sha256([u8; 32]),
}

impl ExpectedChecksum {
    pub(crate) fn matches(self, sha256: &[u8; 32], sha1: &[u8; 20], md5: &[u8; 16]) -> bool {
        match self {
            Self::Sha256OnFirstAcquisition => false,
            Self::Md5(expected) => expected == *md5,
            Self::Sha1(expected) => expected == *sha1,
            Self::Sha256(expected) => expected == *sha256,
        }
    }
}

/// Returns current HYDE release sources after resolving the provider catalog.
pub fn hyde_sources() -> Result<Vec<KnownSource>, SourceCatalogError> {
    let connector = ureq::native_tls::TlsConnector::new()
        .map_err(|error| SourceCatalogError::Request(error.to_string()))?;
    let agent = ureq::AgentBuilder::new()
        .tls_connector(Arc::new(connector))
        .build();
    let response = agent
        .get(HYDE_DATASET_URL)
        .call()
        .map_err(|error| SourceCatalogError::Request(error.to_string()))?;
    let mut payload = String::new();
    response
        .into_reader()
        .read_to_string(&mut payload)
        .map_err(SourceCatalogError::Io)?;
    parse_hyde_sources(&payload)
}

fn parse_hyde_sources(payload: &str) -> Result<Vec<KnownSource>, SourceCatalogError> {
    let dataset = serde_json::from_str::<DataverseDataset>(payload)
        .map_err(|error| SourceCatalogError::Metadata(error.to_string()))?;
    [HYDE_BASELINE, HYDE_SUPPLEMENTARY, HYDE_README]
        .into_iter()
        .map(|expected| {
            let file = dataset
                .data
                .latest_version
                .files
                .iter()
                .find(|file| file.data_file.filename == expected)
                .ok_or(SourceCatalogError::MissingRequiredFile(expected))?;
            let checksum = file
                .data_file
                .checksum
                .as_ref()
                .ok_or(SourceCatalogError::UnexpectedChecksum(expected))?;
            if checksum.kind != "SHA-1" {
                return Err(SourceCatalogError::UnexpectedChecksum(expected));
            }
            Ok(KnownSource {
                id: format!("hyde-3.2.1:{expected}"),
                provider: Provider::Dans,
                release: "HYDE 3.2.1, DANS-25G-GEZ3".to_owned(),
                url: format!(
                    "https://archaeology.datastations.nl/api/access/datafile/{}?format=original",
                    file.data_file.id
                ),
                bytes: file.data_file.filesize,
                expected_checksum: ExpectedChecksum::Sha1(parse_sha1(&checksum.value, expected)?),
                native_resolution: "5 arc-minutes".to_owned(),
                crs: "EPSG:4326".to_owned(),
                vertical_datum: "not applicable".to_owned(),
                license_reference: "dataset metadata CC0; release README CC BY 3.0".to_owned(),
            })
        })
        .collect()
}

impl KnownSource {
    /// Produces a directly reusable cache lock when the catalog has a
    /// SHA-256, so a verified object can be reused without another download.
    pub fn cache_lock(&self) -> Option<crate::SourceLock> {
        let ExpectedChecksum::Sha256(sha256) = self.expected_checksum else {
            return None;
        };
        Some(crate::SourceLock {
            id: self.id.clone(),
            provider: self.provider,
            release: self.release.clone(),
            url: self.url.clone(),
            sha256: sha256.iter().map(|byte| format!("{byte:02x}")).collect(),
            bytes: self.bytes,
            native_resolution: self.native_resolution.clone(),
            crs: self.crs.clone(),
            vertical_datum: self.vertical_datum.clone(),
            license_reference: self.license_reference.clone(),
        })
    }
}

/// Allowlisted metadata resolved from a fixed catalog entry. Its checksum is
/// checked during acquisition; the resulting source lock records SHA-256.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct KnownSource {
    pub id: String,
    pub provider: Provider,
    pub release: String,
    pub url: String,
    pub bytes: u64,
    pub expected_checksum: ExpectedChecksum,
    pub native_resolution: String,
    pub crs: String,
    pub vertical_datum: String,
    pub license_reference: String,
}

impl KnownSource {
    pub(crate) fn has_valid_acquisition_policy(&self) -> bool {
        match self.expected_checksum {
            ExpectedChecksum::Sha256OnFirstAcquisition => {
                let Some(name) = self.id.strip_prefix("worldcover-2021-v200:") else {
                    return false;
                };
                self.provider == crate::Provider::EsaWorldCover
                    && self.release == "ESA WorldCover 2021 v200"
                    && self.url == format!("{WORLD_COVER_BASE_URL}/{name}")
                    && name.starts_with("ESA_WorldCover_10m_2021_v200_")
                    && name.ends_with("_Map.tif")
                    && !name.contains('/')
                    && !name.contains('\\')
            }
            _ => true,
        }
    }

    pub(crate) fn accepts_acquired_bytes(
        &self,
        sha256: &[u8; 32],
        sha1: &[u8; 20],
        md5: &[u8; 16],
    ) -> bool {
        if !self.has_valid_acquisition_policy() {
            return false;
        }
        match self.expected_checksum {
            ExpectedChecksum::Sha256OnFirstAcquisition => true,
            checksum => checksum.matches(sha256, sha1, md5),
        }
    }
}

pub fn potential_biome_sources() -> Result<Vec<KnownSource>, SourceCatalogError> {
    let connector = ureq::native_tls::TlsConnector::new()
        .map_err(|error| SourceCatalogError::Request(error.to_string()))?;
    let agent = ureq::AgentBuilder::new()
        .tls_connector(Arc::new(connector))
        .build();
    let response = agent
        .get(POTENTIAL_BIOME_RECORD_URL)
        .call()
        .map_err(|error| SourceCatalogError::Request(error.to_string()))?;
    let mut payload = String::new();
    response
        .into_reader()
        .read_to_string(&mut payload)
        .map_err(SourceCatalogError::Io)?;
    parse_potential_biome_sources(&payload)
}

fn parse_potential_biome_sources(payload: &str) -> Result<Vec<KnownSource>, SourceCatalogError> {
    let record = serde_json::from_str::<ZenodoRecord>(payload)
        .map_err(|error| SourceCatalogError::Metadata(error.to_string()))?;
    [BIOME_RASTER, BIOME_CLASSES]
        .into_iter()
        .map(|expected| {
            let file = record
                .files
                .iter()
                .find(|file| file.key == expected)
                .ok_or(SourceCatalogError::MissingRequiredFile(expected))?;
            let checksum = file
                .checksum
                .strip_prefix("md5:")
                .ok_or(SourceCatalogError::UnexpectedChecksum(expected))?;
            Ok(KnownSource {
                id: format!("potential-biome-v0.2:{expected}"),
                provider: Provider::Zenodo,
                release: "Zenodo record 3526620, potential biome v0.2".to_owned(),
                url: file.links.content.clone(),
                bytes: file.size,
                expected_checksum: ExpectedChecksum::Md5(parse_md5(checksum, expected)?),
                native_resolution: "250 meters".to_owned(),
                crs: "EPSG:4326".to_owned(),
                vertical_datum: "not applicable".to_owned(),
                license_reference: "CC BY-SA 4.0".to_owned(),
            })
        })
        .collect()
}

/// Pinned global overview source. NOAA's public directory exposes no checksum;
/// this SHA-256 was acquired from that fixed HTTPS URL and is checked before
/// every subsequent use.
pub fn etopo_2022_60s_surface() -> KnownSource {
    KnownSource {
        id: "etopo-2022-v1-60s-surface".to_owned(),
        provider: Provider::Noaa,
        release: "ETOPO 2022 v1".to_owned(),
        url: ETOPO_60S_SURFACE_URL.to_owned(),
        bytes: 465_969_062,
        expected_checksum: ExpectedChecksum::Sha256([
            0x9d, 0x27, 0xd4, 0xb8, 0xea, 0x8e, 0x76, 0x97, 0x7e, 0x29, 0x88, 0xbc, 0xa6, 0x67,
            0xd7, 0xc8, 0xfa, 0x68, 0xb9, 0x27, 0x35, 0x5f, 0xef, 0xfc, 0xdd, 0xd6, 0xb4, 0x87,
            0x5a, 0x7f, 0xd0, 0x8e,
        ]),
        native_resolution: "60 arc-seconds".to_owned(),
        crs: "EPSG:4326".to_owned(),
        vertical_datum: "EGM2008 orthometric".to_owned(),
        license_reference: "NOAA public domain".to_owned(),
    }
}

/// Pinned coarse land polygons for source-derived ocean coverage. This is a
/// coastline fallback only; it cannot classify lakes or rivers.
pub fn natural_earth_10m_land() -> KnownSource {
    KnownSource {
        id: "natural-earth-10m-land-v5.1.1".to_owned(),
        provider: Provider::NaturalEarth,
        release: "Natural Earth 5.1.1, 1:10m land".to_owned(),
        url: NATURAL_EARTH_10M_LAND_URL.to_owned(),
        bytes: 3_269_070,
        expected_checksum: ExpectedChecksum::Sha256([
            0xe5, 0x47, 0xd7, 0x49, 0x44, 0x5e, 0xaa, 0x09, 0x64, 0xab, 0xa7, 0x67, 0x38, 0x09,
            0x0e, 0xc8, 0x8f, 0x5e, 0x63, 0xc4, 0x58, 0x51, 0x22, 0x17, 0x0f, 0x98, 0xc6, 0x7a,
            0x7e, 0xa9, 0x22, 0xdc,
        ]),
        native_resolution: "1:10,000,000 physical vector scale".to_owned(),
        crs: "EPSG:4326".to_owned(),
        vertical_datum: "not applicable".to_owned(),
        license_reference: "Natural Earth public-domain terms".to_owned(),
    }
}

fn parse_md5(value: &str, name: &'static str) -> Result<[u8; 16], SourceCatalogError> {
    if value.len() != 32 {
        return Err(SourceCatalogError::UnexpectedChecksum(name));
    }
    let mut output = [0_u8; 16];
    for (index, slot) in output.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| SourceCatalogError::UnexpectedChecksum(name))?;
    }
    Ok(output)
}

fn parse_sha1(value: &str, name: &'static str) -> Result<[u8; 20], SourceCatalogError> {
    if value.len() != 40 {
        return Err(SourceCatalogError::UnexpectedChecksum(name));
    }
    let mut output = [0_u8; 20];
    for (index, slot) in output.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| SourceCatalogError::UnexpectedChecksum(name))?;
    }
    Ok(output)
}

#[derive(Deserialize)]
struct ZenodoRecord {
    files: Vec<ZenodoFile>,
}

#[derive(Deserialize)]
struct ZenodoFile {
    key: String,
    size: u64,
    checksum: String,
    links: ZenodoLinks,
}

#[derive(Deserialize)]
struct ZenodoLinks {
    #[serde(rename = "self")]
    content: String,
}

#[derive(Deserialize)]
struct DataverseDataset {
    data: DataverseData,
}
#[derive(Deserialize)]
struct DataverseData {
    #[serde(rename = "latestVersion")]
    latest_version: DataverseVersion,
}
#[derive(Deserialize)]
struct DataverseVersion {
    files: Vec<DataverseFile>,
}
#[derive(Deserialize)]
struct DataverseFile {
    #[serde(rename = "dataFile")]
    data_file: DataverseDataFile,
}
#[derive(Deserialize)]
struct DataverseDataFile {
    id: u64,
    filename: String,
    filesize: u64,
    checksum: Option<DataverseChecksum>,
}
#[derive(Deserialize)]
struct DataverseChecksum {
    #[serde(rename = "type")]
    kind: String,
    value: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SourceCatalogError {
    #[error("provider metadata request failed: {0}")]
    Request(String),
    #[error("provider metadata read failed: {0}")]
    Io(#[source] std::io::Error),
    #[error("provider metadata was invalid: {0}")]
    Metadata(String),
    #[error("provider metadata did not contain required file {0}")]
    MissingRequiredFile(&'static str),
    #[error("provider metadata had an invalid checksum for {0}")]
    UnexpectedChecksum(&'static str),
    #[error("source bounds are outside WorldCover coverage")]
    InvalidBounds,
    #[error("provider source did not publish a valid Content-Length")]
    InvalidSourceSize,
    #[error("WorldCover request intersects more than the bounded tile limit")]
    TooManyWorldCoverTiles,
    #[error("cached source metadata is invalid: {0}")]
    Cache(String),
}

#[path = "source_catalog/hydrology.rs"]
mod hydrology_catalog;
pub(crate) use hydrology_catalog::worldcover_sources_for_bounds_cached;
pub use hydrology_catalog::{
    MAX_HYDROLOGY_DOWNLOAD_BYTES, hydrology_vector_sources, worldcover_sources_for_bounds,
    worldcover_tile_ids,
};

#[cfg(test)]
#[path = "source_catalog/tests/catalog.rs"]
mod tests;
