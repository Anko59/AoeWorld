use super::*;

#[test]
fn dense_snapshot_resources_fit_the_resident_chunk_budget_and_roundtrip() {
    let nodes: Vec<_> = (0..grid::CELLS)
        .map(|at| {
            let node = crate::ResourceNode {
                id: at as u64 * 17,
                tile: grid::tile((0, 0), at),
                kind: [
                    crate::ResourceKind::Food,
                    crate::ResourceKind::Wood,
                    crate::ResourceKind::Gold,
                    crate::ResourceKind::Stone,
                ][at % 4],
                object: [
                    crate::ObjectKind::Tree,
                    crate::ObjectKind::ForageBush,
                    crate::ObjectKind::GoldDeposit,
                    crate::ObjectKind::StoneDeposit,
                    crate::ObjectKind::Decoration,
                    crate::ObjectKind::ShoreFish,
                    crate::ObjectKind::DeepFish,
                ][at % 7],
                initial_amount: 100,
                visual_variant: (at % 7) as u8,
            };
            (at as u16, node)
        })
        .collect();
    let stored = chunk(nodes.clone(), vec![(grid::CELLS as u16, 1)], 1);
    let bytes = stored.encode().unwrap();
    let chunk = NavigationChunk::decode(&bytes).unwrap();
    // A France snapshot has 23 million resource nodes. Retaining each node's
    // redundant global tile coordinate would add hundreds of MiB per world.
    assert!(chunk.retained_bytes() <= 24 * 1024);
    assert_eq!(
        chunk.encode().unwrap(),
        bytes,
        "storage format stays identical"
    );
    for (at, node) in nodes {
        assert_eq!(chunk.cell(at as usize).unwrap().node, Some(node));
    }
}

#[test]
fn sparse_component_runs_keep_compact_resident_labels() {
    let labels: Vec<_> = (1..=32).map(|band| (band * 32, band % 2 + 1)).collect();
    let stored = chunk(Vec::new(), labels, 2);
    let bytes = stored.encode().unwrap();
    let chunk = NavigationChunk::decode(&bytes).unwrap();
    // Large maps have hundreds of thousands of mostly uniform chunks. The
    // stored runs must not become a permanent 2 KiB tile array in each one.
    assert!(chunk.retained_bytes() <= 1024);
    assert_eq!(chunk.encode().unwrap(), bytes);
    for at in 0..grid::CELLS {
        assert_eq!(chunk.graph.label(at), ((at / 32 + 1) % 2 + 1) as u16);
    }
}

fn chunk(
    nodes: Vec<(u16, crate::ResourceNode)>,
    labels: Vec<(u16, u16)>,
    components: u16,
) -> NavigationChunk {
    let mut cells = Box::new([0; grid::CELLS]);
    let mut start = 0;
    for (end, label) in labels {
        cells[start..usize::from(end)].fill(label);
        start = usize::from(end);
    }
    let costs = OnceLock::new();
    costs.set(Vec::new()).unwrap();
    NavigationChunk {
        key: (0, 0),
        runs: vec![WalkRun {
            end: grid::CELLS as u16,
            flags: 7,
            height: 0,
        }]
        .into_boxed_slice(),
        nodes: nodes
            .into_iter()
            .map(|(at, node)| PackedNode::new(at, node))
            .collect(),
        graph: Arc::new(ChunkPortals {
            portals: Vec::new(),
            costs,
            labels: grid::Labels::Tiles(cells),
            components,
            links: Vec::new(),
            packed: None,
        }),
        packed: None,
    }
}
