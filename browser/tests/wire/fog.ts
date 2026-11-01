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
  return Buffer.from([
    8,
    ...uint(revision),
    ...uint(tick),
    ...region.flatMap((value) => uint(value * 2)),
    Number(disabled),
    ...uint(tiles.length),
    ...tiles,
    ...uint(overview.length),
    ...overview.flatMap(([index, state]) => [...uint(index), state]),
  ]);
}

/** Current protocol's explicit full-view setting for presentation fixtures. */
export function disabledFog(revision: number[]): Buffer {
  return Buffer.from([8, ...revision, 0, 0, 0, 2, 2, 1, 0, 0]);
}
