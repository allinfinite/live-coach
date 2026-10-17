# Contributing to Live Coach

Live Coach is an independent MIT-licensed conversation coach derived from Meetily Community Edition. We welcome improvements that help people stay clear, exercise choice, and communicate boundaries.

## Start here

Read the README and architecture notes. Open an issue describing the user need and expected behavior before a substantial change. Keep pull requests focused and explain how the behavior was tested. Contributions must preserve upstream licenses and attribution.

The current priority is personal coaching for difficult conversations. Sales support, secretary workflows and local project tools are directions for exploration; they are not promises of autonomous calling or computer control.

## Development checks

On a supported Mac, follow the README build prerequisites, then run:

```sh
pnpm install --frozen-lockfile
cargo test --manifest-path src-tauri/Cargo.toml
python3 -m unittest discover -s voice-runtime -p 'test_*.py'
```

Generate the synthetic audio fixture with `./scripts/generate-fixture.sh` before native audio integration checks.

Browser checks under `tests/` exercise consent and UI behavior. Native capture and model integration also need hardware checks; mocked tests alone do not prove the streaming pipeline works. State which tests you actually ran and any unavailable hardware or models.

## Privacy and behavior requirements

- Audio capture requires an explicit action and participant-awareness acknowledgment.
- Voice enrollment requires the voice owner's consent and an explicit recording action.
- Keep local processing local; clearly disclose when an optional remote provider receives text.
- Never add silent large-model downloads, credentials in logs, or retained call recordings.
- Coaching should describe observable behavior with uncertainty, avoid diagnoses and intent assumptions, and support the user's autonomy.
- Keep generated speech under the user's control. Proposed external actions need a clear review boundary.

Use synthetic fixtures. Never submit real conversations, voice samples, API keys, model weights, app databases or local evidence logs. See SECURITY.md for sensitive reports.

## Meetily community

Upstream: https://github.com/Zackriya-Solutions/meetily

Give credit for reused capture and recognition work. Discuss upstream contributions in the appropriate upstream channel and follow their contribution guidelines. Live Coach is not an official Meetily release and does not imply endorsement.
