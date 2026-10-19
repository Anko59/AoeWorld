//! `aoe-harness assets dat-table`: decode empires2.dat into local-only tables.
use aoe_assets::dat::{self, table};
use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::path::Path;

/// SLP ids of `graphics.drs`; only a missing file means "no pack to compare".
fn pack_ids(input: &Path) -> Result<Option<BTreeSet<i32>>, Box<dyn std::error::Error>> {
    match std::fs::read(input.join("graphics.drs")) {
        Ok(bytes) => Ok(Some(table::slp_ids(&bytes)?)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("reading graphics.drs: {error}").into()),
    }
}

pub(super) fn run(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = dat::decode(&std::fs::read(input.join("empires2.dat"))?)?;
    let pack = pack_ids(input)?;
    let table = table::build(&file, pack.as_ref());
    std::fs::create_dir_all(output)?;
    std::fs::write(
        output.join("table.json"),
        serde_json::to_vec_pretty(&table)?,
    )?;
    std::fs::write(output.join("table.md"), table::markdown(&table))?;
    print!("{}", table::summary(&table));
    println!(
        "wrote {}/table.json and table.md (local only, not for Git)",
        output.display()
    );
    match &file.report.stopped {
        Some(reason) => Err(format!("table is INCOMPLETE: {reason}").into()),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests;
