//! Heavy slot configuration: the defaults, the overrides, and the CPUs each slot's containers get.
use super::super::{Config, SlotEnv};
use std::{path::Path, time::Duration};

#[test]
fn config_defaults_overrides_and_limits() {
    let config = |slots, wait| Config::from_values(2, 12, slots, wait);
    assert_eq!(
        config(None, None).unwrap(),
        Config {
            slots: 2,
            cores: 12,
            wait: Duration::from_secs(90 * 60)
        }
    );
    assert_eq!(
        config(Some("5"), Some("0")).unwrap(),
        Config {
            slots: 5,
            cores: 12,
            wait: Duration::ZERO
        }
    );
    assert_eq!(config(Some(""), Some("")).unwrap().slots, 2);
    for bad in ["0", "9", "-1", "two"] {
        assert!(config(Some(bad), None).is_err(), "{bad}");
    }
    assert!(config(None, Some("soon")).is_err());
    assert!(Config::from_values(0, 12, None, None).is_err());
    assert!(Config::from_values(2, 0, None, None).is_err());
    let committed = Config::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
        &SlotEnv::default(),
    );
    assert!(committed.is_ok_and(|c| (1..=8).contains(&c.slots)));
}

#[test]
fn the_cores_are_shared_among_the_slots_and_each_slot_gets_at_least_one() {
    let cpus = |slots| {
        Config::from_values(3, 12, slots, None)
            .unwrap()
            .cpus_per_slot()
    };
    assert_eq!(cpus(None), 4, "three slots on twelve cores");
    assert_eq!(cpus(Some("2")), 6);
    assert_eq!(cpus(Some("5")), 2, "the remainder is left unused");
    assert_eq!(cpus(Some("8")), 1);
    assert_eq!(
        Config::from_values(3, 2, Some("8"), None)
            .unwrap()
            .cpus_per_slot(),
        1
    );
    let committed = Config::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
        &SlotEnv::default(),
    )
    .unwrap();
    assert_eq!((committed.slots, committed.cores), (3, 12));
}
