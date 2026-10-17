# Live Coach 0.1.2 — macOS development build

A compact always-on-top conversation coach built from [Meetily Community Edition](https://github.com/Zackriya-Solutions/meetily), pinned at `a2cb62e827da7ef59f65064c97233efb2313878e`. MIT attribution is preserved in LICENSE and ATTRIBUTION.md. This is an independent app, bundle ID `com.dnalevity.livecoach`.

## Community

Live Coach is independently maintained; it is not an official Meetily product. See [contributing](CONTRIBUTING.md), [security reporting](SECURITY.md), and [attribution](ATTRIBUTION.md). Personal coaching is the current priority. Automated call handling and local computer control are future exploration, not current features.

## Install this build

Run commands from the repository root. A source checkout must first be built and copied into `dist/Live Coach.app` using the build instructions below. No binaries or model weights are included in the source repository.

Apple Silicon, macOS 14.4 or later:

```sh
./scripts/install.sh
```

This copies the packaged app to `~/Applications/Live Coach.app`, verifies its ad-hoc signature, and opens it. It reopens an identical installed build and refuses to overwrite a different Live Coach. It does not alter `/Applications/meetily.app` or Meetily data.

Manual equivalent:

```sh
mkdir -p "$HOME/Applications"
ditto --norsrc --noextattr "dist/Live Coach.app" "$HOME/Applications/Live Coach.app"
open "$HOME/Applications/Live Coach.app"
```

The delivered development bundle is **ad-hoc signed, not Developer ID signed or notarized**. The installed app passes `codesign --verify --deep --strict`. The installer omits source filesystem attributes so iCloud Finder metadata cannot invalidate that check after copying. Gatekeeper may reject a quarantined download. For this personally reviewed local build, use Finder → Control-click Live Coach → Open; on recent macOS versions, attempt launch then use System Settings → Privacy & Security → Open Anyway if offered. macOS may require an administrator password. Do not disable Gatekeeper globally. A signature integrity check is not notarization.

On first explicit listening session, grant microphone and system-audio recording permission when macOS requests it. If needed, enable Live Coach in System Settings → Privacy & Security → Microphone and Screen & System Audio Recording, then restart the app. No session starts on launch.

## Automatic local coach warm-up

When Live Coach opens with Ollama selected, it preloads the selected installed
model using Ollama's documented [empty-request preload API](https://docs.ollama.com/faq).
The status near Start changes from “Warming local coach…” to “Local coach ready.”
This sends an empty prompt to `127.0.0.1:11434`; it does not start a conversation,
open microphone/system capture, use saved context, or download a model. You still
need explicit Start and participant acknowledgment for audio.

Warm-up runs in the background and does not block the controls. It rechecks on
local model changes, focus and a ten-minute interval, coalescing duplicate work.
The model lease is 30 minutes and ordinary local coaching requests use that same
lease. This can keep the model in RAM while Live Coach is open; after closing,
Ollama manages its expiry. No model is forcibly unloaded because other apps may
be using it. An Ollama restart or memory pressure can still evict a model.

External provider selection cancels warm-up and never preloads a cloud model.
If Ollama is stopped, the model is missing or preload fails, the app shows a
recoverable message with Retry warm-up. Open Ollama first if it is not running;
Live Coach does not install/start Ollama or pull weights automatically. Ready
means preload completed; it does not guarantee zero latency for later inference.

## Use

1. Open Conversation settings. Pick an existing speech model and a locally installed Ollama model. Ollama must be running at `127.0.0.1:11434`. The tested installed model is `gemma4:latest`; no model is pulled automatically.
2. Choose a context/profile and check-in duration (10 minutes by default). Quiet mode generates coaching only when a quick action is pressed.
3. Inform participants as appropriate, acknowledge capture, then press Start conversation. Stop ends both streams and cancels coaching. Closing the window also tears down capture.
4. Read the rolling transcript, recent observation, confidence and short response. Suggest Reply, Analyze Pattern and End Gracefully use current context. The timer is a check-in reminder; it does not automatically end the conversation.

Microphone and System audio labels identify capture channels, not people. Simulator labels A/B are fictional. No reliable speaker diarization is claimed. Use headphones to avoid the microphone also picking up speaker playback.

A conservative output guard reduces multiple reply options to one sentence and substitutes a low-confidence observation for detected unsupported tone, motive or diagnosis wording. This is a lexical safeguard and can miss paraphrases or reject benign wording.

The coaching prompt supports autonomy, boundaries, clarity and constructive repair. It asks for calibrated observations about repetition, circular reasoning, guilt pressure and escalating language, separates behavior from intent, and forbids diagnosis or assumed malicious intent. Text cannot establish audible tone or interruption timing. Model output remains fallible.

## Optional local coaching voice — 0.1.2

Live Coach can read a suggestion using a short sample of **your own voice**, conditioned by local Qwen3-TTS. Open **Conversation settings → My voice**, acknowledge your own voice consent, choose **Record my voice**, read the displayed passage, and **Save sample** after 5–30 seconds. Use **Preview my voice** to check it, **Read aloud** beside a reply to hear it, and **Stop voice** to cancel. Speech is manual. Headphones keep suggestions private; transcript processing pauses automatically during playback.

[Voice setup, exact enrollment passage, storage and deletion](docs/VOICE.md). The optional engine requires Apple Silicon and an explicit installation (approximately 1.71 GB of model files plus 452 MB of Python dependencies). It does not silently download models. Enrollment deliberately persists a voice sample independently of transcript retention. **Forget voice** deletes that sample, metadata and generated speech. The user's personal sample has not yet been enrolled or evaluated.

Optional speech uses [MLX Audio 0.5.9](https://github.com/Blaizzy/mlx-audio/tree/v0.5.9) (MIT) and [Qwen3-TTS 0.6B Base 4-bit](https://huggingface.co/mlx-community/Qwen3-TTS-12Hz-0.6B-Base-4bit) (Apache 2.0), installed separately rather than bundled.

## Models and providers

Live Coach scans these directories read-only:

- `~/Library/Application Support/Meetily/models`
- `~/Library/Application Support/com.meetily.ai/models`
- `~/Library/Application Support/com.dnalevity.livecoach/models`

Compatible Parakeet directories contain `encoder-model[.int8].onnx`, `decoder_joint-model[.int8].onnx`, `nemo128.onnx`, and `vocab.txt`. Compatible Whisper files use the upstream `ggml-*.bin` format. Decoder loading verifies compatibility before capture starts. Missing/corrupt/incompatible files show an error. No speech model weights are bundled or downloaded. Existing Meetily weights are never moved or modified.

Ollama is auto-detected by service availability and common installation paths; choose any installed compatible chat model. An unavailable service, invalid model, malformed response or timeout produces a recoverable notice; transcription continues and actions can be retried.

Optional OpenAI-compatible HTTPS providers, including Venice, require a configured endpoint/model, a key saved in macOS Keychain, and fresh external-processing consent for each session. A persistent warning remains visible while that provider is selected. The key uses service `com.dnalevity.livecoach`, account `api-key`, and is never saved in web storage or transcript files. Forget deletes it. External providers were not contacted during verification; live key/provider interoperability remains unverified. Local Ollama requests disable proxies/redirects and use loopback only. If you run a proxy in place of Ollama, that service controls its own downstream data handling.

## Privacy and retention

Call audio exists only in bounded in-memory capture/processing buffers and is not saved. Explicit voice enrollment is a separate, opt-in microphone recording: it saves your own voice sample and profile metadata locally until **Forget voice**. Transcript retention settings do not expire the voice profile. Generated speech is temporary. Live Coach has no meeting database, analytics or updater. See [voice privacy and deletion](docs/VOICE.md#storage-and-deletion). Listening requires explicit Start plus the participant acknowledgment every session. Cloud transcript processing has separate per-session consent.

Transcripts and coaching context are ephemeral by default. Stop clears the backend context and the displayed non-retained conversation/insight. Preferences and relationship profiles are saved locally; do not place secrets in profile context. API keys are held in Keychain.

Opt-in retention saves up to the most recent 10,000 finalized session segments at Stop/normal close under `~/Library/Application Support/com.dnalevity.livecoach/transcripts`, with 0700 directories, 0600 JSON files and a configurable 1–365 day deadline. The inference context is separately limited to 120 segments/24 KB. A crash loses unflushed session retention. View retained transcripts shows saved files without mixing them into the live conversation. Delete now stops capture, clears live context, pending inference and retained files, and disables retention for that session. Deletion is filesystem removal, not forensic secure erasure or deletion from backups. Expiry runs on startup, while the app runs and when history is opened; files are not deleted while the app is closed.

## Develop and package

See docs/BUILD-PREREQUISITES.md and docs/ARCHITECTURE.md for the inspected source/API mapping, dependencies and native pipeline. Core conversation coaching needs no separate Meetily/Python backend or FFmpeg. Optional local cloned speech uses a separately installed Python/MLX runtime; see [VOICE.md](docs/VOICE.md).

```sh
cd live-coach
pnpm install --frozen-lockfile
pnpm test
pnpm dev
# Locally installable development bundle, ad-hoc signing:
pnpm exec tauri build --debug --bundles app
mkdir -p dist
ditto --norsrc --noextattr "src-tauri/target/debug/bundle/macos/Live Coach.app" "dist/Live Coach.app"
```

`rust-toolchain.toml` selects Rust 1.97.0; install that toolchain with rustup before building. Cargo.lock and pnpm-lock.yaml pin dependencies. The first build fetches native dependencies including ONNX Runtime, not speech model weights. Development bundle output: `src-tauri/target/debug/bundle/macos/Live Coach.app`.

For Developer ID builds, supply your signing identity in a local Tauri configuration override; credentials must be supplied separately for notarization. Verify `codesign`, Gatekeeper status and stapling rather than treating a present certificate as success.

## Verification and known limits

See docs/VERIFICATION.md for actual results and evidence. A simulator or mocked HTTP test alone is not proof of working capture. The bundled CLI self-test explicitly processes only the synthetic fixture and can test simultaneous hardware capture:

```sh
"dist/Live Coach.app/Contents/MacOS/live-coach" --self-test \
 "$HOME/Library/Application Support/com.meetily.ai/models/parakeet/parakeet-tdt-0.6b-v3-int8" \
 tests/fixtures/boundary.wav --capture
```

The capture probe audibly plays the synthetic fixture, reads mic/system samples, runs the shared STT and coaching paths and verifies teardown. Run it only in an appropriate private test environment; it requires audio permissions and uses the locally installed `gemma4:latest`. It does not pull models or save real audio.

The development recognizer uses a fixed energy threshold with 0.6-second silence endpoints and five-second maximum segments. Very quiet speech can be missed; boundaries can split words; transcription can be wrong. Bounded queues drop work with a visible warning when overloaded. No echo cancellation, reliable speaker identities, automatic device switching, or acoustic-tone analysis. Restart capture after changing devices/sample rates. In-flight native decoding finishes before Stop returns; incomplete tail audio is intentionally discarded. Intel builds, live Whisper recognition, and external-provider Keychain interaction have not been tested here. This is a development build, not a production-quality conversation authority.

## Synthetic audio fixture

Before running audio integration checks on macOS, run `./scripts/generate-fixture.sh`. This creates the known synthetic Samantha fixture; no real conversation or user voice sample is distributed.
