export const nodes = [
  { kind: 0, name: "berry", gather: "forage", x: 8191, y: 8194 },
  { kind: 1, name: "tree", gather: "chop", x: 8190, y: 8192 },
  { kind: 2, name: "gold", gather: "mine", x: 8196, y: 8190 },
];

export function chunks() {
  const push = (bytes: number[], value: number, count: number) => {
    for (let byte = 0; byte < count; byte++)
      bytes.push(Math.floor(value / 2 ** (byte * 8)) & 255);
  };
  return [
    [255, 255],
    [256, 255],
    [255, 256],
    [256, 256],
  ].map(([cx = 0, cy = 0]) => {
    const resources = nodes.filter(
      (node) =>
        Math.floor(node.x / 32) === cx && Math.floor(node.y / 32) === cy,
    );
    const bytes = [5, 0, 4, resources.length, 0, 0, 0];
    for (let y = 0; y < 32; y++)
      for (let x = 0; x < 32; x++) {
        push(bytes, 0, 4);
        push(bytes, 0, 2);
        for (let corner = 0; corner < 4; corner++) push(bytes, 0, 2);
        push(bytes, 1 << 21, 4);
        push(bytes, 0, 2);
        push(bytes, cx * 32 + x, 4);
        push(bytes, cy * 32 + y, 4);
        for (let absent = 0; absent < 7; absent++) bytes.push(0);
      }
    for (const node of resources) {
      push(bytes, node.y * (1 << 19) + node.x * 2, 8);
      push(bytes, node.x, 4);
      push(bytes, node.y, 4);
      bytes.push(
        node.kind,
        node.kind === 1 ? 0 : node.kind === 0 ? 1 : 2,
        100,
        0,
        0,
        node.kind === 1 ? 2 : 0,
      );
    }
    return {
      x: cx,
      y: cy,
      payload_hex: bytes
        .map((byte) => byte.toString(16).padStart(2, "0"))
        .join(""),
    };
  });
}
