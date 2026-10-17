# Attribution

Live Coach is independently developed from Meetily Community Edition by Zackriya Solutions, MIT licensed.
Upstream: https://github.com/Zackriya-Solutions/meetily
Pinned source commit: a2cb62e827da7ef59f65064c97233efb2313878e (retrieved October 16, 2026).
The upstream MIT license is retained in LICENSE. Rust audio capture and Parakeet code are adapted from upstream; individual modifications are recorded in src-tauri/src/upstream/README.md.
Live Coach is a separate application and is not endorsed by Zackriya Solutions.

Optional local coaching speech uses MLX Audio 0.5.9 (MIT; https://github.com/Blaizzy/mlx-audio/tree/v0.5.9) and Qwen3-TTS 0.6B Base 4-bit (Apache 2.0; https://huggingface.co/mlx-community/Qwen3-TTS-12Hz-0.6B-Base-4bit). The pinned model revision is 0d6bb6fe33f92d47a507e23b9148940e8366ab5b. Voice model weights and Python dependencies are installed separately by explicit setup and retain their upstream licenses. They are not bundled in the application. See docs/VOICE.md and voice-runtime/README.md.

Third-party components retain their respective licenses, including Tauri (MIT/Apache-2.0), whisper.cpp/whisper-rs (MIT/Unlicense as applicable), ONNX Runtime (MIT), CPAL (Apache-2.0) and cidre (MIT). Speech model weights are user-provided; their separate licenses continue to apply. No model weights are redistributed in the app.
