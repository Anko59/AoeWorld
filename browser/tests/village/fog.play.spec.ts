import { expect } from "@playwright/test";
import { readFileSync } from "node:fs";
import { fogDelta, fogState } from "../wire/fog.js";
import { test } from "../../server.js";

function uint(value: number): number[] {
  const bytes: number[] = [];
  do {
    const low = value % 128;
    value = Math.floor(value / 128);
    bytes.push(low + (value ? 128 : 0));
  } while (value);
  return bytes;
}

/** The bytes the Rust codec pins for this message. */
function golden(name: string): string {
  const file = new URL(`../wire/${name}.hex`, import.meta.url);
  return readFileSync(file, "utf8").trim();
}

test("browser fog encoder matches the Rust golden", () => {
  expect(
    fogState(
      uint,
      1,
      2,
      [10, 20, 13, 21],
      [0, 1, 2],
      [
        [257, 1],
        [258, 2],
      ],
    ).toString("hex"),
  ).toBe(golden("fog"));
});

test("browser fog delta encoder matches the Rust golden", () => {
  // Two runs: tile bytes 0 to 2, then, two bytes on, the window's last tile.
  expect(
    fogDelta(
      uint,
      1,
      3,
      [
        [0, [1, 2, 2, 2, 1, 1, 1, 1, 2, 2, 2, 1]],
        [2, [2]],
      ],
      [
        [257, 2],
        [258, 0],
        [300, 1],
      ],
    ).toString("hex"),
  ).toBe(golden("fog-delta"));
});
