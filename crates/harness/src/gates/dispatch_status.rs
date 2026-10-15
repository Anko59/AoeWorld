//! A dispatch job check is not evaluated for PRs; publish its aggregate as a status.
use std::{env, error::Error, process::Command};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn required(name: &str) -> Result<String> {
    let value = env::var(name)?;
    if value.is_empty() {
        return Err(format!("{name} must not be empty").into());
    }
    Ok(value)
}

pub(crate) fn run() -> Result<()> {
    if env::var("GITHUB_ACTIONS").as_deref() != Ok("true")
        || env::var("GITHUB_EVENT_NAME").as_deref() != Ok("workflow_dispatch")
    {
        return Err("ci-dispatch-status requires GitHub Actions workflow_dispatch".into());
    }
    let reference = required("GITHUB_REF_NAME")?;
    if matches!(reference.as_str(), "dev" | "main") {
        return Err("ci-dispatch-status refuses dev and main refs".into());
    }
    let sha = required("GITHUB_SHA")?;
    let repository = required("GITHUB_REPOSITORY")?;
    let run = required("GITHUB_RUN_ID")?;
    let server = required("GITHUB_SERVER_URL")?;
    let status = required("JOB_STATUS")?;
    let state = if status == "success" {
        "success"
    } else {
        "failure"
    };
    let output = Command::new("gh")
        .args([
            "api",
            "--method",
            "POST",
            &format!("repos/{repository}/statuses/{sha}"),
            "-f",
            "context=required",
            "-f",
            &format!("state={state}"),
            "-f",
            &format!("description=workflow_dispatch run {run}: {status}"),
            "-f",
            &format!("target_url={server}/{repository}/actions/runs/{run}"),
        ])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "required commit status failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    println!(
        "ci-dispatch-status: required={state} on {sha} (workflow_dispatch run {run}: {status})"
    );
    Ok(())
}
