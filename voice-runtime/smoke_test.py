#!/usr/bin/env python3
"""Explicit offline synthetic-fixture integration test; never enrolls a user."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import wave

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--fixture", required=True, type=Path)
parser.add_argument("--output-dir", required=True, type=Path)
args = parser.parse_args()
root = args.output_dir.resolve()
root.mkdir(parents=True, exist_ok=True)
reference = root / "synthetic-reference.wav"
shutil.copyfile(args.fixture, reference)
base = Path.home() / "Library/Application Support/com.dnalevity.livecoach"
requests = [
    {"id": "before", "op": "status"},
    {"id": "cold", "op": "synthesize", "text": "Let me take a moment to think.",
     "reference_audio": str(reference),
     "reference_text": "I need some time to think. I cannot take on another task this week. Can we discuss another option?",
     "output": str(root / "voice-fixture.wav")},
    {"id": "after", "op": "status"},
    {"id": "warm", "op": "synthesize", "text": "Let me take a moment to think.",
     "reference_audio": str(reference),
     "reference_text": "I need some time to think. I cannot take on another task this week. Can we discuss another option?",
     "output": str(root / "voice-fixture-warm.wav")},
]
with (root / "engine-stderr.log").open("w") as log:
    result = subprocess.run(
        [str(base / "voice-runtime/.venv/bin/python"), str(Path(__file__).with_name("engine.py")),
         "--model", str(base / "voice-model"), "--test-root", str(root)],
        input="".join(json.dumps(request) + "\n" for request in requests),
        capture_output=False, stdout=subprocess.PIPE, stderr=log, text=True, timeout=600,
    )
responses = [json.loads(line) for line in result.stdout.splitlines()]
evidence = {"synthetic_fixture_only": True, "exit_code": result.returncode, "responses": responses}
assert result.returncode == 0, result
assert len(responses) == 4 and all(response["ok"] for response in responses), responses
assert responses[0]["loaded"] is False and responses[2]["loaded"] is True, responses
for response in (responses[1], responses[3]):
    path = Path(response["path"])
    with wave.open(str(path)) as wav:
        assert wav.getnframes() > 1000
        assert wav.getframerate() == response["sample_rate"]
        assert wav.getsampwidth() == 2
    response["sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
(root / "voice-smoke.json").write_text(json.dumps(evidence, indent=2) + "\n")
print(json.dumps(evidence, indent=2))
