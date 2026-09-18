use crate::Provider;
use serde::Deserialize;
use std::{io::Read, sync::Arc};

const POTENTIAL_BIOME_RECORD_URL: &str = "https://zenodo.org/api/records/3526620";
const BIOME_RASTER: &str = "pnv_biome.type_biome00k_c_250m_s0..0cm_2000..2017_v0.2.tif";
const BIOME_CLASSES: &str = "pnv_biome.type_biome00k_c_250m_s0..0cm_2000..2017_v0.2.tif.csv";
const ETOPO_60S_SURFACE_URL: &str = "https://www.ngdc.noaa.gov/mgg/global/relief/ETOPO2022/data/60s/60s_surface_elev_gtif/ETOPO_2022_v1_60s_N90W180_surface.tif";

/// A checksum resolved from provider metadata or pinned after a verified,
/// explicitly reviewed acquisition when the provider publishes no digest.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "algorithm", content = "digest")]
pub enum ExpectedChecksum {
    Md5([u8; 16]),
    Sha256([u8; 32]),
}

impl ExpectedChecksum {
    pub(crate) fn matches(self, sha256: &[u8; 32], md5: &[u8; 16]) -> bool {
        match self {
            Self::Md5(expected) => expected == *md5,
            Self::Sha256(expected) => expected == *sha256,
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_requires_the_pinned_raster_and_class_lookup() {
        let sources = parse_potential_biome_sources(
            r#"{"files":[
                {"key":"pnv_biome.type_biome00k_c_250m_s0..0cm_2000..2017_v0.2.tif","size":210668848,"checksum":"md5:e67c4778153fe5dcd9c637f4846e2f03","links":{"self":"https://zenodo.org/file.tif"}},
                {"key":"pnv_biome.type_biome00k_c_250m_s0..0cm_2000..2017_v0.2.tif.csv","size":2968,"checksum":"md5:874f169f966e039935108bc366773f80","links":{"self":"https://zenodo.org/file.csv"}}
            ]}"#,
        )
        .expect("catalog");
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].bytes, 210_668_848);
        assert_eq!(
            sources[1].expected_checksum,
            ExpectedChecksum::Md5([
                0x87, 0x4f, 0x16, 0x9f, 0x96, 0x6e, 0x03, 0x99, 0x35, 0x10, 0x8b, 0xc3, 0x66, 0x77,
                0x3f, 0x80
            ])
        );
    }

    #[test]
    fn overview_source_has_a_reviewed_sha256_checksum() {
        let source = etopo_2022_60s_surface();
        assert_eq!(source.bytes, 465_969_062);
        assert!(matches!(
            source.expected_checksum,
            ExpectedChecksum::Sha256([0x9d, 0x27, 0xd4, 0xb8, ..])
        ));
    }
}
