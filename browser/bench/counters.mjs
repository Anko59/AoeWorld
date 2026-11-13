// Installed only by the benchmark driver. Counts API calls, never GPU time.
export const names = [
  "webgl.error_checks",
  "webgl.texture_uploads",
  "webgl.texture_bytes",
  "webgl.buffer_upload_bytes",
  "webgl.buffer_allocated_bytes",
  "webgl.draw_calls",
  "webgl.instances",
];
export function installCounters(target) {
  let values = Array(7).fill(0),
    active = false;
  const wrap = (name, count) => {
    const original = target[name];
    target[name] = function (...args) {
      const result = original.apply(this, args);
      if (active) count(args);
      return result;
    };
  };
  wrap("getError", () => values[0]++);
  wrap("texSubImage3D", (args) => {
    values[1]++;
    values[2] += args[5] * args[6] * args[7] * 4;
  });
  wrap("bufferSubData", (args) => {
    const source = args[2],
      offset = args[3] ?? 0,
      length = args[4];
    values[3] += ArrayBuffer.isView(source)
      ? (length || source.length - offset) * source.BYTES_PER_ELEMENT
      : source.byteLength;
  });
  wrap(
    "bufferData",
    (args) =>
      (values[4] += typeof args[1] === "number" ? args[1] : args[1].byteLength),
  );
  wrap("drawArraysInstanced", (args) => {
    values[5]++;
    values[6] += args[3];
  });
  return {
    reset() {
      values = Array(7).fill(0);
      active = true;
    },
    take() {
      active = false;
      return [...values];
    },
  };
}
