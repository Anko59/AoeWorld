//! The source-qualification tier: slow offline integration tests that run in
//! `make test-source-qualification` and are excluded from `make test-unit`.
//!
//! One list drives both sides, so a test is in exactly one of the two runs.
//! Names are nextest test names (module path inside the crate, no binary id).

/// Tests taking over 20 s on a quiet machine. Add a test here only with its
/// exact name; removing one returns it to `make test-unit`.
pub(crate) const SOURCE_QUALIFICATION: &[&str] = &[
    "source_qualification::movement::tests::continuous_source_free_run_completes_all_repeated_waypoints",
    "source_qualification::tests::orchestration::synthetic_flat_sahara_geographic_movement_exercises_full_routes_and_replay",
    "source_qualification::tests::orchestration::synthetic_flat_sahara_package_exercises_full_offline_qualification_contract",
];

/// Nextest filterset selecting exactly the tier.
pub(crate) fn include() -> String {
    SOURCE_QUALIFICATION
        .iter()
        .map(|name| format!("test(={name})"))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Nextest filterset selecting everything but the tier.
pub(crate) fn exclude() -> String {
    format!("not ({})", include())
}

#[cfg(test)]
mod tests;
