use crate::devserver::ensure::{Action, decide};
use crate::devserver::state::Instance;

fn instance(revision: &str, identity: &str) -> Instance {
    Instance {
        name: "play".into(),
        port: 8091,
        pid: 1,
        started: 0,
        revision: revision.into(),
        identity: identity.into(),
        container: None,
    }
}

const OLD: &str = "1111111111111111111111111111111111111111";
const NEW: &str = "2222222222222222222222222222222222222222";

#[test]
fn nothing_recorded_means_start() {
    assert_eq!(decide(None, false, None, NEW, "a"), Action::Start);
}

#[test]
fn a_server_at_dev_with_the_same_configuration_is_kept() {
    let current = instance(NEW, "a");
    assert_eq!(
        decide(Some(&current), true, Some(NEW), NEW, "a"),
        Action::Keep
    );
}

#[test]
fn dev_moving_restarts_the_server_and_names_both_revisions() {
    let current = instance(OLD, "a");
    let Action::Restart(why) = decide(Some(&current), true, Some(OLD), NEW, "a") else {
        panic!("a server at the old revision must restart");
    };
    assert!(
        why.contains("111111111111") && why.contains("222222222222"),
        "{why}"
    );
}

#[test]
fn a_dead_or_unhealthy_server_is_restarted() {
    let current = instance(NEW, "a");
    assert!(matches!(
        decide(Some(&current), false, Some(NEW), NEW, "a"),
        Action::Restart(_)
    ));
    assert!(matches!(
        decide(Some(&current), true, None, NEW, "a"),
        Action::Restart(_)
    ));
}

#[test]
fn a_changed_configuration_restarts_a_server_at_the_right_revision() {
    let current = instance(NEW, "a");
    assert!(matches!(
        decide(Some(&current), true, Some(NEW), NEW, "b"),
        Action::Restart(_)
    ));
}

#[test]
fn settings_read_the_environment_and_refuse_a_bad_port() {
    use crate::devserver::ensure::Settings;
    let from = |pairs: &'static [(&str, &str)]| {
        Settings::from_env(&|name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        })
    };
    let defaults = from(&[]).expect("defaults");
    assert_eq!(defaults.port, 8091);
    assert!(defaults.fetch && defaults.rev.is_none());
    let set = from(&[
        ("AOE_DEV_SERVER_PORT", "9000"),
        ("AOE_DEV_SERVER_REV", "HEAD"),
        ("AOE_DEV_SERVER_FETCH", "0"),
    ])
    .expect("set");
    assert_eq!(
        (set.port, set.rev.as_deref(), set.fetch),
        (9000, Some("HEAD"), false)
    );
    for bad in ["80", "abc", "99999", "+9000"] {
        let pairs: &'static [(&str, &str)] = Box::leak(Box::new([("AOE_DEV_SERVER_PORT", bad)]));
        assert!(from(pairs).is_err(), "{bad}");
    }
}
