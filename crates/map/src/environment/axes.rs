use super::{
    EnvironmentError, FieldPyramid, MAX_ENVIRONMENT_SAMPLES_PER_AXIS, PreparedEnvironment,
};

impl PreparedEnvironment {
    pub fn water_samples_per_axis(&self) -> Option<u16> {
        self.water.as_ref()?.axis()
    }

    pub fn vegetation_samples_per_axis(&self) -> Option<u16> {
        self.vegetation.as_ref()?.axis()
    }

    /// History has its own prepared axis.
    pub fn historical_samples_per_axis(&self) -> Option<u16> {
        self.historical_land_use.as_ref()?.axis()
    }
}

impl FieldPyramid {
    pub(super) fn axis(&self) -> Option<u16> {
        self.levels.first().map(|level| level.samples_per_axis)
    }

    pub(super) fn validate_axis(&self, axis: u16) -> Result<(), EnvironmentError> {
        if axis == 0
            || axis > MAX_ENVIRONMENT_SAMPLES_PER_AXIS
            || self
                .levels
                .last()
                .is_none_or(|level| level.samples_per_axis != 1)
            || self
                .levels
                .iter()
                .rev()
                .skip(1)
                .any(|level| level.samples_per_axis == 1)
        {
            return Err(EnvironmentError::InvalidPyramid);
        }
        self.validate(axis)
    }
}
