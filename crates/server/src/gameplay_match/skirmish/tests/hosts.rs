//! What an AI host process inherits from the server.
use super::*;

const INNER: &str = "AOE_TEST_AI_HOST_ENVIRONMENT";
const SECRET: &str = "inherited-match-secret-0123456789";

/// An AI host gets no hosting secret: it plays with its slots' tokens alone. The
/// rest of the environment (`PYTHONPATH`, `PATH`) it needs.
#[test]
fn an_ai_host_inherits_the_environment_without_the_hosting_secret() {
    if std::env::var_os(INNER).is_none() {
        // A process's environment is set safely only as it starts: this test
        // runs itself again in one that has the secret.
        let module = module_path!().split_once("::").expect("a crate path").1;
        let test = "an_ai_host_inherits_the_environment_without_the_hosting_secret";
        let run = std::process::Command::new(std::env::current_exe().expect("this program"))
            .args(["--exact", &format!("{module}::{test}"), "--nocapture"])
            .env(INNER, "1")
            .env("AOE_MATCH_TOKEN", SECRET)
            .output()
            .expect("the inner run");
        let told = String::from_utf8_lossy(&run.stdout);
        let failed = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{told}{failed}");
        assert!(told.contains("1 passed"), "{told}");
        return;
    }
    let directory = tempfile::tempdir().expect("directory");
    let seen = directory.path().join("environment");
    let script = format!("env > '{}'; cat > /dev/null", seen.display());
    let launch = Launch {
        command: vec!["sh".into(), "-c".into(), script],
        url: "http://127.0.0.1:1".into(),
    };
    let mut fleet = launch.spawn(1, &["0".repeat(48)]).expect("a host");
    let started = Instant::now();
    while !fleet.finished() {
        assert!(started.elapsed() < Duration::from_secs(60), "it never ends");
        std::thread::sleep(Duration::from_millis(10));
    }
    let environment = std::fs::read_to_string(&seen).expect("environment");
    let marker = format!("{INNER}=1");
    assert!(
        environment.lines().any(|line| line == marker),
        "the host needs the rest of the server's environment"
    );
    let leaks = environment.contains("AOE_MATCH_TOKEN") || environment.contains(SECRET);
    assert!(!leaks, "the host inherits the hosting secret");
}
