/// Symmetric native geometry envelopes, including mirroring and shadow offsets.
pub(crate) fn include(
    extent: &mut [f64; 2],
    size: [f32; 2],
    anchor: [f32; 2],
    offset: [f32; 2],
    scale: f64,
) {
    for axis in 0..2 {
        if !scale.is_finite()
            || scale < 0.0
            || [size[axis], anchor[axis], offset[axis]]
                .iter()
                .any(|v| !v.is_finite())
        {
            extent[axis] = f64::INFINITY;
            continue;
        }
        let reach = f64::from(anchor[axis].abs().max((size[axis] - anchor[axis]).abs()))
            + f64::from(offset[axis].abs());
        extent[axis] = extent[axis].max(reach * scale + 8.0);
    }
}

#[cfg(test)]
#[path = "tests/extents.rs"]
mod tests;
