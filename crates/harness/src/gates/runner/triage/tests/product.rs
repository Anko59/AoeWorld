//! Finite Cartesian properties, not a fuzzer or authenticated source worker.
use super::*;

#[test]
fn finite_actual_command_retention_endpoint_publication_product_preserves_planes() {
    for kind in ["SUCCESS", "FAILED", "DEADLINE", "CANCELLED", "START"] {
        for log_failed in [false, true] {
            for endpoint in [None, Some(1), Some(2)] {
                for pub_failed in [false, true] {
                    let recipe = match kind {
                        "FAILED" => "exit 7",
                        "DEADLINE" => "sleep 2",
                        _ => "printf 'MEASURED_SECRET'",
                    };
                    let (owner, root) = root(recipe);
                    let destination = tempfile::tempdir().unwrap();
                    if log_failed {
                        fs::create_dir(destination.path().join("alpha.log")).unwrap();
                    }
                    let output =
                        PrivateOutput::new(destination.path(), &[owner.path().into()]).unwrap();
                    let mut runtime = fixture(owner.path(), &output);
                    runtime.changed_at = endpoint;
                    runtime.deadline = kind == "DEADLINE";
                    runtime.start_failure = kind == "START";
                    if kind == "CANCELLED" {
                        runtime.cancellation.cancel();
                    }
                    let ledger = runner::run(
                        &mut runtime,
                        &root,
                        &plan(false),
                        runner::tests::metadata(),
                        budget(),
                    );
                    let publication = if pub_failed {
                        Publication::Failed {
                            io_kind: SafeErrorKind::PermissionDenied,
                        }
                    } else {
                        Publication::Published
                    };
                    let value = safe(&ledger, &publication);
                    if endpoint == Some(1) {
                        assert!(ledger.results[0].triage.is_none());
                        assert_eq!(value["gates"][0]["command"]["kind"], "UNOBSERVED");
                        assert!(capture(&value).is_null());
                        assert!(value["gates"][0]["command"].get("not_started").is_none());
                    } else {
                        assert_eq!(capture(&value)["outcome"]["kind"], kind);
                        assert!(capture(&value)["duration_ms"].is_number());
                        assert_eq!(
                            value["gates"][0]["command"]["observation"]["retention"]["kind"],
                            if log_failed { "FAILED" } else { "RETAINED" }
                        );
                        if kind == "FAILED" {
                            assert_eq!(capture(&value)["outcome"]["code"], 2);
                        }
                    }
                    if log_failed || endpoint.is_some() || pub_failed || kind != "SUCCESS" {
                        assert_ne!(value["overall"], "PASS");
                    }
                    assert!(
                        !summary(&ledger, &publication)
                            .unwrap()
                            .contains("MEASURED_SECRET")
                    );
                    if endpoint.is_some() {
                        assert_eq!(ledger.overall, Overall::Invalid);
                    }
                }
            }
        }
    }
}
