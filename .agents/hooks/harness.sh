#!/bin/sh
# Thin launcher shared by every agent runtime's hooks; the policy lives in
# `aoe-harness agent-hook` (crates/harness/src/agents). The judge is built in the
# pinned tool image from origin/dev, so a branch cannot change the rules that
# judge it. Until origin/dev has this adapter, it is built from this checkout and
# labelled a non-authoritative bootstrap.
#   harness.sh <claude|codex|dsh|pi> <session-start|pre-tool-use|post-tool-use|stop|pre-compact>
#   harness.sh build
set -u
case ${1:-} in
  build) runtime=claude event=build ;;
  claude|codex|dsh|pi) runtime=$1 event=${2:?usage: harness.sh <runtime> <event>} ;;
  *) echo "usage: harness.sh <claude|codex|dsh|pi> <event> | build" >&2; exit 2 ;;
esac
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P) || exit 2
common=$(git -C "$root" rev-parse --path-format=absolute --git-common-dir) || exit 2
if git -C "$root" cat-file -e refs/remotes/origin/dev:crates/harness/src/agents/runtime.rs 2>/dev/null; then
  revision=$(git -C "$root" rev-parse refs/remotes/origin/dev) || exit 2
  key=$revision
  judge="origin/dev $revision"
else
  revision=
  key=bootstrap-$(git -C "$root" rev-parse HEAD:crates/harness) || exit 2
  judge="bootstrap from this checkout (non-authoritative)"
fi
binary="$common/aoe-agent-hook/$key/aoe-harness"
if [ ! -x "$binary" ] || [ "$event" = build ]; then
  mkdir -p "$common/aoe-agent-hook" || exit 2
  if ! flock "$common/aoe-agent-hook/build.lock" sh -c '[ -x "$1" ] && [ "$2" != build ] || make -C "$3" --no-print-directory agent-hook-build AGENT_HOOK_REVISION="$4" AGENT_HOOK_OUTPUT="$1"' \
    sh "$binary" "$event" "$root" "$revision" >&2; then
    echo "AoeWorld harness: the hook judge could not be built. A person must run .agents/hooks/harness.sh build in a terminal (needs Docker)." >&2
    case $event in pre-tool-use) exit 2 ;; *) exit 1 ;; esac
  fi
fi
[ "$event" = build ] && exit 0
AOE_AGENT_HOOK_ROOT=$root AOE_AGENT_HOOK_JUDGE=$judge exec "$binary" agent-hook --runtime "$runtime" "$event"
