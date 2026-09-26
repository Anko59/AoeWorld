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
    pub const DEFAULT_MAP_PACKAGE_DIRECTORY: &'static str = "local-assets/maps-v8";
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
        if let Some(directory) = configured_map_package_directory(
            env::var_os("AOE_MAP_PACKAGE_DIRECTORY").filter(|value| !value.is_empty()),
        )? {
            config.map_package_directory = Some(directory);
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
            // Generation recipe 5 changes source-elevation sampling identity.
            // Keep prior package directories intact and regenerate compatible
            // packages separately.
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

fn configured_map_package_directory(
    path: Option<std::ffi::OsString>,
) -> Result<Option<PathBuf>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let directory = PathBuf::from(path)
        .canonicalize()
        .map_err(|error| format!("invalid AOE_MAP_PACKAGE_DIRECTORY: {error}"))?;
    if !directory.is_dir() {
        return Err("AOE_MAP_PACKAGE_DIRECTORY must name a directory".to_owned());
    }
    Ok(Some(directory))
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct EnvironmentGuard(Vec<(&'static str, Option<std::ffi::OsString>)>);

    impl EnvironmentGuard {
        fn scoped() -> Self {
            let names = [
                "AOE_BIND",
                "AOE_SCENARIO",
                "AOE_TICK_HZ",
                "AOE_ASSET_PACK",
                "AOE_MAP_WORKER",
                "AOE_MAP_PACKAGE_DIRECTORY",
                "AOE_GEODATA_CACHE",
            ];
            Self(
                names
                    .into_iter()
                    .map(|name| (name, env::var_os(name)))
                    .collect(),
            )
        }

        fn set(&self, name: &str, value: Option<impl AsRef<std::ffi::OsStr>>) {
            unsafe {
                match value {
                    Some(value) => env::set_var(name, value),
                    None => env::remove_var(name),
                }
            }
        }
    }

    impl Drop for EnvironmentGuard {
        fn drop(&mut self) {
            for (name, value) in &self.0 {
                self.set(name, value.as_ref());
            }
        }
    }

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

    #[test]
    fn environment_paths_are_canonicalized_and_reject_non_executable_workers() {
        let _serialized = ENV_LOCK.lock().expect("environment lock");
        let guard = EnvironmentGuard::scoped();
        guard.set("AOE_BIND", None::<&str>);
        guard.set("AOE_SCENARIO", None::<&str>);
        guard.set("AOE_TICK_HZ", None::<&str>);
        guard.set("AOE_ASSET_PACK", None::<&str>);
        guard.set("AOE_MAP_PACKAGE_DIRECTORY", None::<&str>);
        guard.set("AOE_GEODATA_CACHE", None::<&str>);

        let worker = tempfile::tempdir().expect("worker directory");
        guard.set("AOE_MAP_WORKER", Some(worker.path()));
        let error = Config::from_env().expect_err("directory is not an executable");
        assert!(error.contains("regular native executable"), "{error}");

        let worker_file = worker.path().join("worker");
        std::fs::write(&worker_file, b"fixture").expect("worker file");
        guard.set("AOE_MAP_WORKER", Some(&worker_file));
        let config = Config::from_env().expect("configured environment");
        assert_eq!(config.map_worker.as_deref(), Some(worker_file.as_path()));

        guard.set("AOE_MAP_WORKER", None::<&str>);
        guard.set("AOE_MAP_PACKAGE_DIRECTORY", Some(worker.path()));
        let config = Config::from_env().expect("configured map package directory");
        assert_eq!(
            config.map_package_directory.as_deref(),
            Some(
                worker
                    .path()
                    .canonicalize()
                    .expect("canonical package dir")
                    .as_path()
            )
        );

        guard.set("AOE_MAP_PACKAGE_DIRECTORY", None::<&str>);
        guard.set(
            "AOE_ASSET_PACK",
            Some(worker.path().join("missing-pack").as_os_str()),
        );
        let error = Config::from_env().expect_err("missing asset pack");
        assert!(error.contains("invalid AOE_ASSET_PACK"), "{error}");
    }

    #[test]
    fn explicit_map_package_directory_is_canonical_and_must_be_a_directory() {
        let directory = tempfile::tempdir().expect("package directory");
        assert_eq!(
            configured_map_package_directory(Some(directory.path().as_os_str().to_owned()))
                .expect("configured package directory"),
            Some(
                directory
                    .path()
                    .canonicalize()
                    .expect("canonical directory")
            )
        );
        assert_eq!(
            configured_map_package_directory(None).expect("default package directory"),
            None
        );
        let file = directory.path().join("manifest.json");
        std::fs::write(&file, b"{}").expect("file");
        let error = configured_map_package_directory(Some(file.into_os_string()))
            .expect_err("file is not a package directory");
        assert!(error.contains("must name a directory"), "{error}");
    }
}
