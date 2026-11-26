use crate::ci_report::chart::{Axis, Chart, Guide, Line, Mark, nice_ticks, render};
use crate::ci_report::format::{first_sentence, grouped, label, short_date, si, signed, stamp};
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

#[test]
fn numbers_and_names_read_for_people() {
    assert_eq!(si(9_696_224_729.0), "9.70G");
    assert_eq!(si(48_681_579.0), "48.7M");
    assert_eq!(si(183_229.0), "183k");
    assert_eq!(si(950.0), "950");
    assert_eq!(grouped(1_045_308), "1,045,308");
    assert_eq!(grouped(12), "12");
    assert_eq!(signed(1.24), "+1.2%");
    assert_eq!(signed(-3.46), "−3.5%");
    assert_eq!(signed(0.02), "0.0%");
    assert_eq!(label("simulation::late_game_x4_main"), "late game 4× main");
    assert_eq!(label("wasm_gzip"), "client bundle, gzip");
    assert_eq!(short_date("2026-11-25T12:32:38Z"), "25 Nov");
    assert_eq!(stamp("2026-11-05T09:07:00Z"), "5 Nov 09:07");
    assert_eq!(first_sentence("One. Two."), "One.");
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
fn a_chart_draws_one_path_per_segment_and_two_widths() {
    let svg = render(&chart(vec![line(
        "late game",
        vec![
            vec![(0.0, 10.0), (86_400.0, 12.0)],
            vec![(172_800.0, 50.0), (259_200.0, 52.0)],
        ],
    )]));
    assert_eq!(
        svg.matches("<svg").count(),
        2,
        "a wide and a narrow drawing"
    );
    assert_eq!(
        svg.matches(r#"class="line s0""#).count(),
        4,
        "two segments in each drawing"
    );
    assert!(svg.contains("late game"), "the legend names the line");
    assert!(svg.contains(r#"role="img""#) && svg.contains(r#"aria-label="A chart""#));
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
fn a_chart_with_no_points_says_so_instead_of_drawing_nothing() {
    let svg = render(&chart(vec![line("x", vec![])]));
    assert!(svg.contains("No measurement"));
    assert!(!svg.contains("<svg"));
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
fn the_headline_is_one_sentence_about_the_latest_measured_commit() {
    let data = series_fixture();
    let sentence = &data.headline;
    assert_eq!(
        sentence.matches(". ").count(),
        0,
        "one sentence: {sentence}"
    );
    assert!(sentence.ends_with('.'));
    assert!(sentence.contains("late game"), "{sentence}");
    assert!(sentence.contains("bundle"), "{sentence}");
    assert!(sentence.contains("#506"), "{sentence}");
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
