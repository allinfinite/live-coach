# Verification — 0.1.2 local voice update

## Local voice engine — October 16, 2026

Installed MLX Audio 0.5.9 and full dependency lockfile in the application's private support directory. Installed the Qwen3-TTS 0.6B Base 4-bit model at pinned revision `0d6bb6fe33f92d47a507e23b9148940e8366ab5b`. Model weight files total 1,706,783,792 bytes; the complete repository is approximately 1.71 GB. Runtime dependencies occupy approximately 452 MB. Directory permissions were verified as 0700. Repeated setup reused downloaded files without another model download.

Eight Python contract tests passed, covering status without model loading, missing-model handling, private output paths, symlink escape rejection, text bounds, protocol recovery after invalid requests, explicit test roots, and enforced offline environment flags.

The real local model also passed a two-request integration test in one persistent process:

| Check | Result |
|---|---|
| Reference | Existing synthetic boundary fixture, 5.743125 seconds |
| Requested text | “Let me take a moment to think.” |
| Cold synthesis | 32.758997 seconds, including imports and model loading |
| Warm synthesis | 0.831019 seconds in the same process |
| Both output WAVs | 24,000 Hz, mono, PCM16, 1.68 seconds, nonzero audio |
| Model reuse | Status changed from unloaded to loaded |
| Pipe protocol | Four valid JSON responses; library diagnostics stayed on stderr |
| Network behavior | Hugging Face/Transformers offline mode enabled during generation |

Evidence relative to the workspace root: `evidence/local-voice/voice-smoke.json`, `voice-fixture.wav`, `voice-fixture-warm.wav`, and `engine-stderr.log`. The test uses `voice-runtime/smoke_test.py` and never enrolls its synthetic fixture as the user's profile. The generated words were not semantically evaluated by a second speech recognizer in this engine test.

## Privacy boundaries

Call audio is not saved. Voice enrollment is a separate, explicit microphone recording: it persists `voice-profiles/reference.wav` and `profile.json` until **Forget voice**. Transcript retention settings do not govern this voice sample. Temporary synthesized speech belongs in `voice-output/`; the app removes it after playback/cancellation and clears stale generated files on startup. The Python engine neither captures nor plays audio.

## Remaining limitations

The user's own reference sample has not yet been enrolled or evaluated. Engine smoke tests establish real local generation from a synthetic reference; they do not establish personal voice resemblance. Intel Macs are unsupported for the optional MLX voice engine. Cold initialization can take tens of seconds and competes for memory with transcription and Ollama. The app remains ad-hoc signed, not Developer ID signed or notarized. Live Whisper and external-provider integration remain unverified unless documented by a subsequent check.

See [VOICE.md](VOICE.md) for setup, the exact enrollment passage and deletion instructions.


# Earlier conversation pipeline verification — October 16, 2026

For the 0.1.1 automatic startup warm-up update, see [OLLAMA-WARMUP.md](OLLAMA-WARMUP.md).

Verified on this Apple Silicon Mac (macOS 26.5.2), using the packaged development executable, the existing Meetily Parakeet model and installed Ollama `gemma4:latest`. This report distinguishes native streaming checks from mocks.

