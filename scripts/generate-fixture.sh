#!/bin/sh
# Synthetic macOS voice only. Never use real call audio as a test fixture.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
tmpdir=$(mktemp -d)
trap 'rm -f "$tmpdir/boundary.aiff"; rmdir "$tmpdir"' EXIT
say -v Samantha -o "$tmpdir/boundary.aiff" 'I need some time to think. I cannot take on another task this week. Can we discuss another option?'
afconvert -f WAVE -d LEI16@16000 -c 1 "$tmpdir/boundary.aiff" "$root/tests/fixtures/boundary.wav"
printf '%s\n' 'Generated synthetic tests/fixtures/boundary.wav'
