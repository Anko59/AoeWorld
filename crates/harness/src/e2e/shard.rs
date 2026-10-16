//! One Playwright shard of the browser suite: `AOE_E2E_SHARD=i/n` (#388).
use std::fmt;

/// The most shards one run may be split into; CI uses three.
const MAX_SHARDS: u32 = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Shard {
    pub(crate) index: u32,
    pub(crate) count: u32,
}

impl fmt::Display for Shard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.index, self.count)
    }
}

impl Shard {
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let invalid =
            || format!("AOE_E2E_SHARD must be i/n with 1 <= i <= n <= {MAX_SHARDS}, got {text:?}");
        let (index, count) = text.split_once('/').ok_or_else(invalid)?;
        let digits = |part: &str| {
            (!part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| part.parse::<u32>().ok())
                .flatten()
        };
        let (index, count) = digits(index).zip(digits(count)).ok_or_else(invalid)?;
        if index == 0 || index > count || count > MAX_SHARDS {
            return Err(invalid());
        }
        Ok(Self { index, count })
    }

    /// The shard named by `AOE_E2E_SHARD`, absent or empty meaning the whole
    /// suite. The coverage run is always the whole suite.
    pub(crate) fn from_env(coverage: Option<&str>) -> Result<Option<Self>, String> {
        let text = std::env::var("AOE_E2E_SHARD").unwrap_or_default();
        Self::select(&text, coverage)
    }

    fn select(text: &str, coverage: Option<&str>) -> Result<Option<Self>, String> {
        if text.is_empty() {
            return Ok(None);
        }
        let shard = Self::parse(text)?;
        if coverage.is_some() {
            return Err("AOE_E2E_COVERAGE runs the whole suite; unset AOE_E2E_SHARD".into());
        }
        Ok(Some(shard))
    }

    /// Records that this shard passed. The release evidence in `pass.json`
    /// needs every test, so a shard never writes it.
    pub(crate) fn write_pass(
        &self,
        landscape_hash: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let report = serde_json::json!({
            "version": 1,
            "shard": self.to_string(),
            "landscape_hash": landscape_hash,
            "result": "PASS"
        });
        std::fs::create_dir_all("reports/e2e")?;
        let path = format!("reports/e2e/shard-{}-of-{}.json", self.index, self.count);
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
