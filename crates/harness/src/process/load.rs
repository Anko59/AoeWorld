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
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    parse(&fs::read_to_string("/proc/loadavg").ok()?, cores)
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
