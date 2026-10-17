#!/bin/zsh
set -euo pipefail
base="$(cd -- "$(dirname -- "$0")/.." && pwd)"
model="${1:-$HOME/Library/Application Support/com.meetily.ai/models/parakeet/parakeet-tdt-0.6b-v3-int8}"
"$base/dist/Live Coach.app/Contents/MacOS/live-coach" --self-test "$model" "$base/tests/fixtures/boundary.wav" "${@:2}"
