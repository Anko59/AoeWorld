//! Markdown table rows of the roadmap files. A table is a roadmap table when its header has a
//! `status` column; the item is the first column, `evidence` and `next step` are found by name.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Status {
    Done,
    Partial,
    InFlight,
    Planned,
    Dropped,
    Superseded,
    Unverified,
    Unknown,
}

impl Status {
    pub(super) const ALL: [Status; 7] = [
        Status::Done,
        Status::Partial,
        Status::InFlight,
        Status::Planned,
        Status::Dropped,
        Status::Superseded,
        Status::Unverified,
    ];

    fn from_cell(cell: &str) -> Status {
        // No label is a prefix of another, so the first match is the only one.
        for status in Status::ALL {
            if cell.starts_with(status.label()) {
                return status;
            }
        }
        Status::Unknown
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Status::Done => "DONE",
            Status::Partial => "PARTIAL",
            Status::InFlight => "IN FLIGHT",
            Status::Planned => "PLANNED",
            Status::Dropped => "DROPPED",
            Status::Superseded => "SUPERSEDED",
            Status::Unverified => "UNVERIFIED",
            Status::Unknown => "?",
        }
    }

    pub(super) fn is_open(self) -> bool {
        !matches!(self, Status::Done | Status::Dropped | Status::Superseded)
    }
}

pub(super) struct Row {
    pub(super) area: String,
    pub(super) line: usize,
    pub(super) item: String,
    pub(super) status: Status,
    pub(super) evidence: String,
    pub(super) next: String,
}

fn cells(line: &str) -> Vec<String> {
    let inner = line.trim().trim_start_matches('|').trim_end_matches('|');
    inner
        .split('|')
        .map(|cell| cell.trim().to_owned())
        .collect()
}

pub(super) fn parse(area: &str, text: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    // Column positions of the table being read, `None` outside a roadmap table.
    let mut columns: Option<(usize, Option<usize>, Option<usize>)> = None;
    let mut in_table = false;
    for (index, line) in text.lines().enumerate() {
        if !line.trim_start().starts_with('|') {
            columns = None;
            in_table = false;
            continue;
        }
        let cells = cells(line);
        if !in_table {
            in_table = true;
            let find = |name: &str| {
                cells
                    .iter()
                    .position(|cell| cell.eq_ignore_ascii_case(name))
            };
            columns = find("status").map(|status| (status, find("evidence"), find("next step")));
            continue;
        }
        let Some((status, evidence, next)) = columns else {
            continue;
        };
        if cells
            .iter()
            .all(|cell| cell.chars().all(|c| matches!(c, '-' | ':' | ' ')))
        {
            continue;
        }
        let get = |position: Option<usize>| {
            position
                .and_then(|p| cells.get(p))
                .cloned()
                .unwrap_or_default()
        };
        rows.push(Row {
            area: area.to_owned(),
            line: index + 1,
            item: cells.first().cloned().unwrap_or_default(),
            status: Status::from_cell(cells.get(status).map_or("", String::as_str)),
            evidence: get(evidence),
            next: get(next),
        });
    }
    rows
}

/// Numbers written as `#123` in a cell, in order, without duplicates.
pub(super) fn pull_requests(text: &str) -> Vec<u64> {
    let mut numbers = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find('#') {
        rest = &rest[at + 1..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(number) = digits.parse::<u64>()
            && !numbers.contains(&number)
        {
            numbers.push(number);
        }
    }
    numbers
}
