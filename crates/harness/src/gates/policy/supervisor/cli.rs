use super::*;
use crate::gates::runner::evidence::PrivateOutput;
use std::io::Read;
#[derive(Debug, clap::Args)]
pub(crate) struct Options {
    /// Bounded unverified requirements JSON; never an admission token.
    #[arg(long)]
    requirements: PathBuf,
    /// Existing external directory for model-only feedback.
    #[arg(long)]
    output: PathBuf,
}
fn input(path: &Path) -> Result<Vec<u8>> {
    let path = plain_absolute(path)?;
    let parent = path.parent().ok_or("requirements parent missing")?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("requirements name invalid")?;
    let path = super::super::regular(parent, name)?;
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(32769).read_to_end(&mut bytes)?;
    if bytes.len() > 32768 {
        return Err("supervisor requirements exceed 32KiB".into());
    }
    Ok(bytes)
}
pub(crate) fn execute(root: &Path, options: Options) -> Result<()> {
    let root = fs::canonicalize(root)?;
    let git_common = crate::hooks::common_directory(&root)?;
    let output = PrivateOutput::new(&options.output, &[root.clone(), git_common])?;
    output.reject_input_alias(&options.requirements, "supervisor input")?;
    output.atomic(
        "supervisor-model.json",
        b"{\"status\":\"UNAVAILABLE\",\"authoritative\":false,\"reason\":\"model in progress\"}",
    )?;
    let result = (|| -> Result<Value> {
        let bytes = input(&options.requirements)?;
        let requirements = requirements::Requirements::parse(&bytes)?;
        let mut report = requirements.report(&root)?;
        report["requirements_blake3"] = json!(blake3::hash(&bytes).to_hex().to_string());
        report["lease_models"] = lease::preview_models();
        if input(&options.requirements)? != bytes {
            return Err("supervisor requirements endpoint changed".into());
        }
        Ok(report)
    })();
    let report = match &result {
        Ok(report) => report.clone(),
        Err(error) => {
            json!({"schema":1,"status":"UNAVAILABLE","authoritative":false,"reason":error.to_string()})
        }
    };
    output.atomic(
        "supervisor-model.json",
        &serde_json::to_vec_pretty(&report)?,
    )?;
    result?;
    println!("SUPERVISOR_MODEL_ONLY; trusted admission UNAVAILABLE");
    Ok(())
}
