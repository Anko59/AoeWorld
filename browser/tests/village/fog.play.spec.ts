import { expect } from "@playwright/test";
import { readFileSync } from "node:fs";
import { fogState } from "../wire/fog.js";
import { test } from "../../server.js";

test("browser fog encoder matches the Rust golden", () => {
  const uint = (value: number) => {
    const bytes = [];
    do {
      const low = value % 128;
      value = Math.floor(value / 128);
      bytes.push(low + (value ? 128 : 0));
    } while (value);
    return bytes;
  };
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
  ).toBe(
    readFileSync(new URL("../wire/fog.hex", import.meta.url), "utf8").trim(),
  );
});
