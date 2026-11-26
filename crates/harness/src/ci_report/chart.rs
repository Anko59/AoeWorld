//! Line charts as inline SVG. Each chart is drawn twice, a wide and a narrow
//! drawing, and the page's style shows the one that fits the screen, so text
//! keeps its size at 400 px. A few lines of inline script (`shell.html`) show
//! the values under the pointer; the numbers ride along as JSON in the figure.
use super::calendar::iso;
use super::format::{escape, short_date, si};
use std::fmt::Write;

#[derive(Clone, Copy, Debug)]
pub(super) enum Axis {
    Count,
    Percent,
    Minutes,
}

/// One line: segments of (seconds since 1970, value), drawn apart.
pub(super) struct Line {
    pub(super) label: String,
    pub(super) segments: Vec<Vec<(f64, f64)>>,
}

/// A vertical marker with a letter (an event on that commit).
pub(super) struct Mark {
    pub(super) at: f64,
    pub(super) label: String,
    pub(super) dashed: bool,
}

/// A horizontal reference line.
pub(super) struct Guide {
    pub(super) value: f64,
    pub(super) label: String,
}

pub(super) struct Chart {
    pub(super) id: String,
    pub(super) about: String,
    pub(super) axis: Axis,
    /// A value holds until the next measurement (commits) rather than sloping to it (days).
    pub(super) step: bool,
    pub(super) lines: Vec<Line>,
    pub(super) marks: Vec<Mark>,
    pub(super) guides: Vec<Guide>,
    /// What to say at each x: (seconds, text with `\n` between lines).
    pub(super) tips: Vec<(f64, String)>,
}

const DAY: f64 = 86_400.0;
const TOP: f64 = 36.0;
const BOTTOM: f64 = 24.0;
const RIGHT: f64 = 10.0;

/// Round tick values around `lo..hi`, about `target` of them, stepping by 1, 2
/// or 5 times a power of ten, the first at or below `lo`, the last at or above `hi`.
pub(super) fn nice_ticks(lo: f64, hi: f64, target: usize) -> Vec<f64> {
    let hi = if hi > lo {
        hi
    } else {
        lo + (lo.abs() * 0.1).max(1.0)
    };
    let raw = (hi - lo) / target.max(1) as f64;
    let magnitude = 10f64.powf(raw.log10().floor());
    let normal = raw / magnitude;
    let step = magnitude
        * match normal {
            n if n <= 1.0 => 1.0,
            n if n <= 2.0 => 2.0,
            n if n <= 5.0 => 5.0,
            _ => 10.0,
        };
    let start = (lo / step).floor();
    let count = ((hi / step).ceil() - start) as usize;
    (0..=count).map(|i| (start + i as f64) * step).collect()
}

fn axis_text(axis: Axis, value: f64) -> String {
    match axis {
        Axis::Count => {
            if value == 0.0 {
                "0".to_owned()
            } else {
                si(value)
            }
        }
        Axis::Percent if value > 0.0 => format!("+{value}%"),
        Axis::Percent if value < 0.0 => format!("−{}%", -value),
        Axis::Percent => "0%".to_owned(),
        Axis::Minutes => format!("{value}"),
    }
}

struct Frame {
    width: f64,
    height: f64,
    left: f64,
    x: (f64, f64),
    y: (f64, f64),
}

impl Frame {
    fn px(&self, at: f64) -> f64 {
        let span = (self.x.1 - self.x.0).max(1.0);
        self.left + (at - self.x.0) / span * (self.width - self.left - RIGHT)
    }

    fn py(&self, value: f64) -> f64 {
        let span = (self.y.1 - self.y.0).max(f64::MIN_POSITIVE);
        self.height - BOTTOM - (value - self.y.0) / span * (self.height - BOTTOM - TOP)
    }
}

fn path(chart: &Chart, frame: &Frame, points: &[(f64, f64)]) -> String {
    let mut d = String::new();
    for (index, &(at, value)) in points.iter().enumerate() {
        let (x, y) = (frame.px(at), frame.py(value));
        let _ = match (index, chart.step) {
            (0, _) => write!(d, "M{x:.1} {y:.1}"),
            (_, true) => write!(d, "H{x:.1}V{y:.1}"),
            (_, false) => write!(d, "L{x:.1} {y:.1}"),
        };
    }
    d
}

