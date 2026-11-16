/** Tile states four to a byte, the first in the low bits, as Rust packs. */
function packed(tiles: number[]): number[] {
  const bytes: number[] = [];
  tiles.forEach((state, at) => {
    bytes[at >> 2] = (bytes[at >> 2] ?? 0) | (state << (2 * (at % 4)));
  });
  return bytes;
}

/** Appended Rust FogState discriminant; postcard varints and zigzag coords. */
export function fogState(
  uint: (value: number) => number[],
  revision: number,
  tick: number,
  region: [number, number, number, number],
  tiles: number[],
  overview: [number, number][] = [],
  disabled = false,
): Buffer {
  const states = packed(tiles);
  return Buffer.from([
    8,
    ...uint(revision),
    ...uint(tick),
    ...region.flatMap((value) => uint(value * 2)),
    Number(disabled),
    ...uint(states.length),
    ...states,
    ...uint(overview.length),
    ...overview.flatMap(([index, state]) => [...uint(index), state]),
  ]);
}

/**
 * Appended Rust FogDelta discriminant: changes to the view a client holds.
 * Each run leaves `skip` tile bytes (four tiles each) as they are after the
 * run before it, then rewrites whole bytes with the new states of `tiles`.
 * An overview state of 0 turns that strategic pixel black again.
 */
export function fogDelta(
  uint: (value: number) => number[],
  revision: number,
  tick: number,
  runs: [skip: number, tiles: number[]][],
  overview: [number, number][] = [],
): Buffer {
  const extents = runs.flatMap(([skip, tiles]) => [
    ...uint(skip),
    ...uint(Math.ceil(tiles.length / 4)),
  ]);
  const states = runs.flatMap(([, tiles]) => packed(tiles));
  return Buffer.from([
    12,
    ...uint(revision),
    ...uint(tick),
    ...uint(runs.length),
    ...extents,
    ...uint(states.length),
    ...states,
    ...uint(overview.length),
    ...overview.flatMap(([index, state]) => [...uint(index), state]),
  ]);
}

/** Current protocol's explicit full-view setting for presentation fixtures. */
export function disabledFog(revision: number[]): Buffer {
  return Buffer.from([8, ...revision, 0, 0, 0, 2, 2, 1, 0, 0]);
}
