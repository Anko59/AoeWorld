//! Boot activation shares the controller's stored-package operation.
use super::activate_completed;
use crate::{AppState, Config};
use std::sync::Arc;

impl AppState {
    pub async fn at_boot(config: &Config, build: impl Into<Arc<str>>) -> Result<Self, String> {
        let error = |reason: &str| {
            match &config.open_map_package {
                Some(hash) => format!(
                    "AOE_OPEN_MAP_PACKAGE {hash:?} in {}: {reason}",
                    config
                        .map_package_directory
                        .as_deref()
                        .unwrap_or_else(|| std::path::Path::new(
                            Config::DEFAULT_MAP_PACKAGE_DIRECTORY
                        ))
                        .display(),
                ),
                None => reason.to_owned(),
            }
            .replace(['\r', '\n'], " ")
        };
        if let Some(hash) = &config.open_map_package
            && (hash.len() != 64
                || !hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
        {
            return Err(error("invalid content hash"));
        }
        let state = Self::new(config, build).map_err(|reason| error(&reason.to_string()))?;
        if let Some(hash) = &config.open_map_package {
            let package = state
                .map_packages
                .read()
                .await
                .get(hash)
                .cloned()
                .ok_or_else(|| error("unknown stored map package"))?;
            let activation = activate_completed(state.clone(), package)
                .await
                .map_err(|(_, reason)| error(&reason))?
                .0;
            if !activation.start_available {
                return Err(error("no suitable land start"));
            }
            tracing::info!(content_hash = %hash, "opened map package at boot");
        }
        Ok(state)
    }
}
