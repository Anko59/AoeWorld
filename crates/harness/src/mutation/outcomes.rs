//! Closed pinned wire records and reconciliation, not authenticated tool verdicts.
use super::{FILES, Result, TOOL_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Outcomes {
    pub(super) cargo_mutants_version: String,
    pub(super) total_mutants: u64,
    pub(super) caught: u64,
    pub(super) missed: u64,
    pub(super) timeout: u64,
    pub(super) unviable: u64,
    pub(super) success: u64,
    pub(super) start_time: Option<String>,
    pub(super) end_time: Option<String>,
    pub(super) outcomes: Vec<Record>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    scenario: Scenario,
    summary: Summary,
    log_path: String,
    diff_path: Option<String>,
    phase_results: Vec<PhaseResult>,
}
#[derive(Debug, Deserialize, Serialize)]
pub(super) enum Scenario {
    Baseline,
    Mutant(Box<Mutant>),
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) enum Summary {
    Success,
    CaughtMutant,
    MissedMutant,
    Unviable,
    Failure,
    Timeout,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) enum Phase {
    Check,
    Build,
    Test,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) enum Status {
    Success,
    Failure(i32),
    Timeout,
    Signalled(i32),
    Other,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PhaseResult {
    phase: Phase,
    duration: f64,
    process_status: Status,
    argv: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Mutant {
    name: String,
    package: String,
    file: String,
    #[serde(deserialize_with = "required_option")]
    function: Option<Function>,
    span: Span,
    replacement: String,
    genre: Genre,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) enum Genre {
    FnValue,
    BinaryOperator,
    UnaryOperator,
    MatchArm,
    MatchArmGuard,
    StructField,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Function {
    function_name: String,
    return_type: String,
    span: Span,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Span {
    start: Location,
    end: Location,
}
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
struct Location {
    line: u64,
    column: u64,
}

fn required_option<'de, D, T>(decoder: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(decoder)
}
fn text(value: &str, limit: usize, empty: bool) -> Result<()> {
    if value.len() > limit || (!empty && value.is_empty()) || value.contains('\0') {
        return Err("mutation text is empty, oversized or contains NUL".into());
    }
    Ok(())
}
fn relative(value: &str) -> Result<()> {
    text(value, 16384, false)?;
    if value.starts_with('/')
        || value.contains('\\')
        || value
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
        || value.chars().any(char::is_control)
    {
        return Err("mutation path must be normal and relative".into());
    }
    Ok(())
}
impl Span {
    fn validate(&self) -> Result<()> {
        if self.start.line == 0
            || self.start.column == 0
            || self.end.line == 0
            || self.end.column == 0
            || self.end < self.start
        {
            return Err("mutation span is invalid".into());
        }
        Ok(())
    }
}
impl Mutant {
    fn validate(&self) -> Result<()> {
        text(&self.name, 16384, false)?;
        text(&self.package, 16384, false)?;
        text(&self.replacement, 65536, true)?;
        relative(&self.file)?;
        if !FILES.contains(&self.file.as_str()) {
            return Err("mutation file outside fixed scope".into());
        }
        self.span.validate()?;
        if let Some(function) = &self.function {
            text(&function.function_name, 16384, false)?;
            text(&function.return_type, 16384, true)?;
            function.span.validate()?;
        }
        Ok(())
    }
}
impl Record {
    fn computed(&self) -> Result<Summary> {
        relative(&self.log_path)?;
        if let Some(path) = &self.diff_path {
            relative(path)?;
        }
        if self.phase_results.is_empty() || self.phase_results.len() > 3 {
            return Err("mutation phases missing or oversized".into());
        }
        let mut previous = 0;
        for result in &self.phase_results {
            let order = match result.phase {
                Phase::Check => 1,
                Phase::Build => 2,
                Phase::Test => 3,
            };
            if order <= previous
                || !result.duration.is_finite()
                || result.duration < 0.0
                || result.argv.is_empty()
                || result.argv.len() > 128
            {
                return Err("mutation phase order, duration or argv invalid".into());
            }
            previous = order;
            if matches!(result.process_status, Status::Failure(0))
                || matches!(result.process_status, Status::Signalled(signal) if signal <= 0)
            {
                return Err("mutation process status is internally invalid".into());
            }
            for arg in &result.argv {
                text(arg, 16384, true)?;
            }
        }
        let last = self.phase_results.last().ok_or("mutation phases missing")?;
        // Pinned v27.1.0 precedence. Signalled/Other are NOT Failure(code).
        if matches!(self.scenario, Scenario::Mutant(_))
            && self.phase_results.iter().any(|phase| {
                phase.phase != Phase::Test && matches!(phase.process_status, Status::Failure(_))
            })
        {
            return Ok(Summary::Unviable);
        }
        if self
            .phase_results
            .iter()
            .any(|phase| phase.process_status == Status::Timeout)
        {
            return Ok(Summary::Timeout);
        }
        if matches!(self.scenario, Scenario::Mutant(_)) && last.phase == Phase::Test {
            if self.phase_results.len() != 2
                || self.phase_results[0].phase != Phase::Build
                || self.phase_results[0].process_status != Status::Success
            {
                return Err("evaluated mutation Test must follow successful Build".into());
            }
            match last.process_status {
                Status::Failure(_) => return Ok(Summary::CaughtMutant),
                Status::Success => return Ok(Summary::MissedMutant),
                _ => {}
            }
        }
        Ok(if last.process_status == Status::Success {
            Summary::Success
        } else {
            Summary::Failure
        })
    }
}
impl Outcomes {
    pub(super) fn evaluated(&self) -> Option<u64> {
        self.caught
            .checked_add(self.missed)?
            .checked_add(self.timeout)
    }
    pub(super) fn counters_valid(&self) -> bool {
        self.evaluated()
            .and_then(|count| count.checked_add(self.unviable))
            .and_then(|count| count.checked_add(self.success))
            == Some(self.total_mutants)
    }
    pub(super) fn reconcile(&self, inventory: &[Mutant]) -> Result<()> {
        if self.cargo_mutants_version != TOOL_VERSION
            || !self.counters_valid()
            || inventory.len() as u64 != self.total_mutants
        {
            return Err("mutation identity or counter/inventory totals invalid".into());
        }
        for time in [&self.start_time, &self.end_time] {
            text(
                time.as_deref().ok_or("mutation completion missing")?,
                128,
                false,
            )?;
        }
        let mut names = BTreeMap::new();
        for mutant in inventory {
            mutant.validate()?;
            if names.insert(&mutant.name, mutant).is_some() {
                return Err("duplicate inventory mutant".into());
            }
        }
        let mut observed = BTreeSet::new();
        let mut baseline = 0;
        let mut counts = [0u64; 5];
        for record in &self.outcomes {
            if record.computed()? != record.summary {
                return Err("claimed mutation summary disagrees with phases".into());
            }
            match &record.scenario {
                Scenario::Baseline => {
                    baseline += 1;
                    if record.summary != Summary::Success
                        || record.diff_path.is_some()
                        || record.phase_results.len() != 2
                        || record.phase_results[0].phase != Phase::Build
                        || record.phase_results[1].phase != Phase::Test
                        || record
                            .phase_results
                            .iter()
                            .any(|phase| phase.process_status != Status::Success)
                    {
                        return Err("mutation baseline must claim successful Build and Test".into());
                    }
                }
                Scenario::Mutant(mutant) => {
                    mutant.validate()?;
                    if names.get(&mutant.name).copied() != Some(mutant.as_ref())
                        || !observed.insert(&mutant.name)
                        || record.diff_path.is_none()
                    {
                        return Err(
                            "mutation outcome missing, duplicate or differs from inventory".into(),
                        );
                    }
                    let index = match record.summary {
                        Summary::CaughtMutant => 0,
                        Summary::MissedMutant => 1,
                        Summary::Timeout => 2,
                        Summary::Unviable => 3,
                        Summary::Success => 4,
                        Summary::Failure => {
                            return Err(
                                "unclassified mutant failure is not complete evidence".into()
                            );
                        }
                    };
                    counts[index] = counts[index]
                        .checked_add(1)
                        .ok_or("mutation counter overflow")?;
                }
            }
        }
        if baseline != 1
            || observed.len() != names.len()
            || counts
                != [
                    self.caught,
                    self.missed,
                    self.timeout,
                    self.unviable,
                    self.success,
                ]
        {
            return Err(
                "mutation baseline, record coverage or counter reconciliation invalid".into(),
            );
        }
        Ok(())
    }
}

pub(super) fn parse(outcomes: &[u8], inventory: &[u8]) -> Result<Outcomes> {
    let wire: Outcomes =
        serde_json::from_value(crate::input_json::parse(outcomes, 4 * 1024 * 1024)?)?;
    let raw = crate::input_json::parse(inventory, 4 * 1024 * 1024)?;
    let entries = raw
        .as_array()
        .ok_or("mutation inventory must be an array")?;
    let mut descriptors = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut fields = entry
            .as_object()
            .ok_or("mutation inventory entry must be an object")?
            .clone();
        let diff = fields
            .remove("diff")
            .ok_or("mutation inventory diff missing")?;
        text(
            diff.as_str().ok_or("mutation diff must be text")?,
            65536,
            false,
        )?;
        descriptors.push(serde_json::from_value(serde_json::Value::Object(fields))?);
    }
    wire.reconcile(&descriptors)?;
    Ok(wire)
}
