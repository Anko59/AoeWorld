//! The machine load when a deadline expires: a gate that hits its budget on
//! an overloaded machine says so, instead of looking like a slow change.
use std::fs;

/// The 1, 5 and 15 minute load averages and the core count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Load {
    pub(crate) one: f64,
    pub(crate) five: f64,
    pub(crate) fifteen: f64,
    pub(crate) cores: usize,
}

/// Reads the load now; tests inject a fake one.
pub(crate) type Reader<'a> = dyn Fn() -> Option<Load> + 'a;

/// The first three fields of `/proc/loadavg`.
pub(crate) fn parse(loadavg: &str, cores: usize) -> Option<Load> {
    let mut fields = loadavg.split_whitespace().map(|f| f.parse::<f64>().ok());
    Some(Load {
        one: fields.next()??,
        five: fields.next()??,
        fifteen: fields.next()??,
        cores,
    })
}

/// The load of this machine (`None` where `/proc/loadavg` does not exist).
pub(crate) fn read() -> Option<Load> {
    parse(&fs::read_to_string("/proc/loadavg").ok()?, cores())
}

/// The cores this process may run on: its CPU affinity list, which `taskset` narrows. It is not
/// `available_parallelism`, which also applies a container's CPU quota (`--cpus`): a container
/// started under a heavy slot keeps the machine's count, not its slot's share.
pub(crate) fn cores() -> usize {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
                .and_then(online_count)
        })
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()))
}

/// The number of CPUs in a kernel CPU list such as `0-3,6` (sysfs online and affinity lists alike).
pub(crate) fn online_count(list: &str) -> Option<usize> {
    list.trim()
        .split(',')
        .map(|range| match range.split_once('-') {
            Some((first, last)) => last
                .parse::<usize>()
                .ok()?
                .checked_sub(first.parse().ok()?)?
                .checked_add(1),
            None => range.parse::<usize>().ok().map(|_| 1),
        })
        .sum::<Option<usize>>()
        .filter(|count| *count > 0)
}

impl Load {
    pub(crate) fn overloaded(&self) -> bool {
        self.one > self.cores as f64
    }

    /// What the deadline message adds: the load, and when it is above the
    /// core count, that the change is probably not at fault.
    pub(crate) fn note(&self) -> String {
        let mut note = format!(
            "machine load was {:.1} / {:.1} / {:.1} (1 / 5 / 15 min) on {} cores",
            self.one, self.five, self.fifteen, self.cores
        );
        if self.overloaded() {
            note.push_str(
                "; the machine was overloaded; this is probably not your change — rerun when it is quieter",
            );
        }
        note
    }
}

/// The note for `reader`'s load, empty when the load cannot be read.
pub(crate) fn note(reader: &Reader<'_>) -> String {
    reader().map(|load| load.note()).unwrap_or_default()
}
