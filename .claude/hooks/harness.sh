#!/bin/sh
# Thin launcher for the Claude Code hook judge; the policy lives in
# `aoe-harness claude-hook` (crates/harness/src/claude). The judge is built in
# the pinned tool image from origin/dev, so a branch cannot change the rules
# that judge it. Until origin/dev has the adapter, it is built from this
# checkout and labelled a non-authoritative bootstrap.
set -u
event=${1:?usage: harness.sh <event>|build}
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P) || exit 2
common=$(git -C "$root" rev-parse --path-format=absolute --git-common-dir) || exit 2
if git -C "$root" cat-file -e refs/remotes/origin/dev:crates/harness/src/claude/mod.rs 2>/dev/null; then
  revision=$(git -C "$root" rev-parse refs/remotes/origin/dev) || exit 2
  key=$revision
  judge="origin/dev $revision"
else
  revision=
  key=bootstrap-$(git -C "$root" rev-parse HEAD:crates/harness) || exit 2
  judge="bootstrap from this checkout (non-authoritative)"
fi
binary="$common/aoe-claude-hook/$key/aoe-harness"
if [ ! -x "$binary" ] || [ "$event" = build ]; then
  mkdir -p "$common/aoe-claude-hook" || exit 2
  if ! flock "$common/aoe-claude-hook/build.lock" sh -c '[ -x "$1" ] && [ "$2" != build ] || make -C "$3" --no-print-directory claude-hook-build CLAUDE_HOOK_REVISION="$4" CLAUDE_HOOK_OUTPUT="$1"' \
    sh "$binary" "$event" "$root" "$revision" >&2; then
    echo "AoeWorld harness: the hook judge could not be built. A person must run .claude/hooks/harness.sh build in a terminal (needs Docker)." >&2
    case $event in pre-tool-use) exit 2 ;; *) exit 1 ;; esac
  fi
fi
[ "$event" = build ] && exit 0
AOE_CLAUDE_HOOK_ROOT=$root AOE_CLAUDE_HOOK_JUDGE=$judge exec "$binary" claude-hook "$event"
