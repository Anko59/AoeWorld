use super::*;

#[test]
fn an_installed_site_runs_the_server_in_the_sdk_image_with_the_ai_command() {
    let root = tempfile::tempdir().expect("checkout");
    assert_eq!(
        container(root.path()),
        (super::super::RUST_TOOLS_IMAGE, vec![])
    );
    std::fs::create_dir_all(site(root.path()).join("aoeworld")).expect("site");
    let (image, arguments) = container(root.path());
    assert_eq!(image, SDK_IMAGE);
    assert!(arguments.contains(&"AOE_AI_COMMAND=python3 -m aoe_ai host".to_owned()));
    let path = format!(
        "PYTHONPATH={}:{}",
        site(root.path()).display(),
        root.path().join("ai").display()
    );
    assert!(arguments.contains(&path));
    assert!(arguments.chunks(2).all(|pair| pair[0] == "-e"));
}
