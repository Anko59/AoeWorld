//! `aoe-harness assets`: inspect, import and verify the local asset packs.
mod dat_table;

use clap::Subcommand;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub(crate) enum AssetCommand {
    Inspect {
        input: Option<PathBuf>,
    },
    Import {
        input: Option<PathBuf>,
    },
    Verify {
        pack: Option<PathBuf>,
    },
    /// Decode empires2.dat into a local-only name to sprite table.
    DatTable {
        input: Option<PathBuf>,
    },
}

pub(crate) fn run(command: AssetCommand) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        AssetCommand::Inspect { input } => {
            let input = input.unwrap_or_else(|| PathBuf::from("local-assets/trial"));
            println!(
                "{}",
                serde_json::to_string_pretty(&aoe_assets::pack::inspect(&input)?)?
            );
        }
        AssetCommand::Import { input } => {
            let input = input.unwrap_or_else(|| {
                let trial = PathBuf::from("local-assets/trial");
                let game_data = trial.join("Data");
                if game_data.is_dir() { game_data } else { trial }
            });
            println!(
                "{}",
                aoe_assets::pack::import(&input, Path::new("local-assets/packs"))?.display()
            );
        }
        AssetCommand::DatTable { input } => {
            let input = input.unwrap_or_else(|| PathBuf::from("local-assets/trial/Data"));
            dat_table::run(&input, Path::new("local-assets/dat-table"))?;
        }
        AssetCommand::Verify { pack } => {
            let packs = if let Some(pack) = pack {
                vec![pack]
            } else {
                std::fs::read_dir("local-assets/packs")?
                    .filter_map(|item| item.ok().map(|value| value.path()))
                    .filter(|path| path.is_dir())
                    .collect()
            };
            if packs.is_empty() {
                return Err("no local asset packs found".into());
            }
            for pack in packs {
                let manifest = aoe_assets::pack::verify(&pack)?;
                println!(
                    "verified {}: {} frames across {} pages",
                    pack.display(),
                    manifest.frames.len(),
                    manifest.pages.len()
                );
            }
        }
    }
    Ok(())
}
