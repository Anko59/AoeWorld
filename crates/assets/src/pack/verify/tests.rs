use super::*;

#[test]
fn manifest_parser_checks_counts_paths_and_frame_bounds_before_file_access() {
    let hash = "a".repeat(64);
    let mut value = serde_json::json!({
        "version":1,"converter":"test","input_hash":hash,"buildings":[],"building_data":"00",
        "pages":[{
            "color":"color.png","color_hash":hash,
            "player":"player.png","player_hash":hash,
            "shadow":"shadow.png","shadow_hash":hash,
            "outline":"outline.png","outline_hash":hash,
            "width":1024,"height":1024
        }],
        "frames":[{
            "source":"fixture","source_hash":hash,"frame":0,"page":0,
            "x":0,"y":0,"width":1,"height":1,"anchor_x":0,"anchor_y":0
        }]
    });
    let parse =
        |value: &serde_json::Value| parse_manifest(&serde_json::to_vec(value).expect("JSON"));
    assert_eq!(parse(&value).expect("valid manifest").frames.len(), 1);
    value["frames"][0]["x"] = 1024.into();
    assert!(parse(&value).is_err());
    value["frames"][0]["x"] = 0.into();
    value["pages"][0]["color"] = "../escape.png".into();
    assert!(parse(&value).is_err());
    value["pages"][0]["color"] = "color.png".into();
    let duplicate = value["frames"][0].clone();
    value["frames"]
        .as_array_mut()
        .expect("frames")
        .push(duplicate);
    assert!(parse(&value).is_err());
    value["version"] = 2.into();
    assert!(parse(&value).is_err());
    assert!(parse_manifest(&vec![0u8; 16 * 1024 * 1024 + 1]).is_err());
}
