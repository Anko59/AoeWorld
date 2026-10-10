use super::*;

/// Native preparation options are shared with the server through `aoe_map`;
/// they are not part of the map request or package wire schema.
pub use aoe_map::OverviewFieldAxes;

/// Field-local overview axes may raise elevation to the landscape cap; the
/// coupled overview context of detailed preparation keeps the direct cap.
pub(crate) fn validate_field_axes(
    axes: OverviewFieldAxes,
    explicit: bool,
) -> Result<(), GeodataError> {
    let elevation_cap = if explicit {
        elevation::MAX_LANDSCAPE_DIRECT_ELEVATION_SAMPLES_PER_AXIS
    } else {
        MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS
    };
    if !(2..=elevation_cap).contains(&axes.elevation)
        || !(2..=MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS).contains(&axes.water)
        || !(2..=MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS).contains(&axes.vegetation)
        || !(2..=MAX_HISTORICAL_GRID_SAMPLES_PER_AXIS).contains(&axes.historical)
    {
        return Err(GeodataError::Preparation(
            "overview field axes are outside direct bounds",
        ));
    }
    Ok(())
}
