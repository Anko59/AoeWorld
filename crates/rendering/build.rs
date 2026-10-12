use std::{env, error::Error, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=src/sprites.wgsl");
    let template = fs::read_to_string("src/sprites.wgsl")?;
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("missing Cargo output directory")?);
    for (mixed, filename) in [(false, "ordinary.wgsl"), (true, "mixing.wgsl")] {
        fs::write(output.join(filename), shader_source(&template, mixed))?;
    }
    Ok(())
}

fn shader_source(template: &str, mixed: bool) -> String {
    // Keep one current template. Validate non-nested sections during the build
    // and ship only the resulting GPU modules, without a browser preprocessor.
    let mut source = String::new();
    let mut branch = None;
    for line in template.lines() {
        match line.trim() {
            "// #if mixing" => {
                assert!(branch.is_none(), "nested shader section");
                branch = Some(true);
            }
            "// #else" => {
                assert_eq!(branch, Some(true), "shader else without mixing section");
                branch = Some(false);
            }
            "// #endif" => {
                assert!(branch.is_some(), "shader end without section");
                branch = None;
            }
            _ if branch.is_none_or(|selected| selected == mixed) && !line.trim().is_empty() => {
                // Formatting stays in the editable template, not each embedded module.
                source.push_str(line.trim());
                source.push('\n');
            }
            _ => {}
        }
    }
    assert!(branch.is_none(), "unclosed shader section");
    source
}
