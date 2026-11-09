//! A reader for Callgrind's output (`callgrind.out`): the self cost of each
//! function and the inclusive cost of each call edge, in every event it counted.
use std::collections::BTreeMap;

/// Event counts in the order of the file's `events:` line.
pub(super) type Cost = Vec<u64>;

#[derive(Debug, Default)]
pub(super) struct Profile {
    pub(super) events: Vec<String>,
    /// The `desc:` lines that describe the simulated caches.
    pub(super) caches: Vec<String>,
    pub(super) totals: Cost,
    pub(super) own: BTreeMap<String, Cost>,
    /// Inclusive cost of the calls from the first function to the second.
    pub(super) edges: BTreeMap<(String, String), Cost>,
}

fn add(into: &mut Cost, cost: &[u64]) {
    if into.len() < cost.len() {
        into.resize(cost.len(), 0);
    }
    for (sum, value) in into.iter_mut().zip(cost) {
        *sum += value;
    }
}

/// `(12) name` defines id 12, `(12)` repeats it.
fn name_of(names: &mut BTreeMap<String, String>, spec: &str) -> String {
    let spec = spec.trim();
    match spec.split_once(')') {
        Some((id, rest)) => {
            let id = id.trim_start_matches('(').to_owned();
            let name = rest.trim();
            if name.is_empty() {
                names.get(&id).cloned().unwrap_or_default()
            } else {
                names.insert(id, name.to_owned());
                name.to_owned()
            }
        }
        None => spec.to_owned(),
    }
}

pub(super) fn parse(text: &str) -> Result<Profile, String> {
    let mut profile = Profile::default();
    let mut positions = 1;
    let (mut names, mut current, mut callee) = (BTreeMap::new(), String::new(), String::new());
    let mut call_cost = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("events:") {
            profile.events = rest.split_whitespace().map(str::to_owned).collect();
        } else if let Some(rest) = line.strip_prefix("positions:") {
            positions = rest.split_whitespace().count();
        } else if let Some(rest) = line.strip_prefix("desc:") {
            if rest.contains("cache:") {
                profile.caches.push(rest.trim().to_owned());
            }
        } else if let Some(rest) = line.strip_prefix("totals:") {
            profile.totals = numbers(rest.split_whitespace())?;
        } else if let Some(rest) = line.strip_prefix("fn=") {
            current = name_of(&mut names, rest);
        } else if let Some(rest) = line.strip_prefix("cfn=") {
            callee = name_of(&mut names, rest);
        } else if line.starts_with("calls=") {
            call_cost = true;
        } else if line.starts_with(|c: char| c.is_ascii_digit() || "+-*".contains(c)) {
            let cost = numbers(line.split_whitespace().skip(positions))?;
            if call_cost {
                let key = (current.clone(), callee.clone());
                add(profile.edges.entry(key).or_default(), &cost);
                call_cost = false;
            } else {
                add(profile.own.entry(current.clone()).or_default(), &cost);
            }
        }
    }
    if profile.events.is_empty() || profile.totals.is_empty() {
        return Err("not a Callgrind profile: no events or totals line".to_owned());
    }
    Ok(profile)
}

fn numbers<'a>(tokens: impl Iterator<Item = &'a str>) -> Result<Cost, String> {
    tokens
        .map(|token| {
            token
                .parse()
                .map_err(|error| format!("bad cost `{token}`: {error}"))
        })
        .collect()
}

/// The function name without generic arguments, for matching: closures and
/// other functions of the same prefix stay apart.
fn plain(name: &str) -> &str {
    name.find("::<").map_or(name, |at| &name[..at])
}

/// Whether a function is `pattern`, written as its trailing path (`GameWorld>::step`).
pub(super) fn is(name: &str, pattern: &str) -> bool {
    plain(name).ends_with(pattern)
}

impl Profile {
    /// Inclusive cost of the functions named `pattern`: their own cost and
    /// that of everything they call. `None` when none ran.
    pub(super) fn inclusive(&self, pattern: &str) -> Option<Cost> {
        let mut sum: Option<Cost> = None;
        for (name, cost) in self.own.iter().filter(|(name, _)| is(name, pattern)) {
            add(sum.get_or_insert_with(Vec::new), cost);
            for ((caller, _), call) in &self.edges {
                if caller == name {
                    add(sum.get_or_insert_with(Vec::new), call);
                }
            }
        }
        sum
    }

    /// Inclusive cost of the calls from `caller` to `callee`.
    pub(super) fn edge(&self, caller: &str, callee: &str) -> Option<Cost> {
        let mut sum: Option<Cost> = None;
        for ((from, to), cost) in &self.edges {
            if is(from, caller) && is(to, callee) {
                add(sum.get_or_insert_with(Vec::new), cost);
            }
        }
        sum
    }

    pub(super) fn event(&self, name: &str) -> Result<usize, String> {
        self.events
            .iter()
            .position(|event| event == name)
            .ok_or_else(|| format!("the profile counted no {name} events (is --cache-sim on?)"))
    }
}
