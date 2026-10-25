//! `aoe-harness assets`: inspect, import and verify the local asset packs.
mod dat_table;
#[path = "tests/makefile.rs"]
#[cfg(test)]
mod makefile_tests;
mod overlay;
#[path = "tests/overlay.rs"]
#[cfg(test)]
mod overlay_tests;

use clap::Subcommand;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub(crate) enum AssetCommand {
    /// Emit the current GPU programs into the ignored browser bundle.
    BrowserShaders,
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
        /// Numeric Feudal survey of a licensed DE VER 8.9 dat file.
        #[arg(long)]
        de: Option<PathBuf>,
    },
    /// Imports the Dark Age buildings from a Definitive Edition install.
    ImportDe {
        install: PathBuf,
        base: Option<PathBuf>,
    },
    /// Local complete-DE versus trial native-scale composition evidence.
    DeReference {
        install: PathBuf,
        output: PathBuf,
    },
    /// Local native-scale proof of a pack building recipe (no public pixels).
    BuildingPreview {
        pack: PathBuf,
        output: PathBuf,
        #[arg(long, default_value_t = 109)]
        unit: i32,
        #[arg(long)]
        recipe: Option<PathBuf>,
    },
}

pub(crate) fn browser_shaders() -> impl Iterator<Item = (&'static str, &'static str)> {
    ["ordinary", "mixing"]
        .into_iter()
        .zip(aoe_rendering::shader_data::browser_shaders())
}

pub(crate) fn run(command: AssetCommand) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        AssetCommand::BrowserShaders => {
            std::fs::create_dir_all("web/pkg")?;
            for (name, source) in browser_shaders() {
                std::fs::write(format!("web/pkg/{name}.wgsl"), source)?;
            }
        }
        AssetCommand::DeReference { install, output } => {
            aoe_assets::de::reference(&install, &output)?;
        }
        AssetCommand::Inspect { input } => {
            let input = input.unwrap_or_else(|| PathBuf::from("local-assets/trial"));
            println!(
                "{}",
                serde_json::to_string_pretty(&aoe_assets::pack::inspect(&input)?)?
            );
        }
        AssetCommand::BuildingPreview {
            pack,
            output,
            unit,
            recipe,
        } => {
            if !output.starts_with(".cache")
                || output
                    .components()
                    .any(|c| c == std::path::Component::ParentDir)
            {
                return Err("building proof must stay below ignored .cache/".into());
            }
            let manifest = aoe_assets::pack::verify(&pack)?;
            let custom;
            let recipe = if let Some(path) = recipe {
                custom = serde_json::from_slice::<aoe_assets::dat::BuildingComposition>(
                    &std::fs::read(path)?,
                )?;
                &custom
            } else {
                manifest
                    .buildings
                    .iter()
                    .find(|b| b.unit_id == unit)
                    .ok_or("building recipe missing from pack")?
            };
            aoe_assets::pack::composite::render(&pack, &manifest, recipe, &output)?;
            println!("wrote native-scale local composite {}", output.display());
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
        AssetCommand::DatTable { input, de } => {
            if let Some(path) = de {
                return dat_table::de(&path, Path::new("local-assets/dat-table"));
            }
            let input = input.unwrap_or_else(|| PathBuf::from("local-assets/trial/Data"));
            dat_table::run(&input, Path::new("local-assets/dat-table"))?;
        }
        AssetCommand::ImportDe { install, base } => {
            let report = aoe_assets::de::import(&install, Path::new("local-assets/packs"))?;
            if let Some(base) = base {
                overlay::attach(&base, &report.pack)?;
            }
            println!("{}", report.pack.display());
            for entry in &report.entries {
                let first = &entry.frames[0];
                println!(
                    "{:<24} {:<18} {:>3} frames  frame0 {}x{}  body {}x{}",
                    entry.building,
                    entry.role,
                    entry.frames.len(),
                    first.width,
                    first.height,
                    first.body.0,
                    first.body.1
                );
            }
            for (building, role, file) in &report.absent {
                println!("absent: {building} {role} ({file})");
            }
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
