use std::{fs, path::Path, process::Command};

#[test]
fn import_de_keeps_pack_metacharacters_in_one_argument_and_preserves_failure() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let makefile = fs::read_to_string(root.join("Makefile")).unwrap();
    let quote = makefile
        .lines()
        .find(|line| line.starts_with("MOUNT_QUOTE ="))
        .unwrap();
    let assets = fs::read_to_string(root.join("make/assets.mk")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("Makefile");
    for pack in ["; printf AOE_INJECTED #", "a path's ; `printf injected` #"] {
        // The stand-in Docker prints each argv member, then fails. A shell escape
        // must neither execute a second command nor turn that failure into success.
        fs::write(&fixture, format!("{quote}\nTOOL_IMAGE := tools\nDOCKER_RUN = sh -c 'printf \"%s\\n\" \"$$@\"; exit 17' fixture tools\nBIND_MOUNT =\n{assets}")).unwrap();
        let output = Command::new("make")
            .args(["--no-print-directory", "-f"])
            .arg(&fixture)
            .arg("assets-import-de")
            .arg(format!("AOE_ASSET_PACK={pack}"))
            .env_remove("MAKEFLAGS")
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "Docker failure was hidden: {output:?}"
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(
            stdout.lines().last(),
            Some(pack),
            "pack must be one literal argv member: {stdout:?}"
        );
    }
}
