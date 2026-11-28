use crate::ci_report::chart::{Axis, Chart, Guide, Line, Mark, nice_ticks, render};
use crate::ci_report::page;
use crate::ci_report::tests::series_fixture;

#[test]
fn nice_ticks_step_by_one_two_or_five_times_a_power_of_ten() {
    assert_eq!(nice_ticks(0.0, 10.0, 5), [0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
    assert_eq!(
        nice_ticks(3.0, 97.0, 5),
        [0.0, 20.0, 40.0, 60.0, 80.0, 100.0]
    );
    assert_eq!(
        nice_ticks(-8.0, 12.0, 4),
        [-10.0, -5.0, 0.0, 5.0, 10.0, 15.0]
    );
    let big = nice_ticks(7.2e9, 9.9e9, 5);
    assert!(
        big.first().unwrap() <= &7.2e9 && big.last().unwrap() >= &9.9e9,
        "{big:?}"
    );
    assert!(
        nice_ticks(5.0, 5.0, 4).len() >= 2,
        "a flat series still has a scale"
    );
}

fn chart(lines: Vec<Line>) -> Chart {
    Chart {
        id: "c".to_owned(),
        about: "A chart".to_owned(),
        axis: Axis::Count,
        step: true,
        lines,
        marks: Vec::new(),
        guides: Vec::new(),
        tips: vec![
            (0.0, "first".to_owned()),
            (86_400.0, "second <b>".to_owned()),
        ],
    }
}

fn line(label: &str, segments: Vec<Vec<(f64, f64)>>) -> Line {
    Line {
        label: label.to_owned(),
        segments,
    }
}

#[test]
fn a_lone_point_is_a_dot_and_marks_and_guides_are_drawn_with_their_labels() {
    let mut c = chart(vec![line(
        "bundle",
        vec![
            vec![(0.0, 400.0)],
            vec![(86_400.0, 450.0), (172_800.0, 460.0)],
        ],
    )]);
    c.marks = vec![
        Mark {
            at: 43_200.0,
            label: "A".to_owned(),
            dashed: true,
        },
        Mark {
            at: 9e12,
            label: "Z".to_owned(),
            dashed: false,
        },
    ];
    c.guides = vec![Guide {
        value: 470.0,
        label: "Limit 470".to_owned(),
    }];
    let svg = render(&c);
    assert!(svg.contains("<circle"), "a one-point segment is a dot");
    assert!(
        svg.contains(">A<") && !svg.contains(">Z<"),
        "a mark outside the chart is left out"
    );
    assert!(svg.contains("Limit 470"));
}

#[test]
fn the_tips_ride_along_as_json_that_cannot_close_the_script() {
    let svg = render(&chart(vec![line(
        "x",
        vec![vec![(0.0, 1.0), (86_400.0, 2.0)]],
    )]));
    assert!(svg.contains(r#"<script type="application/json" class="tips">"#));
    assert!(svg.contains("second \\u003cb>"), "{svg}");
    assert!(!svg.contains("second <b>"));
}

#[test]
fn the_page_is_one_self_contained_file_with_every_section() {
    let html = page::render(&series_fixture());
    for heading in [
        "Late-game kernel",
        "Small worlds",
        "Single operations",
        "Client bundle",
        "CI time",
        "Where each gated row stands",
        "Every move of 2% or more",
        "Reliability",
    ] {
        assert!(html.contains(heading), "missing section {heading:?}");
    }
    // Nothing is fetched: no external script, style, image, font or import.
    for forbidden in [
        "<script src",
        "<link",
        "@import",
        "<img",
        "url(http",
        "src=\"http",
        "fetch(",
    ] {
        assert!(!html.contains(forbidden), "the page loads {forbidden:?}");
    }
    assert!(html.contains("prefers-color-scheme: dark"));
    assert!(html.contains("<meta name=\"viewport\""));
    assert!(
        html.contains("https://github.com/o/r/pull/504"),
        "a move links to its pull request"
    );
}

#[test]
fn an_empty_report_still_renders() {
    let mut data = series_fixture();
    data.dev.clear();
    data.standing.clear();
    data.moves.clear();
    data.events.clear();
    data.ci_days.clear();
    data.ci_runs.clear();
    data.reliability = None;
    data.headline = page::headline(&data);
    let html = page::render(&data);
    assert!(html.contains("No dev commit has been measured yet"));
    assert!(
        html.contains("Reliability"),
        "the section says why it has no numbers"
    );
}
