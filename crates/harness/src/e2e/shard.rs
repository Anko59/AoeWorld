//! One Playwright shard of the browser suite: `AOE_E2E_SHARD=i/n` (#388).
use std::fmt;
mod balance;
mod data;
mod plan;
pub(crate) use data::{DurationsArgs, regenerate};

/// The most shards one run may be split into; CI uses four.
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

    /// The shard named by `AOE_E2E_SHARD`, absent or empty meaning the whole suite.
    pub(crate) fn from_env() -> Result<Option<Self>, String> {
        match std::env::var("AOE_E2E_SHARD").unwrap_or_default().as_str() {
            "" => Ok(None),
            text => Self::parse(text).map(Some),
        }
    }

    /// Records that this shard passed: the run's evidence `report` plus the
    /// shard's identity. A shard alone is not the release evidence.
    pub(crate) fn write_pass(
        &self,
        mut report: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        report["shard"] = self.to_string().into();
        std::fs::create_dir_all("reports/e2e")?;
        let path = format!("reports/e2e/shard-{}-of-{}.json", self.index, self.count);
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
