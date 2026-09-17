use aoe_core::{EntityId, PlayerId, Position, Region, Tick};
use aoe_protocol::{EntityState, ServerMessage, decode_server, encode_server};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn snapshot() -> ServerMessage {
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

fn benchmarks(c: &mut Criterion) {
    let message = snapshot();
    assert!(encode_server(&message).is_ok());
    c.bench_function("snapshot_encode_1k", |bench| {
        bench.iter(|| black_box(encode_server(black_box(&message))));
    });
    let bytes = encode_server(&message).unwrap_or_default();
    assert!(decode_server(&bytes).is_ok());
    c.bench_function("snapshot_decode_1k", |bench| {
        bench.iter(|| black_box(decode_server(black_box(&bytes))));
    });
}

criterion_group!(timings, benchmarks);
criterion_main!(timings);
