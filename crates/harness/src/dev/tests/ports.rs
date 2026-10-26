use super::*;

#[test]
fn the_host_port_comes_from_dev_port_and_defaults_to_8080() {
    assert_eq!(dev_port(None).expect("default"), 8080);
    assert_eq!(dev_port(Some("")).expect("empty"), 8080);
    assert_eq!(dev_port(Some("8082")).expect("chosen"), 8082);
    assert_eq!(dev_port(Some("65535")).expect("highest"), 65535);
    for value in [
        "0", "65536", "-1", "+8082", " 8082", "8082 ", "0x50", "http", "80.0",
    ] {
        let error = dev_port(Some(value)).expect_err(value).to_string();
        assert!(error.contains("DEV_PORT"), "{value}: {error}");
    }
}

#[test]
fn the_chosen_host_port_maps_to_the_fixed_container_port() {
    assert_eq!(host_mapping(8080), "127.0.0.1:8080:8080");
    assert_eq!(host_mapping(8082), "127.0.0.1:8082:8080");
    assert_eq!(url(8082), "http://127.0.0.1:8082");
    let checkout = built_checkout();
    let runtime = FakeRuntime::default();
    runtime.healthy.set(true);
    start_with(
        &runtime,
        checkout.path(),
        "smoke",
        "revision",
        None,
        None,
        8082,
    )
    .expect("start");
    let calls = runtime.calls.borrow();
    let run = calls
        .iter()
        .find(|args| args.first().is_some_and(|arg| arg == "run"))
        .expect("Docker run");
    let published = run
        .windows(2)
        .filter_map(|pair| (pair[0] == "-p").then_some(pair[1].as_str()))
        .collect::<Vec<_>>();
    // Loopback only, one mapping; the server still binds 8080 inside.
    assert_eq!(published, ["127.0.0.1:8082:8080"]);
    assert!(run.contains(&"AOE_BIND=0.0.0.0:8080".to_owned()));
    assert_eq!(*runtime.probed.borrow(), [8082]);
}

#[test]
fn two_checkouts_run_side_by_side_on_their_own_ports() {
    let (first, second) = (built_checkout(), built_checkout());
    let (one, two) = (FakeRuntime::default(), FakeRuntime::default());
    one.healthy.set(true);
    two.healthy.set(true);
    start_with(&one, first.path(), "smoke", "revision", None, None, 8080).expect("first");
    start_with(&two, second.path(), "smoke", "revision", None, None, 8082).expect("second");
    let run = |runtime: &FakeRuntime| {
        runtime
            .calls
            .borrow()
            .iter()
            .find(|args| args.first().is_some_and(|arg| arg == "run"))
            .expect("Docker run")
            .clone()
    };
    let (one_run, two_run) = (run(&one), run(&two));
    assert!(one_run.contains(&name(first.path())) && two_run.contains(&name(second.path())));
    assert_ne!(name(first.path()), name(second.path()));
    assert!(one_run.contains(&"127.0.0.1:8080:8080".to_owned()));
    assert!(two_run.contains(&"127.0.0.1:8082:8080".to_owned()));
    // Stopping one names only its own container.
    down_with(&two, &name(second.path())).expect("stop second");
    assert!(
        two.calls
            .borrow()
            .iter()
            .any(|args| args == &["stop", &name(second.path())])
    );
    assert!(one.running.get());
}

#[test]
fn a_lab_running_on_another_port_is_reported_not_moved() {
    let checkout = built_checkout();
    let runtime = FakeRuntime::default();
    runtime.exists.set(true);
    runtime.running.set(true);
    runtime.published.set(8082);
    start_with(
        &runtime,
        checkout.path(),
        "smoke",
        "revision",
        None,
        None,
        8082,
    )
    .expect("same port");
    let error = start_with(
        &runtime,
        checkout.path(),
        "smoke",
        "revision",
        None,
        None,
        8080,
    )
    .expect_err("other port")
    .to_string();
    assert!(error.contains("http://127.0.0.1:8082"), "{error}");
    assert!(error.contains("make down"), "{error}");
    assert!(!runtime.calls.borrow().iter().any(|args| matches!(
        args.first().map(String::as_str),
        Some("rm" | "run" | "stop")
    )));
}
