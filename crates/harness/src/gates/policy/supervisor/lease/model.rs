//! Fixed in-memory daemon only: no real Docker adapter or authority.
use super::*;
// This deterministic in-memory daemon is intentionally reachable by preview, NOT
// a production Docker transport. Client death never implicitly removes its object.
pub(super) struct ModelDaemon {
    pub(super) mode: &'static str,
    pub(super) time: u64,
    pub(super) object: bool,
    pub(super) calls: Vec<&'static str>,
    pub(super) inspection: usize,
}
impl ModelDaemon {
    pub(super) fn new(mode: &'static str) -> Self {
        Self {
            mode,
            time: 0,
            object: false,
            calls: Vec::new(),
            inspection: 0,
        }
    }
    pub(super) fn id() -> ContainerId {
        ContainerId::simulation()
    }
    pub(super) fn controller(mode: &'static str) -> Controller<Self> {
        Controller {
            backend: Self::new(mode),
            expected: ExpectedIdentity::simulation(),
            events: Vec::new(),
            last_ms: None,
        }
    }
}
impl Backend for ModelDaemon {
    fn now_ms(&self) -> u64 {
        self.time
    }
    fn call(&mut self, action: &Action, timeout_ms: u64) -> Reply {
        self.calls.push(action.name());
        self.time += if self.mode == "late" { timeout_ms } else { 1 };
        let mut success = true;
        let value = match action {
            Action::PersistIntent(identity) => {
                success = self.mode != "persist-failed";
                assert_eq!(*identity, ExpectedIdentity::simulation());
                json!(null)
            }
            Action::Create(identity) => {
                assert_eq!(*identity, ExpectedIdentity::simulation());
                self.object = self.mode != "recover-zero";
                success = !self.mode.starts_with("recover-");
                let bytes = match self.mode {
                    "short-cid" => b"abc\n".to_vec(),
                    "multiple-cid" => {
                        format!("{}\n{}\n", "a".repeat(64), "b".repeat(64)).into_bytes()
                    }
                    "invalid-cid" => vec![255],
                    _ => "a".repeat(64).into_bytes(),
                };
                return Reply {
                    bytes,
                    success,
                    truncated: self.mode == "create-truncated",
                };
            }
            Action::Recover(identity) => {
                assert_eq!(*identity, ExpectedIdentity::simulation());
                match self.mode {
                    "recover-zero" => json!([]),
                    "recover-many" => json!(["a".repeat(64), "b".repeat(64)]),
                    "recover-oversize" => json!(vec!["a".repeat(64); 17]),
                    _ => json!(["a".repeat(64)]),
                }
            }
            Action::Inspect(id) => {
                assert_eq!(*id, Self::id());
                self.inspection += 1;
                success = self.mode != "daemon-lost";
                if !self.object {
                    json!({"absent":true})
                } else {
                    if self.mode == "invalid-inspect" {
                        return Reply {
                            bytes: vec![255],
                            success: true,
                            truncated: false,
                        };
                    }
                    if self.mode == "missing-metadata" {
                        json!({"cid":"a".repeat(64)})
                    } else {
                        let mut observed =
                            Observation::matching(id, &ExpectedIdentity::simulation());
                        if self.mode == "wrong-cid" {
                            observed.cid = "b".repeat(64);
                        }
                        let field = self
                            .mode
                            .strip_prefix("wrong-")
                            .filter(|field| *field != "cid");
                        if let Some(field) = field {
                            observed.identity =
                                serde_json::to_value(ExpectedIdentity::simulation().changed(field))
                                    .unwrap_or(Value::Null);
                        }
                        if self.mode == "changed-before-remove" && self.inspection > 1 {
                            observed.identity = json!({});
                        }
                        serde_json::to_value(observed).unwrap_or(Value::Null)
                    }
                }
            }
            Action::Stop(id) => {
                assert_eq!(*id, Self::id());
                success = !matches!(self.mode, "stop-failed" | "kill-failed");
                json!(null)
            }
            Action::Kill(id) => {
                assert_eq!(*id, Self::id());
                success = self.mode != "kill-failed";
                json!(null)
            }
            Action::Remove(id) => {
                assert_eq!(*id, Self::id());
                success = self.mode != "remove-failed";
                if success && self.mode != "remove-lies" {
                    self.object = false;
                }
                json!(null)
            }
        };
        Reply {
            bytes: serde_json::to_vec(&value).unwrap_or_default(),
            success,
            truncated: self.mode == "inspect-truncated" && matches!(action, Action::Inspect(_)),
        }
    }
}

/// Fixed model scenarios: no caller IDs, commands, daemon or qualification flags.
pub(super) fn preview_models() -> Value {
    let scenarios = [
        "success",
        "wrong-cid",
        "wrong-service",
        "wrong-epoch",
        "wrong-nonce",
        "wrong-daemon",
        "wrong-image",
        "wrong-security",
        "daemon-lost",
        "late",
        "create-truncated",
        "inspect-truncated",
        "invalid-inspect",
        "missing-metadata",
        "recover-one",
        "recover-many",
        "recover-zero",
        "stop-failed",
        "remove-lies",
        "changed-before-remove",
    ];
    let mut receipts: Vec<Value> = scenarios
        .iter()
        .map(|&mode| {
            let mut controller = ModelDaemon::controller(mode);
            json!({"scenario":mode,"receipt":controller.run(15000,15000,false,false)})
        })
        .collect();
    for (scenario, admission, cleanup, before, after) in [
        ("zero-admission", 0, 15000, false, false),
        ("zero-cleanup", 15000, 0, false, false),
        ("cancel-before-create", 15000, 15000, true, false),
        ("cancel-workload-cleanup", 15000, 15000, false, true),
    ] {
        let mut controller = ModelDaemon::controller("success");
        receipts.push(
            json!({"scenario":scenario,"receipt":controller.run(admission,cleanup,before,after)}),
        );
    }
    json!({"authoritative":false,"status":"MODEL_ONLY","scenarios":receipts,
        "limits":["deterministic in-memory daemon only; no Docker mutation or gate verdict", "intent persistence is a model hook, not fsync/durable journal qualification", "labels/CIDs correlate ownership only on an independently qualified service daemon", "no authenticated artifact, isolated deployment, worker execution or SIGKILL recovery qualification"]})
}