| Check | Result and evidence |
| --- | --- |
| Rust unit/integration suite | **20 passed, 0 failed, 1 ignored**. The ignored upstream hardware test is replaced by the explicit hardware probe below. Covers consent, duplicate finalized segments, prior-session ASR rejection, debounce/context bounds, stale HTTP cancellation, quiet/manual actions, service failure, cloud consent, output guards, segmentation, resampling, bounded overload, retention permissions/expiry/deletion isolation. |
| Browser UI assertions | **26 passed** using mocked Tauri. Tests consent, simulator selection, event rendering, source labels, one insight/reply, errors/recovery, timer reset, quiet/settings locking, persistent cloud warning, retained history and stop/delete behavior. This does not prove capture or inference. |
| Real native ASR fixture | Actual installed Parakeet ONNX engine, shared energy segmenter and sinc resampler. **0.94 seconds** in final run. Recognized the synthetic boundary speech, with some word errors and a split at the five-second boundary. |
| Real local coaching | Fixture finalized segments entered the same Rust Coach path used by capture, reached real localhost Ollama, and emitted structured Insight. **2.55 seconds** from finalized ingress including debounce, in the final warm-model run. One spoken sentence and calibrated fallback verified. |
| Simultaneous hardware capture | Packaged executable opened both CPAL microphone and Meetily Core Audio tap, played only the synthetic fixture, and transcribed both sources. At 12 seconds: **577,536 mic samples / 574,464 system samples**, peaks **0.7849632 / 0.79910517**. Four finalized segments entered Coach directly from capture callbacks. Actual Ollama Insight returned from those hardware segments. |
| Capture teardown | Both capture and decoder threads joined; no late callbacks after Stop (observed for 500 ms after join). Old session tokens reject late asynchronous ingress. Capture stopped before waiting for the final coaching response. |
| Native macOS UI / IPC | Installed app cold-opened to Ready, with acknowledgment unchecked and no capture. Native simulator emitted four transcript events, quiet mode stayed quiet, End Gracefully reached real Ollama and displayed: “I need to pause this conversation for now and revisit it later.” Stop cleared text/insight and reset consent/timer. Native window close exited the process. |
| Build / packaging | Tauri debug macOS bundle built successfully. Installed copy and a fresh extraction of the ZIP pass `codesign --verify --deep --strict`. Bundle identity is `com.dnalevity.livecoach`; arm64; minimum macOS 14.4. MIT attribution and collected third-party license notices are bundled. |
| Signing / Gatekeeper | **Ad-hoc signed only.** Developer ID attempt returned `errSecInternalComponent`; notarization credentials unavailable. `spctl --assess` rejects the build, as expected without accepted distribution signing/notarization. README gives local opening instructions. |
| Existing Meetily | Checked all **26 original files** for size and nanosecond modification timestamp, checked file-set additions/deletions: **no changes** under installed Meetily or its two support directories. Upstream source checkout also has no modifications. This is a metadata/file-set regression check, not an independent before/after content hash. |

## Reproduce

From the project directory:

```sh
pnpm test
scripts/self-test.sh "$HOME/Library/Application Support/com.meetily.ai/models/parakeet/parakeet-tdt-0.6b-v3-int8" --capture
python3 scripts/verify-meetily.py ../evidence/meetily-before.json
```

The capture probe is explicitly opt-in, audibly plays a synthetic fixture and captures for 12 seconds. It requires mic/system permissions and the installed local model. No human conversation is needed. Model weights are never downloaded. The CLI uses the same capture, decoder, finalized-ingress and coaching implementations, rather than inventing a separate mock recognizer.

The reusable browser assertion function is in `tests/ui-browser.js`; it accepts a Playwright page and URL with a static server serving `ui/`. Its injected Tauri adapter is intentionally mocked. The native app checks were performed separately through the macOS UI.

Evidence is stored in the parent workspace `evidence/`: `rust-tests-final.log`, `streaming-final.log`, `package-final.log`, `signature-final.txt`, `gatekeeper-final.txt`, `archive-verify.txt`, `meetily-after-check.json`, `native-coaching.png`. Earlier failed build/signing/inference logs are retained as superseded investigation evidence. `live-coach/evidence/ui.png` is the browser screenshot.

## Material limits

- This is a tested development build, not a production release. Apple Silicon only was built/tested. Intel, real Whisper decoding and live external-provider/Keychain interoperability were not exercised.
- No reliable speaker identities, acoustic-tone inference, echo cancellation or automatic device-change handling. Source labels are channels. Restart capture after device/rate changes; use headphones.
- Fixed energy gating can miss soft speech or pass noise; ASR can hallucinate short words in noise. Five-second splits can cut words. The bounded queue drops segments with a warning if overloaded.
- Coaching is model-generated. Prompting plus a conservative lexical output guard cannot guarantee correct interpretations or eliminate every unsupported claim; the guard can also reject benign phrasing. Suggested replies are reduced to one sentence. Confidence is qualitative, not a measured probability.
- Automatic inference waits for a brief pause and is limited to one start per 12 seconds; fresh segments cancel stale work. Slow models or continuous speech can delay updates. Explicit actions bypass the automatic interval. Measured latency is not a cold-model or worst-case guarantee.
- Stop discards incomplete tail audio and may wait for an in-flight native decode. No call audio is retained. Explicit own-voice enrollment separately retains its reference sample until Forget voice; see VOICE.md. Opt-in transcript retention flushes on Stop/close/normal Quit; a crash loses unflushed retention. Expiry only runs while the app is running or reopened.
- OS permission denial, unplugging devices and every Bluetooth configuration were not exhaustively tested. System permission denial can manifest as silence; successful stream creation alone is insufficient evidence of capture.


## Local voice update — 0.1.2

33 Rust tests pass, with two deliberately opt-in native probes ignored by the default suite; 8 Python contract tests and106 browser assertions pass. Explicit real Rust→local Qwen engine→afplay playback passed twice using a synthetic fixed-passage reference (no microphone capture or user profile): cold10.689s and warm3.909s including playback. Cancellation killed a real sleeping subprocess and cleaned output; generated files were removed after real playback. The integration test found and fixed canonical `/var`/`/private/var` path handling. Pause-gate tests reject partial, queued and in-flight transcript results across playback.

