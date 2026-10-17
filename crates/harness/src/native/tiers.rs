//! The post-merge test tiers: slow tests that run in their own make target and
//! are excluded from `make test-unit`. `SOURCE_QUALIFICATION` runs in
//! `make test-source-qualification`, `EXHAUSTIVE` (seed sweeps and full-axis
//! geodata round-trips) in `make test-exhaustive`.
//!
//! One list drives both sides, so a test is in exactly one run. Names are
//! nextest test names (module path inside the crate, no binary id).

/// Tests taking over 20 s on a quiet machine. Add a test here only with its
/// exact name; removing one returns it to `make test-unit`.
pub(crate) const SOURCE_QUALIFICATION: &[&str] = &[
    "source_qualification::movement::tests::continuous_source_free_run_completes_all_repeated_waypoints",
    "source_qualification::tests::orchestration::synthetic_flat_sahara_geographic_movement_exercises_full_routes_and_replay",
    "source_qualification::tests::orchestration::synthetic_flat_sahara_package_exercises_full_offline_qualification_contract",
];

/// Tests that sweep all 16 seeds or prepare and round-trip the full 1024-axis
/// geodata fixtures, each over 20 s on a quiet machine. Each seed sweep keeps a
/// fast per-PR variant on two seeds in `make test-unit` (see docs/testing.md).
pub(crate) const EXHAUSTIVE: &[&str] = &[
    "overview::tests::vector_compose_tests::offline_gdal_overview_composes_typed_vectors_and_roundtrips_all_roots",
    "overview::tests::field_axes_tests::offline_field_axes_prepare_independent_canonical_pyramids",
    "overview::tests::field_axes_tests::landscape_water_and_vegetation_are_accepted_and_published_at_the_elevation_axis",
    "hyde::hyde_tests::production_archive_stream_prepares_the_independent_1024_axis",
    "landscape_patches::tests::wilder_regions_have_more_forest_and_larger_stands_on_all16_seeds",
    "landscape_patches::tests::stand_edges_do_not_align_to_chunk_cell_or_lattice_grids",
    "landscape_patches::tests::cover_follows_a_wildness_gradient_without_a_step_at_cell_borders",
    "landscape_patches::tests::openings::most_forested_country_stays_connected_open_ground_on_all16_seeds",
    "landscape_patches::tests::openings::groves_and_forests_below_the_large_radius_are_solid",
    "terrain::tests::rivers::source_river_water_never_invents_invisible_crossings",
];

fn any_of(names: &[&str]) -> String {
    names
        .iter()
        .map(|name| format!("test(={name})"))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Nextest filterset selecting exactly the source-qualification tier.
pub(crate) fn include() -> String {
    any_of(SOURCE_QUALIFICATION)
}

/// Nextest filterset selecting exactly the exhaustive tier.
pub(crate) fn include_exhaustive() -> String {
    any_of(EXHAUSTIVE)
}

/// Nextest filterset selecting everything but the post-merge tiers.
pub(crate) fn exclude() -> String {
    format!("not ({} | {})", include(), include_exhaustive())
}

#[cfg(test)]
mod tests;
