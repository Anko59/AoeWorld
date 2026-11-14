use crate::quality_metrics::compile_load::CompileLoad;
use std::process::Command;

#[test]
fn compile_load_child_role() {
    if std::env::var_os("AOE_QUALITY_LOAD_CHILD").is_some() {
        eprintln!("Compiling controlled quality load");
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}
#[test]
fn compilation_load_observes_liveness_and_reaps_its_owned_process_group() {
    let directory = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir_in(directory.path()).unwrap();
    let target_path = target.path().to_owned();
    let log = directory.path().join("compile.log");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "quality_metrics::tests::load::compile_load_child_role",
            "--nocapture",
        ])
        .env("AOE_QUALITY_LOAD_CHILD", "1");
    let mut load = CompileLoad::spawn(&mut command, target, log.clone()).unwrap();
    load.live().unwrap();
    let pid = nix::unistd::Pid::from_raw(load.pid() as i32);
    assert!(nix::sys::signal::kill(pid, None).is_ok());
    drop(load);
    assert!(nix::sys::signal::kill(pid, None).is_err());
    assert!(!target_path.exists());
    assert!(std::fs::read_to_string(log).unwrap().contains("Compiling "));
}
#[test]
fn compilation_that_exits_before_measurement_is_not_counted_as_load() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = Command::new("true");
    assert!(
        CompileLoad::spawn(
            &mut command,
            tempfile::tempdir_in(directory.path()).unwrap(),
            directory.path().join("compile.log")
        )
        .is_err()
    );
}
