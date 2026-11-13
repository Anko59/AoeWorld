use super::{
    Shard,
    balance::{Durations, Result, assign, check_selection, records},
};
use crate::{
    e2e::{discovery, quarantine},
    process::Cancellation,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

impl Shard {
    pub(in crate::e2e) fn list_path(self, root: &Path) -> PathBuf {
        root.join(format!(
            "reports/e2e/tests-shard-{}-of-{}.txt",
            self.index, self.count
        ))
    }

    pub(in crate::e2e) fn report_path(self) -> PathBuf {
        PathBuf::from(format!(
            "reports/e2e/playwright-shard-{}-of-{}.json",
            self.index, self.count
        ))
    }

    pub(in crate::e2e) fn prepare(
        self,
        args: &[String],
        root: &Path,
        cancellation: &Cancellation,
    ) -> Result<Vec<String>> {
        let end = args
            .iter()
            .position(|arg| arg == quarantine::REPORTER)
            .ok_or("missing JSON reporter")?;
        let full_report = self.report_path().with_extension("all.json");
        let mut command = Command::new("docker");
        command.args(&args[..=end]).arg("--list");
        discovery::run(command, &full_report, Duration::from_secs(60), cancellation)?;
        let tests = names(&full_report)?;
        let durations: Durations = serde_json::from_str(include_str!("durations.json"))?;
        let plan = assign(&tests, &durations, self.count)?;
        let selected = plan
            .get((self.index - 1) as usize)
            .ok_or("invalid shard index")?;
        if selected.is_empty() {
            return Err("shard has no assigned tests; reduce the shard count".into());
        }
        fs::write(self.list_path(root), selected.join("\n") + "\n")?;
        let filtered_report = self.report_path().with_extension("selected.json");
        let mut command = Command::new("docker");
        command.args(args).arg("--list");
        discovery::run(
            command,
            &filtered_report,
            Duration::from_secs(60),
            cancellation,
        )?;
        check_selection(selected, &names(&filtered_report)?)?;
        println!(
            "test-e2e: shard {self} assigned {} of {} tests; filtered discovery matches exactly",
            selected.len(),
            tests.len()
        );
        Ok(selected.clone())
    }

    pub(in crate::e2e) fn verify(self, assigned: &[String]) -> Result<()> {
        check_selection(assigned, &names(&self.report_path())?)?;
        println!(
            "test-e2e: shard {self} executed exactly its {} assigned tests",
            assigned.len()
        );
        Ok(())
    }
}

fn names(report: &Path) -> Result<Vec<String>> {
    Ok(records(&serde_json::from_slice(&fs::read(report)?)?)?
        .into_iter()
        .map(|(name, _)| name)
        .collect())
}
