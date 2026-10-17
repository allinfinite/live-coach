# Automatic Ollama warm-up — 0.1.1

Opening Live Coach with the local provider selected warms the selected installed
Ollama model. Look below Start for **Warming local coach…** and then **Local coach
ready**. Listening still shows Ready when you are, the timer stays at its initial
value and participant acknowledgment is unchecked. No audio device is opened.

The operation sends an empty local preload request, never transcript, profile
context or credentials. Installed-model preflight avoids downloads; cloud aliases
and external providers are excluded. Errors show Retry warm-up. Start stays
responsive while the model loads. If Ollama is not running, open it and retry.

A ten-minute timer, app focus and local model changes renew readiness. Duplicate
requests coalesce and old responses cannot label a different model ready. A
30-minute keep-alive is shared with normal local inference. Loading consumes local
RAM/GPU memory. Closing the app cancels any pending preload; it does not forcibly
unload a model another app might use. Ollama's lease expires normally. A server
restart, eviction or a long system sleep can still require another load.

Implementation uses Ollama's [documented preload mechanism](https://docs.ollama.com/faq)
and [generate API](https://docs.ollama.com/api/generate).

## Verification

- 25 Rust tests passed, one hardware test remains explicitly ignored. Five new
  warm-up tests verify empty payloads, installed-model and cloud checks, offline
  retry, coalescing/caching, TTL expiry and stale-request cancellation.
- 41 new warm-up browser assertions and 26 existing UI assertions passed. Tests
  cover no automatic Start/consent, saved model selection, readiness/errors/retry,
  provider/model changes, focus and ten-minute renewal, unavailable Ollama and
  explicit Start while preload is pending. Tauri IPC is mocked for browser tests.
- Actual installed 0.1.1 native launch: Ollama `/api/ps` was empty before launch;
  afterward `gemma4:latest` was resident with a 30-minute expiry. Native UI showed
  Local coach ready, Ready when you are, unchecked acknowledgment, 10:00 timer,
  empty transcript and disabled coaching actions. No listening session started.
- Installed bundle passed strict signature integrity verification; signing remains
  ad-hoc and not notarized, as documented for this development app.

Evidence in the parent `evidence/` directory: warmup-rust-tests.log,
warmup-build.log, warmup-before-native-launch.json,
warmup-after-native-launch.json, warmup-ready-coaching.log and warmup-native.png.
