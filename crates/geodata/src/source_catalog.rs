use crate::Provider;
use serde::Deserialize;
use std::{io::Read, sync::Arc};

const POTENTIAL_BIOME_RECORD_URL: &str = "https://zenodo.org/api/records/3526620";
const BIOME_RASTER: &str = "pnv_biome.type_biome00k_c_250m_s0..0cm_2000..2017_v0.2.tif";
const BIOME_CLASSES: &str = "pnv_biome.type_biome00k_c_250m_s0..0cm_2000..2017_v0.2.tif.csv";

/// Provider-published metadata resolved from a fixed catalog entry. The MD5
/// is checked during acquisition; the resulting source lock records SHA-256.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct KnownSource {
    pub id: String,
    pub provider: Provider,
    pub release: String,
    pub url: String,
    pub bytes: u64,
    pub provider_md5: [u8; 16],
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
                provider_md5: parse_md5(checksum, expected)?,
                native_resolution: "250 meters".to_owned(),
                crs: "EPSG:4326".to_owned(),
                vertical_datum: "not applicable".to_owned(),
                license_reference: "CC BY-SA 4.0".to_owned(),
            })
        })
        .collect()
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
            sources[1].provider_md5,
            [
                0x87, 0x4f, 0x16, 0x9f, 0x96, 0x6e, 0x03, 0x99, 0x35, 0x10, 0x8b, 0xc3, 0x66, 0x77,
                0x3f, 0x80
            ]
        );
    }
}
