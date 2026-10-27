import { expect } from "@playwright/test";
import type {
  FullConfig,
  FullResult,
  Suite,
  TestCase,
  TestResult,
} from "@playwright/test/reporter";
import QuarantineReporter from "../../quarantine-reporter.js";
import { test } from "../../server.js";

test("quarantine step evidence is serialized once and retains nested failures", async () => {
  // Playwright projects inherit the same metadata object as the full config.
  // JSON reports serialize both project lists as well as the global metadata.
  const metadata = { runner: "kept" };
  const config: FullConfig = {
    ...test.info().config,
    metadata,
    projects: test.info().config.projects.map((project) => ({
      ...project,
      metadata,
    })),
  };
  const reporter = new QuarantineReporter();
  const testCase = {
    id: "sentinel",
    annotations: [],
    location: { file: "tests/sentinel.spec.ts", line: 7, column: 3 },
    titlePath: () => ["", "webgpu", "tests/sentinel.spec.ts", "sentinel"],
    parent: { project: () => ({ name: "webgpu" }) },
  } as unknown as TestCase;
  const error = { message: "nested assertion", stack: "assertion stack" };
  const nested = {
    title: "Expect sentinel",
    duration: 10,
    category: "expect",
    error,
    steps: [],
  };
  const hook = {
    title: "Before Hooks",
    duration: 20,
    category: "hook",
    error: undefined,
    steps: [nested],
  };
  reporter.onBegin(config, { allTests: () => [testCase] } as Suite);
  reporter.onTestEnd(testCase, {
    errors: [error],
    steps: [hook],
  } as unknown as TestResult);
  reporter.onEnd({ status: "failed" } as FullResult);

  const report = JSON.stringify({
    metadata: config.metadata,
    projects: config.projects,
    filteredProjects: config.projects,
  });
  expect(report.match(/"aoeQuarantine":/g)).toHaveLength(1);
  expect(config.metadata).toEqual({
    runner: "kept",
    aoeQuarantine: {
      selected: ["sentinel"],
      status: "failed",
      tests: {
        sentinel: {
          file: "tests/sentinel.spec.ts",
          line: 7,
          title: "sentinel",
          project: "webgpu",
        },
      },
      results: {
        sentinel: { errors: [error], steps: [{ ...hook, error: null }] },
      },
    },
  });
  expect(testCase.annotations).toEqual([
    { type: "aoe-quarantine-id", description: "sentinel" },
  ]);
});
