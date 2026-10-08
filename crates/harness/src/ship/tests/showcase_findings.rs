use super::super::showcase::{Storyboard, resolve_out};

#[test]
fn showcase_output_name_matches_the_shipping_attachment_validator() {
    let root = tempfile::tempdir().unwrap();
    assert!(
        resolve_out(root.path(), ".cache/showcase/my take.webm")
            .err()
            .unwrap()
            .contains("filename")
    );
    let output = resolve_out(root.path(), ".cache/showcase/my-take_2.webm").unwrap();
    assert!(super::super::github::attachable_video(&output.path));
}

#[test]
fn showcase_check_refuses_newlines_in_concat_manifest_paths() {
    for separator in ['\n', '\r'] {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join(format!("checkout{separator}review"));
        std::fs::create_dir(&root).unwrap();
        let error = super::super::showcase::validate_manifest_root(&root).unwrap_err();
        assert!(error.contains("manifest paths"), "{error}");
    }

    let root = tempfile::tempdir().unwrap();
    assert!(super::super::showcase::validate_manifest_root(root.path()).is_ok());
    assert!(
        check_voice(
            "high",
            r#"{"title":"x","scenes":[{"kind":"card","heading":"x"}]}"#
        )
        .is_err()
    );
}

fn check_voice(level: &str, json: &str) -> Result<(), String> {
    let level = super::super::describe::Level::parse(level).unwrap();
    let board = Storyboard::parse(json)?;
    super::super::showcase::check_voice_requirement(level, &board)
}

#[test]
fn high_and_max_showcases_require_narration_but_low_and_medium_do_not() {
    let silent = r#"{"title":"x","scenes":[{"kind":"card","heading":"x"}]}"#;
    let voiced =
        r#"{"title":"x","scenes":[{"kind":"card","heading":"x","narration":"Say hello."}]}"#;
    for level in ["high", "max"] {
        assert!(
            check_voice(level, silent)
                .unwrap_err()
                .contains("narrated scene")
        );
        assert!(check_voice(level, voiced).is_ok());
    }
    for level in ["low", "medium"] {
        assert!(check_voice(level, silent).is_ok());
    }
}
