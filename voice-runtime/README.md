# Local voice runtime

The optional Apple Silicon voice engine uses [MLX Audio 0.5.9](https://github.com/Blaizzy/mlx-audio/tree/v0.5.9) (MIT) and [Qwen3-TTS 0.6B Base 4-bit](https://huggingface.co/mlx-community/Qwen3-TTS-12Hz-0.6B-Base-4bit) (Apache 2.0). The pinned model revision is `0d6bb6fe33f92d47a507e23b9148940e8366ab5b`. Model weights total approximately 1.71 GB; Python dependencies require additional storage. Upstream models and runtime retain their licenses and attribution. See the downloaded model's README and the app's notices.

Run `./voice-runtime/setup.sh` deliberately to download the model and install the runtime in the application's private support directory. Setup requires Homebrew Python 3.13 and uv. Generation is fully local, forces Hugging Face/Transformers offline mode, and cannot silently download weights. The interpreter is `~/Library/Application Support/com.dnalevity.livecoach/voice-runtime/.venv/bin/python`.

The app starts `engine.py --model LOCAL_MODEL_DIRECTORY` as a persistent process. Each stdin JSON line contains `{id, op: "synthesize", text, reference_audio, reference_text, output}`. Each stdout line returns `{id, ok, path, sample_rate, duration, elapsed}` or `{id, ok: false, error}`. Third-party diagnostics go to stderr. `{"id":1,"op":"status"}` checks the process without loading the model. Synthesis loads lazily and reuses the model. The parent app must kill the process to cancel an in-flight generation.

Only WAV reference files under `voice-profiles/` and WAV outputs under `voice-output/` inside the application's support directory are accepted. References must be 2–60 seconds long and include an accurate transcript. Output is private PCM16 WAV, written atomically. `--test-root TEMP_DIRECTORY` explicitly permits synthetic fixture files within a separate test directory. No audio recording or playback occurs inside this service.

Run the lightweight contract tests with `python3 -m unittest discover -s voice-runtime -p test_engine.py -v`. Real model smoke testing additionally requires the explicit setup above. The existing boundary fixture is synthetic speech; it must never be represented as the user's enrolled voice.
