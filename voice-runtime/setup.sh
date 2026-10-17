#!/bin/bash
set -euo pipefail
# This is the only network-enabled step; run deliberately to install local speech.
BASE="$HOME/Library/Application Support/com.dnalevity.livecoach"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
UV="${LIVE_COACH_UV:-/opt/homebrew/bin/uv}"
if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  echo "The optional local voice engine requires an Apple Silicon Mac." >&2
  exit 1
fi
mkdir -p "$BASE/voice-runtime" "$BASE/voice-model"
if [[ ! -x "$BASE/voice-runtime/.venv/bin/python" ]]; then
  "$UV" venv --python /opt/homebrew/bin/python3.13 "$BASE/voice-runtime/.venv"
fi
"$UV" pip install --python "$BASE/voice-runtime/.venv/bin/python" -r "$SCRIPT_DIR/requirements.lock"
cp "$SCRIPT_DIR/engine.py" "$SCRIPT_DIR/requirements.txt" "$SCRIPT_DIR/requirements.lock" "$BASE/voice-runtime/"
"$BASE/voice-runtime/.venv/bin/python" - "$BASE/voice-model" <<'PY'
import sys
from huggingface_hub import snapshot_download
snapshot_download(repo_id="mlx-community/Qwen3-TTS-12Hz-0.6B-Base-4bit",
                  revision="0d6bb6fe33f92d47a507e23b9148940e8366ab5b",
                  local_dir=sys.argv[1])
PY
chmod -R go-rwx "$BASE/voice-runtime" "$BASE/voice-model"
echo "Local voice engine installed. Runtime generation is offline."
