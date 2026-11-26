//! What the CI workflow may not repeat: work a job does not use, and caches a pull request writes.
use std::path::Path;

fn read(path: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::read_to_string(root.join(path)).unwrap_or_else(|error| panic!("{path}: {error}"))
}

#[test]
fn only_pushes_to_dev_write_docker_layers() {
    let setup = read(".github/actions/docker-setup/action.yml");
    assert!(setup.contains("DOCKER_BUILD_CACHE_SAVE="));
    assert!(setup.contains("github.ref == 'refs/heads/dev'"));
    let makefile = read("Makefile");
    let build = makefile
        .lines()
        .find(|line| line.starts_with("IMAGE_BUILD ="))
        .expect("IMAGE_BUILD");
    let (read_part, write_part) = build.split_once("--cache-to").expect("cache export");
    assert!(read_part.contains("--cache-from"));
    assert!(
        read_part
            .trim_end()
            .ends_with("$(if $(DOCKER_BUILD_CACHE_SAVE),"),
        "the export is conditional on a dev push"
    );
    assert!(write_part.contains("mode=max"));
}
