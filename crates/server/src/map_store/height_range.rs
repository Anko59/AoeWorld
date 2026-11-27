use aoe_map::{ELEVATION_LEVEL_CENTIMETERS, EnvironmentPage, Ratio};

/// Lowest and highest geographic height of the pages one opening scan decoded:
/// every level-zero elevation sample and every modeled water surface. Overview
/// levels do not count.
#[derive(Debug)]
pub(super) struct HeightRange {
    minimum: i32,
    maximum: i32,
}

impl HeightRange {
    pub(super) fn new() -> Self {
        Self {
            minimum: i32::MAX,
            maximum: i32::MIN,
        }
    }

    /// Adds one validated page. A hydrology page has a water model exactly when
    /// its index does; decoding refuses any other.
    pub(super) fn include(&mut self, page: &EnvironmentPage) {
        match page {
            EnvironmentPage::Elevation(page) if page.level == 0 => {
                self.extend(page.geographic_height_centimeters.iter().copied());
            }
            EnvironmentPage::HydrologyEvidence(page) => {
                if let Some(model) = &page.water_model {
                    self.extend(model.surface_level_centimeters.iter().flatten().copied());
                }
            }
            _ => {}
        }
    }

    /// The range in game height levels, or `None` when no height was scanned.
    /// Quantisation never reverses an order, so the lowest and highest heights
    /// have the lowest and highest levels.
    pub(super) fn levels(&self, compression: Ratio) -> Option<(i16, i16)> {
        (self.minimum <= self.maximum).then(|| {
            (
                quantize_height(self.minimum, compression),
                quantize_height(self.maximum, compression),
            )
        })
    }

    fn extend(&mut self, heights: impl Iterator<Item = i32>) {
        for height in heights {
            self.minimum = self.minimum.min(height);
            self.maximum = self.maximum.max(height);
        }
    }
}

fn quantize_height(height: i32, compression: Ratio) -> i16 {
    let denominator =
        i64::from(ELEVATION_LEVEL_CENTIMETERS).saturating_mul(i64::from(compression.numerator));
    let level = i64::from(height).saturating_mul(i64::from(compression.denominator)) / denominator;
    level.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
}