fn x_ticks(frame: &Frame, count: usize) -> Vec<f64> {
    let span = frame.x.1 - frame.x.0;
    let step = if span < 2.0 * DAY {
        DAY / 4.0
    } else {
        [1.0, 2.0, 3.0, 7.0, 14.0, 28.0]
            .into_iter()
            .map(|days| days * DAY)
            .find(|step| span / step <= count as f64)
            .unwrap_or(56.0 * DAY)
    };
    let mut at = (frame.x.0 / step).ceil() * step;
    let mut ticks = Vec::new();
    while at <= frame.x.1 {
        ticks.push(at);
        at += step;
    }
    ticks
}

fn x_text(at: f64, short: bool) -> String {
    let stamp = iso(at as i64);
    if short {
        stamp[11..16].to_owned()
    } else {
        short_date(&stamp)
    }
}

fn drawing(chart: &Chart, frame: &Frame, ticks: &[f64], x_count: usize, class: &str) -> String {
    let mut svg = String::new();
    let (left, right) = (frame.left, frame.width - RIGHT);
    let (top, base) = (TOP, frame.height - BOTTOM);
    let xs: Vec<String> = chart
        .tips
        .iter()
        .map(|(at, _)| format!("{:.1}", frame.px(*at)))
        .collect();
    let _ = write!(
        svg,
        r#"<svg class="{class}" viewBox="0 0 {w} {h}" role="img" aria-label="{about}" data-w="{w}" data-top="{top}" data-base="{base}" data-xs="{xs}">"#,
        w = frame.width,
        h = frame.height,
        about = escape(&chart.about),
        xs = xs.join(","),
    );
    for tick in ticks {
        let y = frame.py(*tick);
        let _ = write!(
            svg,
            r#"<line class="grid" x1="{left}" x2="{right}" y1="{y:.1}" y2="{y:.1}"/>"#
        );
        let _ = write!(
            svg,
            r#"<text class="ax" x="{:.1}" y="{y:.1}" dy="0.32em" text-anchor="end">{}</text>"#,
            left - 6.0,
            axis_text(chart.axis, *tick)
        );
    }
    let short = frame.x.1 - frame.x.0 < 2.0 * DAY;
    for at in x_ticks(frame, x_count) {
        let _ = write!(
            svg,
            r#"<text class="ax" x="{:.1}" y="{:.1}" text-anchor="middle">{}</text>"#,
            frame.px(at),
            base + 16.0,
            x_text(at, short)
        );
    }
    let zero = if matches!(chart.axis, Axis::Percent) {
        0.0
    } else {
        frame.y.0
    };
    let _ = write!(
        svg,
        r#"<line class="base" x1="{left}" x2="{right}" y1="{0:.1}" y2="{0:.1}"/>"#,
        frame.py(zero)
    );
    for guide in &chart.guides {
        let y = frame.py(guide.value);
        let _ = write!(
            svg,
            r#"<line class="guide" x1="{left}" x2="{right}" y1="{y:.1}" y2="{y:.1}"/>"#
        );
        let _ = write!(
            svg,
            r#"<text class="gl" x="{:.1}" y="{:.1}">{}</text>"#,
            left + 4.0,
            y - 4.0,
            escape(&guide.label)
        );
    }
    let (mut last, mut row) = (f64::MIN, 0);
    for mark in chart
        .marks
        .iter()
        .filter(|m| m.at >= frame.x.0 && m.at <= frame.x.1)
    {
        let x = frame.px(mark.at);
        row = if x - last < 14.0 { (row + 1) % 3 } else { 0 };
        last = x;
        let dash = if mark.dashed { "5 4" } else { "1 4" };
        let _ = write!(
            svg,
            r#"<line class="mark" x1="{x:.1}" x2="{x:.1}" y1="{:.1}" y2="{base:.1}" stroke-dasharray="{dash}"/><text class="mk" x="{x:.1}" y="{:.1}" text-anchor="middle">{}</text>"#,
            top - 4.0,
            top - 9.0 - f64::from(row) * 11.0,
            escape(&mark.label)
        );
    }
    for (index, line) in chart.lines.iter().enumerate() {
        for segment in line.segments.iter().filter(|segment| !segment.is_empty()) {
            if segment.len() == 1 {
                let _ = write!(
                    svg,
                    r#"<circle class="dot s{}" r="3" cx="{:.1}" cy="{:.1}"/>"#,
                    index % 8,
                    frame.px(segment[0].0),
                    frame.py(segment[0].1)
                );
            } else {
                let _ = write!(
                    svg,
                    r#"<path class="line s{}" d="{}"/>"#,
                    index % 8,
                    path(chart, frame, segment)
                );
            }
        }
    }
    let _ = write!(
        svg,
        r#"<line class="cross" y1="{top}" y2="{base}" x1="0" x2="0" style="display:none"/><rect class="hit" x="{left}" y="{top}" width="{:.1}" height="{:.1}"/></svg>"#,
        right - left,
        base - top
    );
    svg
}

