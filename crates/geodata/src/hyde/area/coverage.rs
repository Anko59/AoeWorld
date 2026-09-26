use super::HydeAreaAllocation;
use aoe_map::HistoricalCoverage;

impl HydeAreaAllocation {
    pub(in crate::hyde) fn to_coverage(self) -> HistoricalCoverage {
        let total = self.covered_area_square_meters();
        if total <= 0.0 {
            return HistoricalCoverage {
                outside_percent: 100,
                ..HistoricalCoverage::default()
            };
        }
        // Preserve even sub-percent positive coverage as a nonzero signal.
        let percent = |area: f64| {
            if area > 0.0 {
                (area / total * 100.0).round().clamp(1.0, 100.0) as u8
            } else {
                0
            }
        };
        HistoricalCoverage {
            land_percent: percent(self.land_area_square_meters),
            valid_land_percent: percent(self.valid_land_area_square_meters),
            lake_percent: percent(self.lake_area_square_meters),
            ocean_percent: percent(self.ocean_area_square_meters),
            nodata_percent: percent(self.nodata_area_square_meters),
            outside_percent: percent(self.outside_area_square_meters),
        }
    }
}
