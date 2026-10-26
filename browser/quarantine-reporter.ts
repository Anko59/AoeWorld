// Transport runner evidence omitted by Playwright's JSON reporter. Rust owns
// classification and retry policy. This reporter must precede `json` on the CLI.
import type {
  FullConfig,
  FullResult,
  Reporter,
  Suite,
  TestCase,
  TestResult,
  TestStep,
} from "@playwright/test/reporter";

function steps(items: TestStep[]): unknown[] {
  return items.map((step) => ({
    title: step.title,
    duration: step.duration,
    category: step.category,
    error: step.error ?? null,
    steps: steps(step.steps),
  }));
}

export default class QuarantineReporter implements Reporter {
  private evidence: {
    selected: string[];
    status: string;
    results: Record<string, unknown>;
  } = { selected: [], status: "incomplete", results: {} };

  printsToStdio(): boolean {
    return false;
  }

  onBegin(config: FullConfig, suite: Suite): void {
    this.evidence.selected = suite.allTests().map((test) => test.id);
    // Projects inherit this object; mutating it repeats the entire step tree in
    // both JSON project lists. Keep the complete evidence once at config level.
    config.metadata = { ...config.metadata, aoeQuarantine: this.evidence };
  }

  onTestEnd(test: TestCase, result: TestResult): void {
    test.annotations.push({ type: "aoe-quarantine-id", description: test.id });
    this.evidence.results[test.id] = {
      errors: result.errors,
      steps: steps(result.steps),
    };
  }

  onEnd(result: FullResult): void {
    this.evidence.status = result.status;
  }
}
