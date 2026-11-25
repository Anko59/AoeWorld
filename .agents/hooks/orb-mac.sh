#!/bin/sh
# Installed in the OrbStack machine as `gh`, `claude` and `aoe-mac-sign`
# (docs/local-development.md): runs the Mac's own command through OrbStack's
# `mac`, so the machine holds no login, token or key. `mac` keeps the directory,
# stdin, stdout and exit status but not the environment, so the harness's
# variables are passed explicitly.
set -u
name=$(basename -- "$0")
if [ "$name" = aoe-mac-sign ]; then
  # git's ssh signing program: `-Y sign -n git -f <key> <file>` must write
  # <file>.sig. The machine's own key setting is a placeholder: the Mac signs
  # with the key its git configuration names for this directory.
  [ "${1:-}" = -Y ] && [ "${2:-}" = sign ] || exec mac ssh-keygen "$@"
  for file do :; done
  mac sh -c 'exec ssh-keygen -q -Y sign -n git -f "$(git config user.signingkey)"' <"$file" >"$file.sig"
  exit $?
fi
names=$(awk 'BEGIN {
  for (n in ENVIRON)
    if (n ~ /^(AOE|HARNESS|SHIP|REVIEW|GH|CLAUDE)_[A-Za-z0-9_]*$/) print n
}')
set -- "$name" "$@"
# AOE_GH_ACCOUNT picks one of the Mac's gh accounts for this call; its token
# stays in that gh process's environment on the Mac.
if [ "$name" = gh ] && [ -n "${AOE_GH_ACCOUNT:-}" ]; then
  shift
  set -- sh -c 'GH_TOKEN=$(gh auth token -u "$0") || exit; export GH_TOKEN; exec gh "$@"' "$AOE_GH_ACCOUNT" "$@"
fi
for n in $names; do
  eval "value=\${$n}"
  set -- "$n=$value" "$@"
done
exec mac env "$@"
