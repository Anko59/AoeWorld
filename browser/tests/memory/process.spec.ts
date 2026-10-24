import { expect } from "@playwright/test";
import { test } from "../../server.js";
import { firefoxTabMemory, type ProcessMemory } from "./process.js";

const content = { type: "Web Content:42", residentBytes: 512 * 1024 ** 2 };
const parent = { type: "firefox:20", residentBytes: 1400 * 1024 ** 2 };

function tree(processes: ProcessMemory["processes"]): ProcessMemory {
  return {
    residentBytes: processes.reduce(
      (sum, entry) => sum + entry.residentBytes,
      0,
    ),
    processes,
  };
}

test("Firefox tab residency excludes parent and auxiliary processes", () => {
  expect(
    firefoxTabMemory(
      tree([
        parent,
        content,
        { type: "WebExtensions:43", residentBytes: 200 * 1024 ** 2 },
        { type: "Privileged Cont:44", residentBytes: 40 * 1024 ** 2 },
      ]),
    ),
  ).toEqual(tree([content]));
});

test("Firefox tab residency rejects missing or ambiguous content processes", () => {
  expect(() => firefoxTabMemory(tree([parent]))).toThrow(/exactly one/);
  expect(() =>
    firefoxTabMemory(
      tree([parent, content, { ...content, type: "Web Content:45" }]),
    ),
  ).toThrow(/exactly one/);
});
