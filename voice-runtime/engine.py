#!/usr/bin/env python3
"""Offline, persistent Qwen3-TTS voice cloning over a JSON-lines pipe.

No network server, audio capture, playback, or implicit model downloads.
The parent app owns consent, reference recording, cancellation and deletion.
"""
import argparse
import contextlib
import json
import os
from pathlib import Path
import sys
import time

# Set before importing either Transformers or Hugging Face.
os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"
os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"
os.environ["DO_NOT_TRACK"] = "1"

APP_ROOT = Path.home() / "Library/Application Support/com.dnalevity.livecoach"


def bounded_path(value, root, *, exists):
    if not isinstance(value, str) or not value:
        raise ValueError("A local WAV path is required")
    path = Path(value).expanduser().resolve()
    root = root.resolve()
    if not path.is_relative_to(root) or path == root or path.suffix.lower() != ".wav":
        raise ValueError("Audio path is outside the private voice directory")
    if exists and not path.is_file():
        raise ValueError("Reference audio is missing; record your voice first")
    return path


def required_text(value, name, maximum):
    if not isinstance(value, str) or not value.strip() or len(value) > maximum:
        raise ValueError(f"{name} must contain 1–{maximum} characters")
    return value.strip()


class Engine:
    def __init__(self, model_path, root=APP_ROOT, test_root=None):
        self.model_path = Path(model_path).expanduser().resolve()
        self.reference_root = Path(test_root) if test_root else root / "voice-profiles"
        self.output_root = Path(test_root) if test_root else root / "voice-output"
        self.model = None

    def handle(self, request):
        if not isinstance(request, dict):
            raise ValueError("Expected a JSON object")
        if request.get("op") == "status":
            return {"loaded": self.model is not None}
        if request.get("op") != "synthesize":
            raise ValueError("Unsupported voice operation")
        text = required_text(request.get("text"), "Suggestion", 2000)
        ref_text = required_text(request.get("reference_text"), "Reference transcript", 4000)
        reference = bounded_path(request.get("reference_audio"), self.reference_root, exists=True)
        output = bounded_path(request.get("output"), self.output_root, exists=False)
        if output == reference:
            raise ValueError("Output cannot overwrite the voice reference")
        for item in ("config.json", "model.safetensors", "speech_tokenizer/model.safetensors"):
            if not (self.model_path / item).is_file():
                raise ValueError("Local voice model is missing or incomplete; run voice setup explicitly")
        started = time.monotonic()
        # Third-party libraries print progress; keep stdout exclusively for our protocol.
        with contextlib.redirect_stdout(sys.stderr):
            import numpy as np
            import soundfile as sf
            info = sf.info(str(reference))
            if not 2 <= info.duration <= 60:
                raise ValueError("Reference audio must be between 2 and 60 seconds")
            if self.model is None:
                from mlx_audio.tts.utils import load_model
                self.model = load_model(str(self.model_path))
            results = list(self.model.generate(
                text=text, ref_audio=str(reference), ref_text=ref_text,
                lang_code="english", max_tokens=1024, verbose=False,
                temperature=0.7,
            ))
            if not results:
                raise RuntimeError("The local voice model returned no audio")
            rates = {int(result.sample_rate) for result in results}
            if len(rates) != 1:
                raise RuntimeError("The local voice model returned inconsistent sample rates")
            rate = rates.pop()
            audio = np.concatenate([np.asarray(result.audio).reshape(-1) for result in results])
            if len(audio) == 0 or not np.isfinite(audio).all() or np.max(np.abs(audio)) < 1e-6:
                raise RuntimeError("The local voice model returned empty or silent audio")
            output.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            temporary = output.with_suffix(".pending.wav")
            try:
                sf.write(str(temporary), audio, rate, subtype="PCM_16")
                temporary.chmod(0o600)
                temporary.replace(output)
            finally:
                temporary.unlink(missing_ok=True)
        return {"path": str(output), "sample_rate": rate,
                "duration": len(audio) / rate, "elapsed": time.monotonic() - started}


def serve(engine, incoming=sys.stdin, outgoing=sys.stdout):
    for line in incoming:
        request = None
        try:
            if len(line) > 32768:
                raise ValueError("Voice request is too large")
            request = json.loads(line)
            response = {"id": request.get("id") if isinstance(request, dict) else None,
                        "ok": True, **engine.handle(request)}
        except Exception as exc:
            # Do not echo coaching text, reference transcript, or full requests into logs.
            response = {"id": request.get("id") if isinstance(request, dict) else None,
                        "ok": False, "error": str(exc)}
        outgoing.write(json.dumps(response, ensure_ascii=False) + "\n")
        outgoing.flush()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--test-root", type=Path, help="Explicit temporary test workspace only")
    args = parser.parse_args()
    os.umask(0o077)
    serve(Engine(args.model, test_root=args.test_root))
