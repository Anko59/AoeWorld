/// IEEE clamp with the standard value/NaN/signed-zero behavior and a bounded
/// panic message. Formatting invalid floating bounds would retain the full
/// float Debug formatter in browser WASM, even though valid calls never use it.
#[inline]
pub fn clamp_f64(value: f64, min: f64, max: f64) -> f64 {
    assert!(min <= max, "invalid floating clamp bounds");
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
