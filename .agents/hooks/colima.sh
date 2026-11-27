#!/bin/sh
# macOS has no Linux kernel for the harness, its judge or its containers, so a
# Mac runs them inside a Linux VM that mounts the home directory at the same
# path: the Colima VM, or else an OrbStack machine (docs/local-development.md).
# This runs one command there, in the caller's directory, with the caller's
# stdin, exit status and the harness's environment variables. Policy stays in
# `aoe-harness`.
#   colima.sh <command> [argument...]
#   colima.sh --backend    prints colima or orb
# AOE_COLIMA_MAKE_VARS names variables to append as NAME=value arguments.
# AOE_ORB_MACHINE names the OrbStack machine (default aoeworld).
set -u
[ $# -gt 0 ] || { echo "usage: colima.sh <command> [argument...]" >&2; exit 2; }
profile=${COLIMA_PROFILE:-default}
case $profile in default) instance=colima ;; *) instance=colima-$profile ;; esac
config=${COLIMA_HOME:-$HOME/.colima}/_lima/$instance/ssh.config
machine=${AOE_ORB_MACHINE:-aoeworld}
orb=$(command -v orb 2>/dev/null) || orb=$HOME/.orbstack/bin/orb
if [ -f "$config" ]; then
  backend=colima
elif [ -x "$orb" ] && "$orb" list -q 2>/dev/null | grep -qx -- "$machine"; then
  backend=orb
else
  echo "AoeWorld harness: no Colima VM '$profile' ($config is missing) and no OrbStack machine '$machine'. Run: colima start --cpu 8 --memory 16 --disk 100, or set up an OrbStack machine (docs/local-development.md)" >&2
  exit 2
fi
[ "$1" != --backend ] || { echo "$backend"; exit 0; }
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P) || exit 2
case $here in
  "$HOME"/*) ;;
  *) echo "AoeWorld harness: this checkout is outside $HOME, which is all Colima mounts in the VM." >&2; exit 2 ;;
esac
case $PWD in "$HOME" | "$HOME"/*) directory=$PWD ;; *) directory=/ ;; esac

# One shell word per argument: the VM's login shell parses the command line again.
quote() {
  rest=$1 quoted=
  while :; do
    case $rest in
      *"'"*) quoted="$quoted${rest%%"'"*}'\\''" rest=${rest#*"'"} ;;
      *) quoted="$quoted$rest"; break ;;
    esac
  done
  printf "'%s' " "$quoted"
}

# The harness's own variables, Git's hook context and the committer identity.
# Never the Mac's paths (HOME, PATH, TMPDIR, GIT_EXEC_PATH), and never a token:
# the command line is visible to `ps`, so the VM holds its own `gh auth login`.
names=$(awk 'BEGIN {
  for (name in ENVIRON)
    if (name ~ /^(AOE|HARNESS|SHIP|REVIEW|PERF|SHOWCASE|NEXTEST|CARGO_PROFILE)_[A-Za-z0-9_]*$/ ||
        name ~ /^(CLAUDE_PROJECT_DIR|SESSION|DEV_PORT|GAME_SCORE_PR_ARGS|RUNTIME|CARGO_INCREMENTAL|CARGO_BUILD_JOBS|RUST_TEST_THREADS)$/ ||
        name ~ /^GIT_(INDEX_FILE|DIR|WORK_TREE|PREFIX|REFLOG_ACTION|(AUTHOR|COMMITTER)_(NAME|EMAIL|DATE))$/)
      print name
}')
remote="cd $(quote "$directory")&& exec env "
for name in $names; do
  case $name in AOE_COLIMA_MAKE_VARS) continue ;; esac
  eval "value=\${$name}"
  remote="$remote$(quote "$name=$value")"
done
# The VM has no global Git configuration: commits made there keep this identity.
for field in name email; do
  value=$(git config "user.$field" 2>/dev/null) || continue
  upper=$(printf %s "$field" | tr a-z A-Z)
  for role in AUTHOR COMMITTER; do
    eval "[ -n \"\${GIT_${role}_${upper}:-}\" ]" || remote="$remote$(quote "GIT_${role}_${upper}=$value")"
  done
done
for argument in "$@"; do remote="$remote$(quote "$argument")"; done
for name in ${AOE_COLIMA_MAKE_VARS:-}; do
  eval "value=\${$name-}"
  remote="$remote$(quote "$name=$value")"
done

if [ "$backend" = orb ]; then
  "$orb" -m "$machine" sh -c "$remote"
  exit $?
fi
ssh -o LogLevel=QUIET -F "$config" "lima-$instance" "$remote"
status=$?
if [ "$status" = 255 ]; then
  echo "AoeWorld harness: the Colima VM '$profile' is unreachable. Run: colima start" >&2
  exit 2
fi
exit "$status"
