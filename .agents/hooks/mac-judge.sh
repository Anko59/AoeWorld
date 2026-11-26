#!/bin/sh
# The ship commands of a Mac with a Colima VM, run by a judge the Mac builds for
# itself, so they use the Mac's own gh, claude, git, ssh and commit signing
# (docs/local-development.md). Gates still go through `make` into the VM.
#   mac-judge.sh goals    the commands this Mac runs itself, if any
#   mac-judge.sh build <root> <origin/dev revision> <judges directory> <key>
set -u
case ${1:-} in
  goals)
    # Opt-in: a person may be logged in inside the VM. OrbStack needs no such route.
    [ "${AOE_NATIVE_SHIP:-}" = 1 ] || exit 0
    [ "$("$(dirname -- "$0")/colima.sh" --backend 2>/dev/null)" = colima ] || exit 0
    echo ship ship-status describe-check review review-floor review-pr issue next carry-review update-prs
    ;;
  build)
    root=$2 revision=$3 home=$4 key=$5
    channel=$(git -C "$root" show "$revision:rust-toolchain.toml" | sed -n 's/^channel = "\(.*\)"$/\1/p')
    rustup=$(command -v rustup) || rustup=${CARGO_HOME:-$HOME/.cargo}/bin/rustup
    if ! "$rustup" run "$channel" cargo --version >/dev/null 2>&1; then
      echo "AoeWorld harness: AOE_NATIVE_SHIP needs Rust on this Mac (rustup toolchain install $channel --profile minimal); see docs/local-development.md, or use OrbStack, which needs none" >&2
      exit 1
    fi
    mkdir -p "$home" || exit 1
    # macOS has no flock(1): Perl holds the kernel lock while the build runs.
    exec perl -MFcntl=:flock -e 'open my $lock, ">>", shift or die $!; flock $lock, LOCK_EX or die $!; my $status = system @ARGV; exit($status && ($status >> 8 || 1))' \
      "$home/build.lock" "$0" locked "$root" "$revision" "$home" "$key" "$rustup" "$channel"
    ;;
  locked)
    root=$2 revision=$3 home=$4 key=$5 rustup=$6 channel=$7
    binary=$home/$key/aoe-harness source=$home/source
    [ ! -x "$binary" ] || exit 0
    rm -rf "$source" && mkdir -p "$source" "$home/$key" || exit 1
    git -C "$root" archive -o "$home/source.tar" "$revision" && tar -xf "$home/source.tar" -C "$source" && rm "$home/source.tar" || exit 1
    # From `/`: Cargo would otherwise read .cargo/config.toml of the checkout
    # around the source, and a branch must not configure its judge's build.
    cd / && "$rustup" run "$channel" cargo build --locked -p aoe-harness \
      --manifest-path "$source/Cargo.toml" --target-dir "$home/target" || exit 1
    cp "$home/target/debug/aoe-harness" "$binary.new" && mv -f "$binary.new" "$binary" || exit 1
    cd "$source" && "$rustup" run "$channel" "$binary" agent-prune --home "$home" --keep "$key" ||
      echo "AoeWorld harness: the Mac's older judges were not pruned ($home)" >&2
    exit 0
    ;;
  *) echo "usage: mac-judge.sh goals | build <root> <revision> <directory> <key>" >&2; exit 2 ;;
esac
