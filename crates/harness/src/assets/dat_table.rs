//! `aoe-harness assets dat-table`: decode empires2.dat into local-only tables.
use aoe_assets::dat::{self, table};
use std::path::Path;

pub(super) fn run(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = dat::decode(&std::fs::read(input.join("empires2.dat"))?)?;
    let pack = match std::fs::read(input.join("graphics.drs")) {
        Ok(bytes) => Some(table::slp_ids(&bytes)?),
        Err(_) => None,
    };
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
    Ok(())
}
