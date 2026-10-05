use super::*;

fn reverse_chain() -> Registry {
    let mut registry = registry();
    for (id, implied) in [
        ("browser", vec!["native"]),
        ("native", vec!["performance"]),
        ("docs", vec!["native", "static"]),
    ] {
        registry
            .suites
            .iter_mut()
            .find(|suite| suite.id == id)
            .expect("fixture suite")
            .implies = paths(&implied);
    }
    // This declaration order requires multiple rounds in the production scan.
    registry
        .suites
        .sort_by_key(|suite| match suite.id.as_str() {
            "performance" => 0,
            "native" => 1,
            "browser" => 2,
            "docs" => 3,
            _ => 4,
        });
    Registry::parse(&serde_json::to_vec(&registry).unwrap()).expect("valid reverse chain")
}

fn expected_closure(registry: &Registry, initial: &[(&str, &str)]) -> Classification {
    let mut expected = Classification {
        suites: suite_set(&["static"]),
        reasons: BTreeMap::from([("static".into(), suite_set(&["baseline"]))]),
    };
    for (suite, reason) in initial {
        expected.suites.insert((*suite).into());
        expected
            .reasons
            .entry((*suite).into())
            .or_default()
            .insert((*reason).into());
    }
    // Independent graph walk: process each reachable owner once, but retain
    // every outgoing edge's reason even when its target was already reached.
    let mut pending: Vec<_> = expected.suites.iter().cloned().collect();
    let mut processed = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !processed.insert(id.clone()) {
            continue;
        }
        if let Some(suite) = registry.suites.iter().find(|suite| suite.id == id) {
            for implied in &suite.implies {
                expected
                    .reasons
                    .entry(implied.clone())
                    .or_default()
                    .insert(format!("implied by {id}"));
                if expected.suites.insert(implied.clone()) {
                    pending.push(implied.clone());
                }
            }
        }
    }
    expected
}

#[test]
fn reverse_chain_closes_all_hops_and_preserves_multiple_owner_reasons() {
    let registry = reverse_chain();
    let classification = registry.classify(&paths(&["web/index.html", "docs/testing.md"]));
    assert_eq!(
        classification.suites,
        suite_set(&["static", "docs", "browser", "native", "performance"])
    );
    assert_eq!(
        classification.reasons,
        BTreeMap::from([
            ("static".into(), suite_set(&["baseline", "implied by docs"])),
            ("docs".into(), suite_set(&["path: docs/testing.md"])),
            ("browser".into(), suite_set(&["path: web/index.html"])),
            (
                "native".into(),
                suite_set(&["implied by browser", "implied by docs"])
            ),
            ("performance".into(), suite_set(&["implied by native"])),
        ])
    );
}

#[test]
fn classification_cartesian_paths_and_declaration_orders_have_exact_reasons() {
    let registry = reverse_chain();
    let mut reversed = registry.clone();
    reversed.suites.reverse();
    type Case = (
        &'static [&'static str],
        &'static [(&'static str, &'static str)],
    );
    let cases: &[Case] = &[
        (&[], &[("everything", "no comparison paths")]),
        (&["web/index.html"], &[("browser", "path: web/index.html")]),
        (&["docs/testing.md"], &[("docs", "path: docs/testing.md")]),
        (
            &["web/index.html", "docs/testing.md"],
            &[
                ("browser", "path: web/index.html"),
                ("docs", "path: docs/testing.md"),
            ],
        ),
        (
            &["unknown/file"],
            &[("everything", "unknown: unknown/file")],
        ),
        (&["AGENTS.md"], &[("everything", "protected: AGENTS.md")]),
        (
            &["docs/agent-engineering.md"],
            &[("everything", "protected: docs/agent-engineering.md")],
        ),
        (
            &["docs/../testing.md"],
            &[("everything", "unknown: docs/../testing.md")],
        ),
        (
            &["web/index.html", "unknown/file"],
            &[
                ("browser", "path: web/index.html"),
                ("everything", "unknown: unknown/file"),
            ],
        ),
    ];
    for (names, initial) in cases {
        let expected = expected_closure(&registry, initial);
        let original = paths(names);
        let mut reversed_paths = original.clone();
        reversed_paths.reverse();
        let mut duplicated = original.clone();
        duplicated.extend(original.clone());
        for order in [&registry, &reversed] {
            for inputs in [&original, &reversed_paths, &duplicated] {
                assert_eq!(order.classify(inputs), expected, "{inputs:?}");
            }
        }
    }
}

#[test]
fn direct_classification_keeps_unknown_implied_ids_and_cycle_reasons_finite() {
    let mut registry = reverse_chain();
    registry
        .suites
        .iter_mut()
        .find(|suite| suite.id == "performance")
        .unwrap()
        .implies = paths(&["browser", "undeclared"]);
    let expected = expected_closure(&registry, &[("browser", "path: web/index.html")]);
    assert!(expected.suites.contains("undeclared"));
    assert_eq!(registry.classify(&paths(&["web/index.html"])), expected);
}
