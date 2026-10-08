use super::super::describe::{Level, Template, parse_probe};

const GOOD: &str = "<!-- level: medium -->\n## Why\n> \"I don't review code\" — the user\nAgents publish unchecked code today.\n## What\nmake ship gates every publish.\n## For AI\nDesign notes.\n";

#[test]
fn a_well_formed_description_parses() {
    let template = Template::parse(GOOD).unwrap();
    assert_eq!(template.level, Level::Medium);
    assert!(template.why.starts_with("> "));
    assert_eq!(template.what, "make ship gates every publish.");
    assert_eq!(template.for_ai, "Design notes.");
}

#[test]
fn descriptions_accept_crlf_and_only_supported_grounding() {
    assert!(Template::parse(&GOOD.replace('\n', "\r\n")).is_ok());
    for grounding in [
        "> request quote",
        "See [the report](docs/report.md)",
        "Reference: https://example.com/report",
        "Evidence: https://example.com/report",
        "![screenshot](image.png)",
        "Fixes #123",
        "Latency measured at 42 ms",
    ] {
        let body = GOOD.replace("> \"I don't review code\" — the user", grounding);
        assert!(Template::parse(&body).is_ok(), "{grounding}");
    }
    for prose in ["This fixes an http parsing bug.", "Our http cache is slow."] {
        let body = GOOD.replace("> \"I don't review code\" — the user", prose);
        assert!(
            Template::parse(&body)
                .unwrap_err()
                .contains("grounding element")
        );
    }
}

#[test]
fn the_human_section_limits_are_enforced() {
    let no_level = GOOD.replace("<!-- level: medium -->\n", "");
    assert!(Template::parse(&no_level).unwrap_err().contains("level"));
    let no_element = GOOD.replace("> \"I don't review code\" — the user\n", "");
    assert!(
        Template::parse(&no_element)
            .unwrap_err()
            .contains("grounding element")
    );
    let issue = GOOD.replace("> \"I don't review code\" — the user", "Fixes #116");
    assert!(
        Template::parse(&issue).is_ok(),
        "an issue number grounds the why"
    );
    for incomplete in [
        "See [reference](",
        "See [reference]()",
        "See [reference](   )",
    ] {
        let body = GOOD.replace("> \"I don't review code\" — the user", incomplete);
        assert!(
            Template::parse(&body)
                .unwrap_err()
                .contains("grounding element"),
            "incomplete Markdown link accepted: {incomplete}"
        );
    }
    let two_grounding_lines = GOOD.replace(
        "Agents publish unchecked code today.",
        "> second quote\nAgents publish unchecked code today.",
    );
    assert!(
        Template::parse(&two_grounding_lines)
            .unwrap_err()
            .contains("split")
    );
    let valid_why = GOOD.replace("Agents publish unchecked code today.", "One.\nTwo.");
    assert!(Template::parse(&valid_why).is_ok());
    let long_why = GOOD.replace(
        "Agents publish unchecked code today.\n",
        "One.\nTwo.\nThree.\n",
    );
    assert!(Template::parse(&long_why).unwrap_err().contains("split"));
    let long_what = GOOD.replace("make ship gates every publish.\n", "One.\nTwo.\nThree.\n");
    assert!(Template::parse(&long_what).unwrap_err().contains("split"));
}

#[test]
fn levels_fit_the_change_and_the_video() {
    let medium = Template::parse(GOOD).unwrap();
    assert!(medium.check(500, Some((48, false))).is_ok());
    assert!(
        medium
            .check(500, None)
            .unwrap_err()
            .contains("needs a showcase video")
    );
    assert!(
        medium
            .check(500, Some((61, false)))
            .unwrap_err()
            .contains("allows 60s")
    );
    let high = Template::parse(&GOOD.replace("medium", "high")).unwrap();
    assert!(
        high.check(500, Some((100, false)))
            .unwrap_err()
            .contains("voice-over")
    );
    assert!(high.check(500, Some((100, true))).is_ok());
    let low = Template::parse(&GOOD.replace("medium", "low")).unwrap();
    assert!(low.check(20, None).is_ok(), "a tiny PR may skip the video");
    assert_eq!(
        low.check(20, Some((1, false))).unwrap_err(),
        "low PRs have no video; use medium or higher"
    );
    assert!(low.check(300, None).unwrap_err().contains("smallest PRs"));
}