The built0.1.2 CLI separately decoded the real synthetic boundary WAV using read-only installed Parakeet in1.02s, sent finalized segments through the actual coaching ingress and local Ollama, and returned coaching in3.06s (`SELF_TEST_PASS`). This run did not open microphone/system capture. Previous simultaneous hardware capture evidence remains above; live acoustic feedback while speaking is not newly measured.

The installed native0.1.2 window displayed local coach ready, idle timer10:00, conversation consent unchecked, empty transcript, Read aloud disabled and local voice runtime ready for enrollment. No enrolled user voice existed during verification. The user must explicitly record and save their own reference to evaluate resemblance. The optional model is already installed on this Mac; no startup model download or audio capture occurs.

Development app packaging succeeded. Installed app and freshly extracted ZIP pass strict deep signature verification using ad-hoc signing; notarization remains unavailable. Original Meetily snapshot still reports26files and zerochanges. Evidence: `voice-rust-tests-final.log`, `voice-python-tests.log`, `voice-ui-tests.json`, `voice-coaching-regression.log`, `voice-meetily-check.json`, `voice-package.log`, `local-voice/rust-playback-smoke.json`.


## Personal coaching expansion — 0.1.3

38 Rust tests pass (2 explicit native probes remain ignored by default);123 browser assertions cover originalUI, warm-up, voice and17mode checks. Six Codex draft-helper tests pass. Mode tests inspect actual HTTP request payloads for Personal/Sales/Secretary, legacy settings defaults, goal/intention bounds and separation of supplied text from system instructions. Two cancellation regressions verify Stop and shutdown invalidate accepted playback before synthesis starts.

Packaged0.1.3 real Parakeet→finalized segment ingress→local Ollama passed:1.15s ASR and2.95s coaching on the synthetic boundary fixture. This test did not capture microphone/system audio. Native startup shows Personal coach, goal/intention fields, local coach ready, stopped capture and the existing enrolled voice ready. Saved voice file size/modification metadata remained unchanged; no private sample was copied into source or evidence. Original Meetily26file snapshot remains unchanged.

Public export contains only reviewedsource/docs/assets and passes credential-pattern and personal-absolute-path checks. No voice profiles, models, recordings, personal transcripts, Codexhistory or local evidence are included. Source is published at https://github.com/allinfinite/live-coach; the project page is https://dnalevity.com/live-coach. Dot messaging, automatic call participation and command execution remain unimplemented; the included helper only prepares a local reviewable draft.


## Reviewed Codex handoffs — 0.2.0

67 Rust tests pass; two existing native audio/voice probes remain opt-in. Forty Codex browser assertions and 123 existing UI/voice/warm-up/mode assertions pass. Six legacy handoff-helper tests and 8 voice-engine contract tests pass. Nine real Codex isolation checks exercise project-only reads, denied sibling/Codex-home reads, restricted thread-profile acknowledgment, disabled command networking and absent MCP startup markers.

The actual packaged app-server integration used the existing Codex sign-in and real OpenAI inference in a disposable synthetic project: Plan returned both selected facts; Task created the exact requested result.txt; reused review identifiers were rejected; immediate startup cancellation disconnected cleanly. The installed native app independently completed Connect, file selection, full review, explicit Send, streamed result and Disconnect. Cold startup had zero child processes before Connect and audio remained stopped. Saved voice-profile size/modification metadata and the 26-file Meetily snapshot remain unchanged.

Ad-hoc signed installed app and freshly extracted development ZIP pass strict deep signature verification. Notarization credentials remain unavailable. This release was tested against Codex CLI 0.161.0 on Apple Silicon macOS. Other CLI versions fail closed if required configuration/profile acknowledgments are unavailable. ChatGPT authentication was exercised; API-key routing is unit-tested but not exercised against a paid API account. Live history selection and direct Dot messaging remain unavailable.

Real integration found and fixed auth-aware endpoint routing (ChatGPT sign-ins need the Codex backend rather than the public API endpoint), inherited custom command environments, cancellation during initialization, stale approval/turn handling and reviewed-folder identity changes. Broad legacy sandbox overrides are deliberately not used. Plan cannot approve scope expansion; Task-mode one-time exception approvals require careful user review.

Reproduce the controlled real provider test, which uses account inference and creates/deletes only its own synthetic project:

```sh
./src-tauri/target/debug/live-coach --codex-self-test
```

Local verification evidence is excluded from the public repository. Reproducible test code and sanitized test summaries are included.
