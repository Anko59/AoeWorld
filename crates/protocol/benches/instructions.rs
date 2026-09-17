use aoe_core::{EntityId, PlayerId, Position, Region, Tick};
use aoe_protocol::{EntityState, ServerMessage, encode_server};
use gungraun::Dhat;
use gungraun::prelude::*;
use std::hint::black_box;

fn setup() -> ServerMessage {
    ServerMessage::Snapshot {
        tick: Tick(17),
        region: Region {
            x: 0,
            y: 0,
            width: 256,
            height: 256,
        },
        entities: (0..1_000)
            .map(|id| EntityState {
                id: EntityId(id),
                player: PlayerId((id % 8) as u16),
                position: Position {
                    x: (id % 256) as i32,
                    y: (id / 4) as i32,
                },
            })
            .collect(),
        loaded_chunks: 16,
        total_entities: 8_000,
    }
}

#[library_benchmark(setup = setup)]
fn snapshot_encode(message: ServerMessage) -> usize {
    black_box(
        encode_server(black_box(&message))
            .map(|bytes| bytes.len())
            .unwrap_or_default(),
    )
}

library_benchmark_group!(name = protocol, benchmarks = [snapshot_encode]);

main!(
    config = LibraryBenchmarkConfig::default().tool(Dhat::default()),
    library_benchmark_groups = protocol
);