#[test]
fn ffmpeg_banners_give_duration_and_sound() {
    let silent = "Input #0, matroska,webm, from '/video':\n  Duration: 00:00:48.60, start: 0.000000, bitrate: 282 kb/s\n  Stream #0:0: Video: vp8";
    assert_eq!(parse_probe(silent), Some((49, false)));
    let voiced = "Input #0, matroska, from '/video':\n  Duration: 00:01:59.00, start\n  Stream #0:0: Video: vp8\n  Stream #0:1: Audio: opus, 48000 Hz";
    assert_eq!(parse_probe(voiced), Some((119, true)));
    let metadata_only = "Input #0, matroska, from '/video':\n  Metadata: comment=Duration: 00:00:01.00, Audio:\n  Duration: 00:02:01.00, start\n  Stream #0:0: Video: vp8";
    assert_eq!(parse_probe(metadata_only), Some((121, false)));
    assert_eq!(
        parse_probe(
            "Input #0, wav, from '/audio':\n  Duration: 00:00:10.00\n  Stream #0:0: Audio: pcm"
        ),
        None
    );
    assert_eq!(parse_probe("/video: Invalid data found"), None);
}

#[test]
fn gh_uploaded_video_is_moved_into_the_what_section() {
    let before = "# 🧑 For humans\n\n### 🎯 Why\n\nBecause.\n\n### 🛠️ What\n\n🎬 ![Showcase](./demo.mp4)\n\n### ✅ How\n\nReview.\n";
    let after =
        format!("{before}\n![demo.mp4](https://github.com/user-attachments/assets/abc123)\n");
    let updated = super::super::describe::place_video_in_what(before, &after);
    assert!(
        updated.contains("### 🛠️ What\n\n🎬 Showcase:\n\nhttps://github.com/user-attachments/assets/abc123\n\n### ✅ How"),
        "{updated}"
    );
    assert_eq!(updated.matches("user-attachments/assets/abc123").count(), 1);
}

#[test]
fn attachment_diff_preserves_old_screenshot_and_only_uses_new_video() {
    let screenshot = "https://github.com/user-attachments/assets/screenshot";
    let video = "https://github.com/user-attachments/assets/video";
    let before = format!("### 🎯 Why\n\n![screenshot]({screenshot})\n\n### 🛠️ What\n\nChange.\n");
    let after = format!("{before}\n![video]({video})\n");
    let updated = super::super::describe::place_video_in_what(&before, &after);
    assert!(
        updated.contains(&format!("![screenshot]({screenshot})")),
        "{updated}"
    );
    assert!(
        updated.contains(&format!("🎬 Showcase:\n\n{video}\n\n")),
        "{updated}"
    );
    assert_eq!(updated.matches(screenshot).count(), 1);
    assert_eq!(updated.matches(video).count(), 1);
    assert!(!updated.contains(&format!("🎬 Showcase:\n{screenshot}")));
}

#[test]
fn preexisting_attachments_are_not_rewritten_as_the_uploaded_video() {
    let before =
        "### 🛠️ What\n\nChange.\n\n![old](https://github.com/user-attachments/assets/existing)\n";
    let updated = super::super::describe::place_video_in_what(before, before);
    assert_eq!(updated, before);
}

#[test]
fn ship_video_extensions_match_github_attachment_formats() {
    use std::path::Path;
    for path in ["clip.mp4", "clip.MOV", "clip.webm"] {
        assert!(super::super::github::attachable_video(Path::new(path)));
    }
    for path in ["clip.mkv", "clip.avi", "clip"] {
        assert!(!super::super::github::attachable_video(Path::new(path)));
    }
}

#[test]
fn an_empty_what_or_an_empty_quote_is_refused() {
    let empty_what = GOOD.replace("make ship gates every publish.\n", "");
    assert!(
        Template::parse(&empty_what)
            .unwrap_err()
            .contains("`## What` is empty")
    );
    let empty_quote = GOOD.replace("> \"I don't review code\" — the user", ">");
    assert!(
        Template::parse(&empty_quote)
            .unwrap_err()
            .contains("grounding element")
    );
}

#[test]
fn only_safely_named_videos_are_attached() {
    use super::super::github::attachable_video;
    use std::path::Path;
    assert!(attachable_video(Path::new("/tmp/showcase-1.webm")));
    assert!(attachable_video(Path::new("demo.MP4")));
    for bad in [
        "demo#alt.mp4",
        "my demo.mp4",
        "x.mp4)\n\n### How.mp4",
        "clip.gif",
    ] {
        assert!(!attachable_video(Path::new(bad)), "{bad:?}");
    }
}
