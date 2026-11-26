//! UTC dates from the `…Z` timestamps GitHub and Git give.
use crate::perf_comment::epoch_seconds;

const DAY: i64 = 86_400;

/// `2026-11-25T12:32:38Z` from Unix seconds.
pub(super) fn iso(seconds: i64) -> String {
    crate::ship::lock::utc(u64::try_from(seconds).unwrap_or(0)).replace(' ', "T")
}

/// The date part of a timestamp: `2026-11-25`.
pub(super) fn day(at: &str) -> &str {
    at.get(..10).unwrap_or(at)
}

/// The Monday (`2026-11-23`) of the week a timestamp falls in.
pub(super) fn week_start(at: &str) -> Option<String> {
    let days = epoch_seconds(at)?.div_euclid(DAY);
    // 1970-01-01 was a Thursday: with Monday as 0 its index is 3.
    let weekday = (days + 3).rem_euclid(7);
    Some(day(&iso((days - weekday) * DAY)).to_owned())
}

/// Seconds since 1970 of the day's midnight, or `None` for a malformed date.
pub(super) fn midnight(date: &str) -> Option<i64> {
    epoch_seconds(&format!("{date}T00:00:00Z"))
}
