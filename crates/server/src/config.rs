use aoe_scenario::{Scenario, named};
use std::{env, net::SocketAddr, path::PathBuf};

#[derive(Debug)]
pub struct Config {
    pub bind: SocketAddr,
    pub scenario: Scenario,
    pub tick_hz: u32,
    pub asset_pack: Option<PathBuf>,
    pub map_package_directory: Option<PathBuf>,
    pub map_worker: Option<PathBuf>,
    pub geodata_cache_directory: PathBuf,
}

impl Config {
    pub const DEFAULT_MAP_PACKAGE_DIRECTORY: &'static str = "local-assets/maps-v7";
    pub const DEFAULT_GEODATA_CACHE_DIRECTORY: &'static str = ".cache/geodata";

    pub fn from_env() -> Result<Self, String> {
        let bind = env::var("AOE_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
        let name = env::var("AOE_SCENARIO").unwrap_or_else(|_| "smoke".to_owned());
        let tick_hz = env::var("AOE_TICK_HZ").unwrap_or_else(|_| "20".to_owned());
        let mut config = Self::parse(&bind, &name, &tick_hz)?;
        if let Some(path) = env::var_os("AOE_ASSET_PACK").filter(|value| !value.is_empty()) {
            let root = PathBuf::from("local-assets/packs")
                .canonicalize()
                .map_err(|e| format!("invalid AOE_ASSET_PACK root: {e}"))?;
            let pack = PathBuf::from(path)
                .canonicalize()
                .map_err(|e| format!("invalid AOE_ASSET_PACK: {e}"))?;
            if pack.parent() != Some(root.as_path()) || !pack.join("manifest.json").is_file() {
                return Err(
                    "AOE_ASSET_PACK must name a local-assets/packs child with a manifest"
                        .to_owned(),
                );
            }
            config.asset_pack = Some(pack);
        }
        if let Some(path) = env::var_os("AOE_MAP_WORKER").filter(|value| !value.is_empty()) {
            let worker = PathBuf::from(path)
                .canonicalize()
                .map_err(|e| format!("invalid AOE_MAP_WORKER: {e}"))?;
            let metadata = worker
                .metadata()
                .map_err(|e| format!("invalid AOE_MAP_WORKER: {e}"))?;
            if !metadata.is_file() {
                return Err("AOE_MAP_WORKER must name a regular native executable".to_owned());
            }
            config.map_worker = Some(worker);
        }
        if let Some(cache) = configured_geodata_cache(env::var_os("AOE_GEODATA_CACHE"))? {
            config.geodata_cache_directory = cache;
        }
        Ok(config)
    }

    fn parse(bind: &str, name: &str, tick_hz: &str) -> Result<Self, String> {
        let bind = bind.parse().map_err(|e| format!("invalid AOE_BIND: {e}"))?;
        let scenario = named(name).ok_or_else(|| format!("unknown AOE_SCENARIO: {name}"))?;
        let tick_hz: u32 = tick_hz
            .parse()
            .map_err(|e| format!("invalid AOE_TICK_HZ: {e}"))?;
        if !(1..=60).contains(&tick_hz) {
            return Err("AOE_TICK_HZ must be 1..=60".to_owned());
        }
        Ok(Self {
            bind,
            scenario,
            tick_hz,
            asset_pack: None,
            // Generation recipe 3 changes immutable package identity. Keep maps-v6
            // intact and regenerate compatible packages in the new directory.
            map_package_directory: Some(PathBuf::from(Self::DEFAULT_MAP_PACKAGE_DIRECTORY)),
            map_worker: None,
            geodata_cache_directory: PathBuf::from(Self::DEFAULT_GEODATA_CACHE_DIRECTORY),
        })
    }
}

fn configured_geodata_cache(path: Option<std::ffi::OsString>) -> Result<Option<PathBuf>, String> {
    let Some(path) = path.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let cache = PathBuf::from(path)
        .canonicalize()
        .map_err(|e| format!("invalid AOE_GEODATA_CACHE: {e}"))?;
    if !cache.is_dir() {
        return Err("AOE_GEODATA_CACHE must name a directory".to_owned());
    }
    Ok(Some(cache))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_and_rejects_invalid_configuration() {
        let config = Config::parse("127.0.0.1:0", "target-hotspot", "20").expect("valid config");
        assert_eq!(config.tick_hz, 20);
        assert_eq!(config.scenario.name, "target-hotspot");
        assert_eq!(
            config.map_package_directory,
            Some(PathBuf::from(Config::DEFAULT_MAP_PACKAGE_DIRECTORY))
        );
        assert_eq!(
            config.geodata_cache_directory,
            PathBuf::from(Config::DEFAULT_GEODATA_CACHE_DIRECTORY)
        );
        assert!(config.map_worker.is_none());
        assert!(Config::parse("bad", "smoke", "20").is_err());
        assert!(Config::parse("127.0.0.1:0", "missing", "20").is_err());
        assert!(Config::parse("127.0.0.1:0", "smoke", "bad").is_err());
        assert!(Config::parse("127.0.0.1:0", "smoke", "0").is_err());
        assert!(Config::parse("127.0.0.1:0", "smoke", "61").is_err());
    }

    #[test]
    fn explicit_geodata_cache_is_canonical_and_must_be_a_directory() {
        let cache = tempfile::tempdir().expect("cache");
        assert_eq!(
            configured_geodata_cache(Some(cache.path().as_os_str().to_owned()))
                .expect("configured cache"),
            Some(cache.path().canonicalize().expect("canonical cache"))
        );
        assert_eq!(configured_geodata_cache(None).expect("default"), None);
        let file = cache.path().join("file");
        std::fs::write(&file, "not a directory").expect("file");
        assert!(configured_geodata_cache(Some(file.into_os_string())).is_err());
    }
}
