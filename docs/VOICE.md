# Private local coaching voice

Live Coach 0.1.2 can read a coaching suggestion using a local model conditioned on a short sample of your own voice. This feature is optional. It plays suggestions through your Mac's output device; it does not send speech into a call application's microphone. Use headphones so other participants do not hear private suggestions.

## Install the optional engine

This Apple Silicon feature requires Homebrew Python 3.13 and uv. Deliberately running setup downloads approximately **1.71 GB of model files**, plus approximately 452 MB of installed Python dependencies. No voice-model download occurs merely by opening Live Coach or requesting speech.

```sh
brew install uv python@3.13
./voice-runtime/setup.sh
```

The engine uses MLX Audio 0.5.9 and Qwen3-TTS 0.6B Base 4-bit. It runs locally over a private process pipe, with Hugging Face and Transformers offline modes enabled. No API key or voice upload is involved. Missing or incompatible model files produce an error that can be resolved with explicit setup.

## Record your own voice

1. Stop the conversation before enrolling. Open **Conversation settings → My voice**.
2. Acknowledge: “This is my own voice. I consent to recording and storing a sample on this Mac to read coaching suggestions. I can delete it with Forget voice.”
3. Choose **Record my voice**, read the exact passage below naturally, then choose **Save sample** after 5–30 seconds. This records only the microphone. **Cancel** discards enrollment.
4. Choose **Preview my voice** to check the result. Use **Forget voice** whenever you want to remove it.

> I am recording my own voice for private coaching. I can pause, think clearly, and choose words that reflect what matters to me. I want to listen carefully and keep my boundaries.

Use a quiet room and your normal speaking voice. Read the passage as written: its transcript conditions the model alongside your audio. You can record again if the sample is noisy. The saved profile belongs to Live Coach and does not affect Meetily.

## Hear a suggestion

Choose **Read aloud** beside a coaching reply. Speech is requested manually, so the app does not continually speak or automatically read new suggestions. Start with a short suggestion and headphones. Choose **Stop voice** during generation or playback to cancel it. Stop the current voice before requesting another reading.

Enrollment requires a stopped conversation. During spoken playback, transcript processing pauses automatically, including a brief tail after speech, to avoid transcribing the app's own voice; the audio devices remain open. It resumes afterward without another Start action. Enrollment and speech do not waive the conversation consent acknowledgment.

## Storage and deletion

Call audio is not retained. **The voice sample is an explicit exception:** choosing enrollment saves your microphone sample and profile metadata so your voice can be reused. They remain until you choose **Forget voice**; transcript retention settings do not delete or expire the voice profile.

The profile is stored under:

```text
~/Library/Application Support/com.dnalevity.livecoach/voice-profiles/reference.wav
~/Library/Application Support/com.dnalevity.livecoach/voice-profiles/profile.json
```

The profile metadata contains the fixed passage, consent acknowledgment and timestamp. Voice directories are private to your macOS user (0700); saved files use 0600 permissions. Generated WAVs are temporary, removed after playback or cancellation, and stale generated files are cleared at startup. **Forget voice** stops enrollment/playback and deletes the sample, metadata and generated voice files. It keeps the reusable model installation.

To remove the optional model and Python runtime as well, close Live Coach and run:

```sh
rm -rf "$HOME/Library/Application Support/com.dnalevity.livecoach/voice-runtime" \
       "$HOME/Library/Application Support/com.dnalevity.livecoach/voice-model"
```

## Verified performance and limitations

The real model was tested offline with the project's **synthetic** boundary fixture, not the user's voice. It generated “Let me take a moment to think.” as valid non-silent 24 kHz PCM16 WAV. First use, including Python imports and model loading, took **32.76 seconds**; a second request in the same process took **0.831 seconds**. Both outputs lasted 1.68 seconds. These are measurements on this Apple M4 Pro Mac, not latency guarantees.

The user's own voice sample has not yet been enrolled or evaluated. Voice resemblance, pronunciation and prosody can vary. Apple Silicon is required; Intel macOS is unsupported for this optional engine. Cold starts can be slow, and local speech generation shares memory and compute with transcription and Ollama. If generation fails, the written suggestion remains available. The app does not train a new model or guarantee exact voice reproduction.

The model is [Qwen3-TTS, Apache 2.0](https://huggingface.co/mlx-community/Qwen3-TTS-12Hz-0.6B-Base-4bit); [MLX Audio is MIT licensed](https://github.com/Blaizzy/mlx-audio/tree/v0.5.9). The pinned model revision is `0d6bb6fe33f92d47a507e23b9148940e8366ab5b`. See [runtime documentation](../voice-runtime/README.md) for the local protocol and tests.
