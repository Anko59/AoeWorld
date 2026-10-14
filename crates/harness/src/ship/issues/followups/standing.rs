//! Preserve the fresh body while folding its oldest sections (docs/issues.md).
use super::{
    BODY_LIMIT, FOLDED, INTRO, MARKER, MAX_SECTIONS, SECTION_HEADING, STANDING_TITLE, Section,
    body_budget,
};
use std::ops::Range;

pub(super) fn update(current: &str, section: &Section) -> String {
    let mut body = splice(current, section);
    loop {
        let mut standing = Standing::parse(&body);
        let limit = if standing.marked {
            BODY_LIMIT
        } else {
            body_budget(STANDING_TITLE)
        };
        if standing.sections.len() <= 1
            || (standing.sections.len() <= MAX_SECTIONS && body.chars().count() <= limit)
        {
            return body;
        }
        let (pr, oldest) = &standing.sections[0];
        standing.folded += 1;
        standing.last_folded = *pr;
        let folded = standing.folded;
        let last_folded = standing.last_folded;
        let noun = if folded == 1 { "section" } else { "sections" };
        let summary = format!("{FOLDED}{folded} earlier review {noun}, up to #{last_folded}.");
        body.replace_range(oldest.clone(), "");
        if let Some(range) = standing.summary {
            body.replace_range(range, &summary);
        } else {
            body.insert_str(oldest.start, &format!("{summary}\n\n"));
        }
    }
}

/// Byte ranges let folding remove only the oldest section and update only
/// the single summary line, retaining owner text, ticks, whitespace and marker.
#[derive(Default)]
struct Standing {
    folded: u64,
    last_folded: u64,
    summary: Option<Range<usize>>,
    sections: Vec<(u64, Range<usize>)>,
    marked: bool,
}

impl Standing {
    fn parse(body: &str) -> Self {
        let mut standing = Self::default();
        let mut section: Option<(u64, usize)> = None;
        let mut offset = 0;
        for line in body.split_inclusive('\n') {
            let heading = line
                .strip_prefix(SECTION_HEADING)
                .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
                .and_then(|number| number.parse().ok());
            if (heading.is_some() || line.starts_with(MARKER))
                && let Some((pr, start)) = section.take()
            {
                standing.sections.push((pr, start..offset));
            }
            if let Some(pr) = heading {
                section = Some((pr, offset));
            } else if line.starts_with(MARKER) {
                standing.marked = true;
            } else if standing.sections.is_empty()
                && section.is_none()
                && let Some(summary) = line.strip_prefix(FOLDED)
            {
                standing.folded = summary
                    .split_whitespace()
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                standing.last_folded = summary
                    .split_once('#')
                    .and_then(|(_, n)| n.split(|c: char| !c.is_ascii_digit()).next())
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                standing.summary = Some(offset..offset + line.trim_end_matches('\n').len());
            }
            offset += line.len();
        }
        if let Some((pr, start)) = section {
            standing.sections.push((pr, start..body.len()));
        }
        standing
    }
}

fn splice(current: &str, section: &Section) -> String {
    if current.is_empty() && !section.items.is_empty() {
        return format!("{INTRO}\n\n{}\n", section.render());
    }
    let mut offset = 0;
    let mut start = None;
    let mut end = current.len();
    let mut insertion = current.len();
    for line in current.split_inclusive('\n') {
        let pr = line.strip_prefix(SECTION_HEADING).and_then(|rest| {
            rest.split(|c: char| !c.is_ascii_digit())
                .next()?
                .parse::<u64>()
                .ok()
        });
        if line.starts_with(MARKER) {
            insertion = offset;
        }
        if start.is_some() && (pr.is_some() || line.starts_with(MARKER)) {
            end = offset;
            break;
        }
        if pr == Some(section.pr) {
            start = Some(offset);
        }
        offset += line.len();
    }
    if let Some(start) = start {
        // Separating whitespace belongs to the surrounding body.
        let end = start + current[start..end].trim_end().len();
        let replacement = if section.items.is_empty() {
            String::new()
        } else {
            section.render()
        };
        format!("{}{replacement}{}", &current[..start], &current[end..])
    } else if section.items.is_empty() {
        current.to_owned()
    } else {
        format!(
            "{}\n{}\n\n{}",
            &current[..insertion],
            section.render(),
            &current[insertion..]
        )
    }
}
