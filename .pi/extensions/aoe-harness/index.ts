// AoeWorld harness for pi: forwards pi's events to the shared judge
// (`.agents/hooks/harness.sh pi <event>`, i.e. `aoe-harness agent-hook --runtime pi`)
// and applies its Claude-shaped answers. The policy lives in Rust; this file only
// translates. See docs/agent-runtimes.md.
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

type Answer = { [key: string]: any } | undefined;

function checkout(cwd: string): string | undefined {
  const result = spawnSync("git", ["-C", cwd, "rev-parse", "--show-toplevel"], { encoding: "utf8" });
  return result.status === 0 ? result.stdout.trim() : undefined;
}

/** Runs the judge; `undefined` means allow, `{ unavailable }` means it could not
 * run or answered unreadably, which a tool call treats as a refusal. */
function judge(cwd: string, event: string, payload: object): Answer {
  const root = checkout(cwd);
  if (!root) return { unavailable: `${cwd} is not inside a Git checkout` };
  const result = spawnSync(join(root, ".agents/hooks/harness.sh"), ["pi", event], {
    input: JSON.stringify(payload),
    encoding: "utf8",
    timeout: 1_800_000,
    env: { ...process.env, CLAUDE_PROJECT_DIR: root },
  });
  if (result.error || result.status !== 0) {
    return { unavailable: (result.stderr || String(result.error ?? "the judge failed")).trim() };
  }
  const out = (result.stdout || "").trim();
  if (!out) return undefined;
  try {
    return JSON.parse(out);
  } catch {
    return { unavailable: `unreadable judge answer: ${out.slice(0, 200)}` };
  }
}

export default function (pi: ExtensionAPI) {
  const session = `pi-${process.pid}-${Date.now()}`;
  let started = false;
  const base = (cwd: string, name: string) => ({ session_id: session, hook_event_name: name, cwd });

  pi.on("before_agent_start", async (_event, ctx) => {
    if (started) return;
    started = true;
    const answer = judge(ctx.cwd, "session-start", { ...base(ctx.cwd, "SessionStart"), source: "startup" });
    const context = answer?.hookSpecificOutput?.additionalContext;
    if (context) return { message: { customType: "aoe-harness", content: context, display: false } };
  });

  pi.on("tool_call", async (event, ctx) => {
    const answer = judge(ctx.cwd, "pre-tool-use", {
      ...base(ctx.cwd, "PreToolUse"),
      tool_name: event.toolName,
      tool_input: event.input,
    });
    if (answer?.unavailable) return { block: true, reason: `AoeWorld harness unavailable: ${answer.unavailable}` };
    const decision = answer?.hookSpecificOutput;
    if (decision?.permissionDecision === "deny") return { block: true, reason: decision.permissionDecisionReason };
  });

  pi.on("tool_result", async (event, ctx) => {
    if (event.toolName !== "write" && event.toolName !== "edit") return;
    const answer = judge(ctx.cwd, "post-tool-use", {
      ...base(ctx.cwd, "PostToolUse"),
      tool_name: event.toolName,
      tool_input: event.input,
    });
    if (answer?.decision === "block") {
      return { content: [...event.content, { type: "text", text: answer.reason }] };
    }
  });

  pi.on("agent_end", async (_event, ctx) => {
    const answer = judge(ctx.cwd, "stop", base(ctx.cwd, "Stop"));
    if (answer?.decision === "block") pi.sendUserMessage(answer.reason, { deliverAs: "followUp" });
    else if (answer?.systemMessage) ctx.ui.notify(answer.systemMessage, "warning");
  });

  pi.on("session_before_compact", async (_event, ctx) => {
    judge(ctx.cwd, "pre-compact", base(ctx.cwd, "PreCompact"));
  });
}
