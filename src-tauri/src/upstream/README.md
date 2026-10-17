# Meetily source attribution and adaptation notes

Original project: [Meetily Community Edition, Zackriya Solutions](https://github.com/Zackriya-Solutions/meetily).
Pinned source: `a2cb62e827da7ef59f65064c97233efb2313878e`, retrieved October 16, 2026.
License: MIT. The original copyright and license are retained in the repository
root `LICENSE`; see also root `ATTRIBUTION.md`. Live Coach is independent and is
not endorsed by the upstream authors.

| Local file | Original path beneath `frontend/src-tauri/src/` | Changes |
| --- | --- | --- |
| `core_audio.rs` | `audio/capture/core_audio.rs` | Added attribution header and clarified the ignored hardware test requirement; capture implementation retained |
| `parakeet.rs` | `parakeet_engine/model.rs` | Added attribution header; model implementation retained, including runtime-availability hook supplied by Live Coach |
| `resample.rs` | `audio/audio_processing.rs`, `resample` function | Extracted only the sinc resampler and required imports; no audio encoding or saving helpers |
| `mod.rs` | New | Declares the three isolated modules |

Additional small adaptations live in `../capture.rs`: CPAL microphone setup and
numeric sample conversion are derived from upstream `audio/stream.rs`; Whisper
context configuration and final text extraction derive from upstream
`whisper_engine/whisper_engine.rs`. The session lifecycle, bounded channels,
energy segmenter, activity counters, coach callback, fixture helper and local
privacy controls are Live Coach integration code.

No upstream recording manager, recording saver, database, model downloader,
telemetry, meeting summary frontend, or branded application bundle is included.
Model weights are not redistributed. Their licenses remain separate from the
application's MIT source license. ONNX Runtime, CPAL, cidre, whisper-rs and other
Rust dependencies retain their own licenses.
