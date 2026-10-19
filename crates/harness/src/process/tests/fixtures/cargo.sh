#!/bin/sh
printf '%s\n' "$*" >> calls
exec /bin/sh -c "$AOE_FAKE_CARGO_BODY" -- "$@"
