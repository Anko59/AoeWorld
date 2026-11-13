//! Joins the shards' evidence into the release evidence `pass.json`.
//!
//! Each shard of the Playwright suite records `shard-i-of-n.json` beside the
//! same report body the release reads. The suite passed as a whole only when
//! every shard 1..=n is there, all are PASS, and all describe one revision.
use serde_json::{Map, Value};
use std::path::Path;

type Failure = Box<dyn std::error::Error>;

/// Reads the shards in `directory` and writes `directory/pass.json`.
pub(crate) fn combine(directory: &Path) -> Result<(), Failure> {
    let report = join(directory)?;
    std::fs::write(
        directory.join("pass.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}

/// `shard-i-of-n.json` as `(i, n)`.
fn shard_name(name: &str) -> Option<(u32, u32)> {
    let (index, count) = name
        .strip_prefix("shard-")?
        .strip_suffix(".json")?
        .split_once("-of-")?;
    Some((index.parse().ok()?, count.parse().ok()?))
}

fn join(directory: &Path) -> Result<Value, Failure> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if let Some(name) = entry.file_name().to_str().and_then(shard_name) {
            found.push((name, entry.path()));
        }
    }
    let Some(&((_, count), _)) = found.first() else {
        return Err(format!("no shard evidence in {}", directory.display()).into());
    };
    if found.iter().any(|((_, other), _)| *other != count) {
        return Err("shard evidence comes from runs split differently".into());
    }
    let mut bodies: Vec<Option<Map<String, Value>>> = vec![None; count as usize];
    for ((index, _), path) in &found {
        let slot = bodies
            .get_mut((*index as usize).wrapping_sub(1))
            .ok_or_else(|| format!("shard {index} is outside 1..={count}"))?;
        let Value::Object(mut body) = serde_json::from_slice(&std::fs::read(path)?)? else {
            return Err(format!("{}: not a JSON object", path.display()).into());
        };
        if body.get("result").and_then(Value::as_str) != Some("PASS") {
            return Err(format!("shard {index}/{count} did not pass").into());
        }
        if body.remove("shard") != Some(Value::String(format!("{index}/{count}"))) {
            return Err(format!("{}: names a different shard", path.display()).into());
        }
        *slot = Some(body);
    }
    let mut bodies = bodies.into_iter().enumerate().map(|(offset, body)| {
        body.ok_or_else(|| format!("shard {}/{count} is missing", offset + 1))
    });
    let first = bodies.next().ok_or("no shard evidence")??;
    for (offset, body) in bodies.enumerate() {
        if body? != first {
            return Err(format!(
                "shard {}/{count} differs from shard 1/{count} (revision, server, assets or fixture)",
                offset + 2
            )
            .into());
        }
    }
    Ok(Value::Object(first))
}

#[cfg(test)]
mod tests;
