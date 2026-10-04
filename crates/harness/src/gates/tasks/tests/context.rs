use super::super::{
    catalog::Role,
    task::{Artifact, ArtifactKind, ContextPacket, Kind, Status},
};
use super::{catalog, task};

#[test]
fn context_packet_routes_every_closed_task_kind_and_keeps_the_utf8_bound() {
    let roles = catalog();
    let coordinator = roles.role(Role::Coordinator).unwrap();
    let mut value = task();
    value.role = Role::Coordinator;
    value.status = Status::Planned;

    for (kind, required) in [
        (Kind::Feature, vec!["skills/protocol/SKILL.md"]),
        (Kind::Bug, vec!["skills/protocol/SKILL.md"]),
        (Kind::Refactor, vec!["skills/protocol/SKILL.md"]),
        (Kind::Performance, vec!["skills/performance/SKILL.md"]),
        (
            Kind::Geodata,
            vec![
                "skills/asset-import/SKILL.md",
                "skills/game-assets/SKILL.md",
            ],
        ),
        (Kind::Qa, vec!["skills/game-assets/SKILL.md"]),
        (Kind::ReleaseInspection, vec!["skills/release/SKILL.md"]),
        (
            Kind::PolicyUpgrade,
            vec![
                "skills/harness-ci/SKILL.md",
                "docs/adr/0006-provider-neutral-harness.md",
            ],
        ),
    ] {
        value.kind = kind;
        let packet = ContextPacket::new(&value, coordinator).unwrap();
        for guide in required {
            assert!(
                packet.guides.iter().any(|actual| actual == guide),
                "{kind:?}: {guide}"
            );
        }
        assert!(
            packet
                .guides
                .iter()
                .any(|guide| guide == "docs/agent-engineering.md")
        );
        assert!(packet.guides.iter().any(|guide| guide == "docs/testing.md"));
        assert!(packet.guides.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(serde_json::to_vec(&packet).unwrap().len() <= 16_384);
    }

    value.objective = "x".repeat(16_384);
    assert!(ContextPacket::new(&value, coordinator).is_err());
}

#[test]
fn artifact_count_and_digest_shape_rejections_stay_fail_closed() {
    let hash = super::registry().fingerprint().unwrap();
    let mut value = task();
    value.artifacts = (0..17)
        .map(|index| Artifact {
            kind: ArtifactKind::Review,
            path: format!("task-artifacts/{}/review-{index}.json", value.id),
            blake3: None,
        })
        .collect();
    assert!(value.validate(&hash).is_err());

    for digest in ["A".repeat(64), "g".repeat(64), "a".repeat(63)] {
        let mut invalid = task();
        invalid.artifacts.push(Artifact {
            kind: ArtifactKind::Review,
            path: "task-artifacts/movement-fix/review.json".into(),
            blake3: Some(digest),
        });
        assert!(invalid.validate(&hash).is_err());
    }
}
