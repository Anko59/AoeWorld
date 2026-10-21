//! A display title is lossy: retain every component of a Playwright case identity.
use super::{Result, string};
use serde_json::Value;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    pub file: String,
    pub line: u64,
    pub column: u64,
    pub project: String,
    pub titles: Vec<String>,
}

impl Identity {
    pub(super) fn parse(
        spec: &Value,
        test: &Value,
        titles: &[String],
        root: &Path,
    ) -> Result<Self> {
        let location = |field| {
            spec.get(field)
                .and_then(Value::as_u64)
                .filter(|value| *value > 0)
                .ok_or_else(|| format!("missing spec {field}"))
        };
        Ok(Self {
            file: root
                .join(string(spec, "file")?)
                .to_string_lossy()
                .into_owned(),
            line: location("line")?,
            column: location("column")?,
            project: string(test, "projectName")?.to_owned(),
            titles: titles.to_vec(),
        })
    }
}
