use serde::Serialize;

// Required denominators come from validated nonempty cohorts and tick windows.
fn scaled_ratio(n: u64, d: u64, scale: u64) -> u64 {
    assert!(d > 0, "required metric denominator must be positive");
    (u128::from(n) * u128::from(scale) / u128::from(d)) as u64
}
fn ratio(n: u64, d: u64, scale: u64) -> Option<u64> {
    (d != 0).then(|| scaled_ratio(n, d, scale))
}
fn quantile(values: &mut [u64], percent: usize) -> Option<u64> {
    values.sort_unstable();
    (!values.is_empty()).then(|| values[(values.len() * percent).div_ceil(100) - 1])
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub(super) struct MovementReport {
    pub ordered: usize,
    pub arrived: usize,
    pub never_arrived: usize,
    pub arrival_ppm: u64,
    pub never_arrived_ppm: u64,
    pub ticks_p50: Option<u64>,
    pub ticks_p95: Option<u64>,
    pub ratio_ppm_p50: Option<u64>,
    pub ratio_ppm_p95: Option<u64>,
    pub obstructed_ticks: u64,
    pub obstructed_ticks_mean_milli: u64,
    pub obstructed_ticks_worst_unit: u64,
    pub units_moved_zero_tiles: usize,
    pub slot_samples: u64,
    pub slot_deviation_mean_subunits: Option<u64>,
    pub slot_deviation_worst_subunits: Option<u64>,
    pub out_of_shape_tick_ppm: Option<u64>,
}
pub(super) struct Movement {
    bounds: Vec<u64>,
    arrivals: Vec<Option<u64>>,
    blocked: Vec<u64>,
    displaced: Vec<u64>,
    slot_sum: u64,
    slot_max: u64,
    slot_count: u64,
    shape_ticks: u64,
    out_ticks: u64,
    tick_has_shape: bool,
    tick_out: bool,
}
impl Movement {
    pub fn new(bounds: Vec<u64>) -> Self {
        assert!(!bounds.is_empty() && bounds.iter().all(|&v| v > 0));
        let n = bounds.len();
        Self {
            bounds,
            arrivals: vec![None; n],
            blocked: vec![0; n],
            displaced: vec![0; n],
            slot_sum: 0,
            slot_max: 0,
            slot_count: 0,
            shape_ticks: 0,
            out_ticks: 0,
            tick_has_shape: false,
            tick_out: false,
        }
    }
    pub fn arrive(&mut self, index: usize, tick: u64) {
        self.arrivals[index].get_or_insert(tick);
    }
    pub fn bound(&mut self, index: usize, ticks: u64) {
        self.bounds[index] = ticks.max(1);
    }
    pub fn arrived_count(&self) -> usize {
        self.arrivals
            .iter()
            .filter(|arrival| arrival.is_some())
            .count()
    }
    pub fn all_arrived(&self) -> bool {
        self.arrivals.iter().all(Option::is_some)
    }
    /// Sample in cohort order, once per unit per tick, including dead/missing units.
    pub fn sample(
        &mut self,
        index: usize,
        obstructed: bool,
        displacement: u64,
        slot_error: Option<u64>,
    ) {
        let active = self.arrivals[index].is_none();
        self.blocked[index] += u64::from(active && obstructed);
        self.displaced[index] = self.displaced[index].max(displacement);
        if let Some(error) = slot_error.filter(|_| active) {
            self.slot_sum += error;
            self.slot_max = self.slot_max.max(error);
            self.slot_count += 1;
            self.tick_has_shape = true;
            self.tick_out |= error > 1024;
        }
        if index + 1 == self.bounds.len() {
            self.shape_ticks += u64::from(self.tick_has_shape);
            self.out_ticks += u64::from(self.tick_out);
            self.tick_has_shape = false;
            self.tick_out = false;
        }
    }
    pub fn finish(self) -> MovementReport {
        let mut ticks: Vec<_> = self.arrivals.iter().flatten().copied().collect();
        let mut ratios: Vec<_> = self
            .arrivals
            .iter()
            .zip(&self.bounds)
            .filter_map(|(t, b)| t.map(|t| scaled_ratio(t, *b, 1_000_000)))
            .collect();
        let n = self.bounds.len();
        let arrived = ticks.len();
        let blocked = self.blocked.iter().sum();
        MovementReport {
            ordered: n,
            arrived,
            never_arrived: n - arrived,
            arrival_ppm: scaled_ratio(arrived as u64, n as u64, 1_000_000),
            never_arrived_ppm: scaled_ratio((n - arrived) as u64, n as u64, 1_000_000),
            ticks_p50: quantile(&mut ticks, 50),
            ticks_p95: quantile(&mut ticks, 95),
            ratio_ppm_p50: quantile(&mut ratios, 50),
            ratio_ppm_p95: quantile(&mut ratios, 95),
            obstructed_ticks: blocked,
            obstructed_ticks_mean_milli: scaled_ratio(blocked, n as u64, 1000),
            obstructed_ticks_worst_unit: self.blocked.into_iter().fold(0, u64::max),
            units_moved_zero_tiles: self.displaced.into_iter().filter(|&v| v < 1024).count(),
            slot_samples: self.slot_count,
            slot_deviation_mean_subunits: ratio(self.slot_sum, self.slot_count, 1),
            slot_deviation_worst_subunits: (self.slot_count > 0).then_some(self.slot_max),
            out_of_shape_tick_ppm: ratio(self.out_ticks, self.shape_ticks, 1_000_000),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(super) struct FightReport {
    pub engaged: usize,
    pub engaged_ppm: u64,
    pub first_contact_ticks: Option<u64>,
    pub first_contact_ticks_p50: Option<u64>,
    pub first_contact_ticks_p95: Option<u64>,
    pub damage_dealt_quarters: u64,
    pub damage_received_quarters: u64,
    pub damage_per_unit_fight_minute_milli: u64,
    pub exchange_ratio_ppm: Option<u64>,
}
pub(super) struct Fight {
    first: Vec<Option<u64>>,
    dealt: u64,
    received: u64,
}
impl Fight {
    pub fn new(n: usize) -> Self {
        Self {
            first: vec![None; n],
            dealt: 0,
            received: 0,
        }
    }
    pub fn hit(&mut self, index: usize, tick: u64, quarters: u64) {
        assert!(quarters > 0);
        self.first[index].get_or_insert(tick);
        self.dealt += quarters;
    }
    pub fn received(&mut self, quarters: u64) {
        self.received += quarters;
    }
    pub fn finish(self, ticks: u64, hz: u64) -> FightReport {
        let mut first: Vec<_> = self.first.iter().flatten().copied().collect();
        FightReport {
            engaged: first.len(),
            engaged_ppm: scaled_ratio(first.len() as u64, self.first.len() as u64, 1_000_000),
            first_contact_ticks: first.iter().min().copied(),
            first_contact_ticks_p50: quantile(&mut first, 50),
            first_contact_ticks_p95: quantile(&mut first, 95),
            damage_dealt_quarters: self.dealt,
            damage_received_quarters: self.received,
            damage_per_unit_fight_minute_milli: scaled_ratio(
                self.dealt * hz * 60,
                4 * self.first.len() as u64 * ticks,
                1000,
            ),
            exchange_ratio_ppm: ratio(self.dealt, self.received, 1_000_000),
        }
    }
}