/// The figure: legend, wide and narrow drawings, tooltip and the tips as JSON.
pub(super) fn render(chart: &Chart) -> String {
    let points = || chart.lines.iter().flat_map(|l| l.segments.iter().flatten());
    if points().next().is_none() {
        return r#"<p class="empty">No measurement in this period yet.</p>"#.to_owned();
    }
    let mut xs: Vec<f64> = points()
        .map(|p| p.0)
        .chain(chart.tips.iter().map(|t| t.0))
        .collect();
    xs.sort_by(f64::total_cmp);
    let x = (xs[0], xs[xs.len() - 1].max(xs[0] + DAY));
    let mut ys: Vec<f64> = points()
        .map(|p| p.1)
        .chain(chart.guides.iter().map(|g| g.value))
        .collect();
    ys.sort_by(f64::total_cmp);
    let (mut lo, mut hi) = (ys[0], ys[ys.len() - 1]);
    match chart.axis {
        Axis::Percent => (lo, hi) = (lo.min(0.0), hi.max(0.0)),
        Axis::Minutes => lo = 0.0,
        Axis::Count if lo >= 0.0 && lo < hi * 0.5 => lo = 0.0,
        Axis::Count => {}
    }
    let pad = (hi - lo).max(hi.abs() * 0.02).max(1.0) * 0.06;
    let ticks = nice_ticks(if lo == 0.0 { 0.0 } else { lo - pad }, hi + pad, 5);
    let y = (ticks[0], ticks[ticks.len() - 1]);
    let label_width = ticks
        .iter()
        .map(|tick| axis_text(chart.axis, *tick).chars().count())
        .max()
        .unwrap_or(2) as f64;
    let left = (label_width * 6.6 + 12.0).ceil();
    let wide = Frame {
        width: 760.0,
        height: 300.0,
        left,
        x,
        y,
    };
    let narrow = Frame {
        width: 400.0,
        height: 280.0,
        left,
        x,
        y,
    };

    let mut html = format!(
        r#"<figure class="chart" id="{}"><ul class="legend">"#,
        escape(&chart.id)
    );
    for (index, line) in chart.lines.iter().enumerate() {
        let _ = write!(
            html,
            r#"<li><span class="sw s{}"></span>{}</li>"#,
            index % 8,
            escape(&line.label)
        );
    }
    html.push_str(r#"</ul><div class="plot">"#);
    html.push_str(&drawing(chart, &wide, &ticks, 8, "wide"));
    html.push_str(&drawing(chart, &narrow, &ticks, 4, "narrow"));
    html.push_str(r#"<div class="tip" hidden></div></div>"#);
    let tips: Vec<(f64, &str)> = chart
        .tips
        .iter()
        .map(|(at, text)| (*at, text.as_str()))
        .collect();
    let json = serde_json::to_string(&tips)
        .unwrap_or_else(|_| "[]".to_owned())
        .replace('<', "\\u003c");
    let _ = write!(
        html,
        r#"<script type="application/json" class="tips">{json}</script></figure>"#
    );
    html
}
