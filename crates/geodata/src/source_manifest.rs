use crate::{CacheError, Provider, SourceLock};

impl SourceLock {
    /// Converts a verified cache lock into the immutable, environment-neutral
    /// source manifest embedded in a generated map package.
    pub fn to_map_source_lock(
        &self,
        acquired_at: String,
        preprocessing_version: String,
    ) -> Result<aoe_map::SourceLock, CacheError> {
        self.validate()?;
        if acquired_at.trim().is_empty() || preprocessing_version.trim().is_empty() {
            return Err(CacheError::InvalidLock(
                "acquisition and preprocessing metadata are required",
            ));
        }
        Ok(aoe_map::SourceLock {
            id: self.id.clone(),
            provider: provider_name(self.provider).to_owned(),
            release: self.release.clone(),
            url: self.url.clone(),
            sha256: parse_sha256(&self.sha256)?,
            acquired_at,
            native_resolution: self.native_resolution.clone(),
            crs: self.crs.clone(),
            vertical_datum: self.vertical_datum.clone(),
            license: self.license_reference.clone(),
            preprocessing_version,
        })
    }
}

fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Noaa => "NOAA",
        Provider::Zenodo => "Zenodo",
        Provider::Dans => "DANS",
        Provider::HydroSheds => "HydroSHEDS",
        Provider::NaturalEarth => "Natural Earth",
        Provider::Copernicus => "Copernicus",
        Provider::EsaWorldCover => "ESA WorldCover",
    }
}

fn parse_sha256(value: &str) -> Result<[u8; 32], CacheError> {
    let mut bytes = [0_u8; 32];
    if value.len() != 64 {
        return Err(CacheError::InvalidLock("SHA-256 must be hexadecimal"));
    }
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| CacheError::InvalidLock("SHA-256 must be hexadecimal"))?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_lock_preserves_native_metadata_in_a_map_manifest() {
        let lock = SourceLock {
            id: "etopo-2022".to_owned(),
            provider: Provider::Noaa,
            release: "ETOPO 2022 v1".to_owned(),
            url: "https://www.ngdc.noaa.gov/etopo.tif".to_owned(),
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_owned(),
            bytes: 3,
            native_resolution: "60 arc-seconds".to_owned(),
            crs: "EPSG:4326".to_owned(),
            vertical_datum: "EGM2008".to_owned(),
            license_reference: "public domain".to_owned(),
        };
        let map_lock = lock
            .to_map_source_lock("2026-09-18T00:00:00Z".to_owned(), "gdal-3.6.2".to_owned())
            .expect("manifest");
        assert_eq!(map_lock.provider, "NOAA");
        assert_eq!(map_lock.native_resolution, "60 arc-seconds");
        assert_eq!(map_lock.sha256[0], 0xba);
    }
}
