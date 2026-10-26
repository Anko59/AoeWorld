/// The standard float-clamp behavior, including NaN values and signed zero.
/// Keep the invalid-bound check, with static panic text: formatting its bounds
/// otherwise retains the full float-to-decimal formatter in browser builds.
#[inline]
#[track_caller]
pub fn clamp_f64(value: f64, min: f64, max: f64) -> f64 {
    assert!(min <= max, "min > max, or either was NaN");
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

#[cfg(test)]
mod tests;
