use super::super::static_data::NavigationChunk;
use super::*;

pub(crate) fn encode(chunk: &NavigationChunk) -> Result<Vec<u8>> {
    if let Some(packed) = &chunk.packed {
        return Ok(packed.view().bytes.to_vec());
    }
    let graph = &chunk.graph;
    let mut labels: Vec<(u16, u16)> = Vec::new();
    for (at, label) in graph.labels().enumerate() {
        if let Some(last) = labels.last_mut().filter(|last| last.1 == label) {
            last.0 = at as u16 + 1;
        } else {
            labels.push((at as u16 + 1, label));
        }
    }
    let n = graph.portal_count();
    let length = HEADER
        + chunk.runs.len() * RUN
        + chunk.nodes.len() * NODE
        + n * PORTAL
        + n * n * 4
        + labels.len() * 4
        + graph.links.len() * 4;
    if length > MAX_BYTES || graph.costs.get().is_none() {
        return Err(EnvironmentPageError::Invalid);
    }
    let mut out = Vec::with_capacity(length);
    out.extend(chunk.key.0.to_le_bytes());
    out.extend(chunk.key.1.to_le_bytes());
    for count in [
        chunk.runs.len(),
        chunk.nodes.len(),
        n,
        labels.len(),
        graph.links.len(),
        graph.components as usize,
    ] {
        out.extend(
            u16::try_from(count)
                .map_err(|_| EnvironmentPageError::Invalid)?
                .to_le_bytes(),
        );
    }
    for run in &chunk.runs {
        out.extend(run.end.to_le_bytes());
        out.extend(run.height.to_le_bytes());
        out.extend([run.flags, 0]);
    }
    for node in &chunk.nodes {
        out.extend(node.id.to_le_bytes());
        out.extend(node.at.to_le_bytes());
        out.extend(node.amount.to_le_bytes());
        out.extend([node.kind as u8, node.object as u8, node.variant, 0]);
    }
    for portal in graph.portals() {
        out.extend((grid::index(portal.tile) as u16).to_le_bytes());
        out.extend([
            (portal.cross.x - chunk.key.0 * CHUNK_TILES) as i8 as u8,
            (portal.cross.y - chunk.key.1 * CHUNK_TILES) as i8 as u8,
        ]);
        out.extend(
            u16::try_from(portal.crossing_cost)
                .map_err(|_| EnvironmentPageError::Invalid)?
                .to_le_bytes(),
        );
        out.extend([u8::from(portal.bidirectional), 0]);
    }
    for from in 0..n {
        for to in 0..n {
            out.extend(graph.cost(from, to).unwrap_or(u32::MAX).to_le_bytes());
        }
    }
    for (end, label) in labels {
        out.extend(end.to_le_bytes());
        out.extend(label.to_le_bytes());
    }
    for (a, b) in graph.links() {
        out.extend(a.to_le_bytes());
        out.extend(b.to_le_bytes());
    }
    Ok(out)
}

pub(crate) fn decode(bytes: &[u8]) -> Result<NavigationChunk> {
    let view = View::validate(bytes)?;
    let key = view.key();
    let packed = PackedChunk {
        storage: Arc::new(bytes.to_vec()),
        range: 0..bytes.len(),
    };
    Ok(NavigationChunk {
        key,
        runs: Box::new([]),
        nodes: Box::new([]),
        graph: Arc::new(ChunkPortals::from_packed(packed.clone())),
        packed: Some(packed),
    })
}
