use crate::source_qualification::report::ProcessRssBytes;
use aoe_simulation::GameWorld;
use std::fs;

const PROC_SELF_STATUS: &str = "/proc/self/status";

#[derive(Debug, Default)]
pub(super) struct NavigationCachePeaks {
    pub(super) route_entries: usize,
    pub(super) replay_entries: usize,
    pub(super) combined_entries: usize,
    pub(super) route_logical_retained_bytes: usize,
    pub(super) replay_logical_retained_bytes: usize,
    pub(super) combined_logical_retained_bytes: usize,
}

impl NavigationCachePeaks {
    pub(super) fn observe(&mut self, route: &GameWorld, replay: &GameWorld) {
        let route_usage = route.navigation_cache_usage();
        let replay_usage = replay.navigation_cache_usage();
        self.route_entries = self.route_entries.max(route_usage.entries);
        self.replay_entries = self.replay_entries.max(replay_usage.entries);
        self.combined_entries = self
            .combined_entries
            .max(route_usage.entries.saturating_add(replay_usage.entries));
        self.route_logical_retained_bytes = self
            .route_logical_retained_bytes
            .max(route_usage.retained_bytes);
        self.replay_logical_retained_bytes = self
            .replay_logical_retained_bytes
            .max(replay_usage.retained_bytes);
        self.combined_logical_retained_bytes = self.combined_logical_retained_bytes.max(
            route_usage
                .retained_bytes
                .saturating_add(replay_usage.retained_bytes),
        );
    }
}

pub(super) struct ProcessRssSampler {
    start: Option<u64>,
    peak: Option<u64>,
}

impl ProcessRssSampler {
    pub(super) fn start() -> Self {
        let start = read_process_rss_bytes();
        Self { start, peak: start }
    }

    pub(super) fn observe(&mut self) {
        self.record(read_process_rss_bytes());
    }

    pub(super) fn finish(mut self) -> ProcessRssBytes {
        let end = read_process_rss_bytes();
        self.record(end);
        ProcessRssBytes {
            start: self.start,
            peak: self.peak,
            end,
        }
    }

    fn record(&mut self, observed: Option<u64>) {
        self.peak = match (self.peak, observed) {
            (Some(peak), Some(observed)) => Some(peak.max(observed)),
            (peak, observed) => peak.or(observed),
        };
    }
}

fn read_process_rss_bytes() -> Option<u64> {
    parse_vm_rss_bytes(&fs::read_to_string(PROC_SELF_STATUS).ok()?)
}

fn parse_vm_rss_bytes(status: &str) -> Option<u64> {
    let line = status.lines().find(|line| {
        line.split_once(':')
            .is_some_and(|(name, _)| name.trim() == "VmRSS")
    })?;
    let (_, value) = line.split_once(':')?;
    let mut fields = value.split_whitespace();
    let amount = fields.next()?.parse::<u64>().ok()?;
    let unit = fields.next()?;
    if fields.next().is_some() {
        return None;
    }
    match unit {
        "kB" => amount.checked_mul(1_024),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vm_rss_parser_converts_kilobytes_to_bytes() {
        assert_eq!(
            parse_vm_rss_bytes("Name:\taoe-server\nVmRSS:\t   24480 kB\n"),
            Some(25_067_520)
        );
    }

    #[test]
    fn vm_rss_parser_returns_none_when_vm_rss_is_absent() {
        assert_eq!(
            parse_vm_rss_bytes("Name:\taoe-server\nVmHWM:\t96 kB\n"),
            None
        );
        assert_eq!(parse_vm_rss_bytes(""), None);
    }

    #[test]
    fn vm_rss_parser_rejects_invalid_values_and_units() {
        for status in [
            "VmRSS:\n",
            "VmRSS:\tnot-a-number kB\n",
            "VmRSS:\t12.5 kB\n",
            "VmRSS:\t12 kB extra\n",
            "VmRSS:\t12 MB\n",
            "VmRSS:\t-1 kB\n",
            "VmRSS:\t18446744073709551615 kB\n",
        ] {
            assert_eq!(parse_vm_rss_bytes(status), None, "{status:?}");
        }
    }

    #[test]
    fn process_sampler_reports_distinct_start_peak_and_end() {
        let rss = ProcessRssSampler::start().finish();
        if let Some(start) = rss.start {
            assert!(start > 0);
            assert!(rss.peak.unwrap_or_default() >= start);
            assert!(rss.end.is_some());
        } else {
            assert!(rss.peak.is_none());
            assert!(rss.end.is_none());
        }
    }
}
