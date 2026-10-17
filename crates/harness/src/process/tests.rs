use super::*;
mod capture;
mod excerpt;

#[test]
fn bounded_log_keeps_tail() {
    let mut log = BoundedLog::new();
    log.push(&vec![b'a'; LOG_LIMIT]);
    log.push(b"tail");
    let rendered = log.render();
    assert!(rendered.starts_with(b"[earlier output truncated]"));
    assert!(rendered.ends_with(b"tail"));
}

#[test]
fn real_exit_and_timeout_are_visible() {
    assert!(matches!(
        run("false", &[], Duration::from_secs(2)),
        Err(ProcessError::Exit { .. })
    ));
    assert!(matches!(
        run("sleep", &["2"], Duration::from_millis(10)),
        Err(ProcessError::Deadline { .. })
    ));
}

#[test]
fn supervised_child_receives_explicit_environment() {
    run_with_env(
        "sh",
        &["-c", "test \"$AOE_TEST_VALUE\" = expected"],
        &[("AOE_TEST_VALUE", "expected")],
        Duration::from_secs(2),
    )
    .expect("child environment");
}

#[test]
fn explicit_root_execution_uses_child_cwd_without_changing_parent() {
    let root = tempfile::tempdir().expect("child root");
    let parent = std::env::current_dir().expect("parent cwd");
    fs::write(root.path().join("marker"), "expected").expect("marker");
    run_in(
        root.path(),
        "sh",
        &[
            "-c",
            "test -f marker && test \"$AOE_TEST_VALUE\" = expected",
        ],
        &[("AOE_TEST_VALUE", "expected")],
        Duration::from_secs(2),
    )
    .expect("explicit cwd/environment");
    assert_eq!(std::env::current_dir().expect("unchanged cwd"), parent);
    let empty = tempfile::tempdir().expect("different root");
    assert!(matches!(
        run_in(
            empty.path(),
            "sh",
            &["-c", "test -f marker"],
            &[],
            Duration::from_secs(2)
        ),
        Err(ProcessError::Exit { .. })
    ));
}

#[test]
fn real_cancellation_stops_child_promptly() {
    let cancellation = Cancellation::default();
    let signal = cancellation.clone();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(30));
        signal.cancel();
    });
    let start = Instant::now();
    assert!(matches!(
        run_cancellable("sleep", &["5"], Duration::from_secs(10), &cancellation),
        Err(ProcessError::Cancelled { .. })
    ));
    worker.join().expect("cancellation worker");
    assert!(start.elapsed() < Duration::from_secs(2));
}

fn fake(one: f64) -> impl Fn() -> Option<load::Load> {
    move || {
        Some(load::Load {
            one,
            five: 20.0,
            fifteen: 10.5,
            cores: 12,
        })
    }
}

fn deadline_message(reader: &load::Reader<'_>) -> String {
    let mut command = Command::new("sleep");
    command.arg("2");
    supervise_reading(
        "sleep",
        command,
        Duration::from_millis(10),
        &Cancellation::default(),
        reader,
    )
    .expect_err("deadline")
    .to_string()
}

#[test]
fn deadline_error_states_the_load_and_calls_an_overloaded_machine_so() {
    let busy = deadline_message(&fake(31.4));
    assert!(busy.contains("machine load was 31.4 / 20.0 / 10.5 (1 / 5 / 15 min) on 12 cores"));
    assert!(busy.contains("the machine was overloaded; this is probably not your change"));
    assert!(busy.contains("rerun when it is quieter"));
    let quiet = deadline_message(&fake(3.0));
    assert!(
        quiet.contains("machine load was 3.0 / 20.0 / 10.5"),
        "{quiet}"
    );
    assert!(!quiet.contains("overloaded"), "{quiet}");
    let unknown = deadline_message(&|| None);
    assert!(
        unknown.contains("exceeded its 0s deadline; log:"),
        "{unknown}"
    );
}

#[test]
fn the_online_cpu_list_counts_ranges_and_single_cpus() {
    assert_eq!(load::online_count("0-11\n"), Some(12));
    assert_eq!(load::online_count("0-3,6"), Some(5));
    assert_eq!(load::online_count("7"), Some(1));
    assert_eq!(load::online_count("3-1"), None);
    assert_eq!(load::online_count("zero"), None);
    assert_eq!(load::online_count(""), None);
    assert!(load::cores() >= 1);
}

#[test]
fn the_load_is_parsed_from_proc_loadavg_and_exactly_the_core_count_is_not_overload() {
    let load = load::parse("0.52 1.25 3.00 2/801 12345\n", 4).unwrap();
    assert_eq!(
        (load.one, load.five, load.fifteen, load.cores),
        (0.52, 1.25, 3.0, 4)
    );
    assert!(load::parse("garbage", 4).is_none());
    assert!(load::parse("1.0 2.0", 4).is_none());
    assert!(!load::parse("4.00 1 1 1/1 1", 4).unwrap().overloaded());
    assert!(load::parse("4.01 1 1 1/1 1", 4).unwrap().overloaded());
}
