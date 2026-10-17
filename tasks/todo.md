# Reviewed Codex project handoffs

- [x] Verify installed app-server schema and isolate optional hooks/connectors from reviewed tasks.
- [x] Add explicit connection, project/file context selection and bounded local previews.
- [x] Require an immutable review token for exact task, context, model, provider and scope before task start.
- [x] Stream progress/results and surface single-action approvals; support cancellation and shutdown.
- [x] Test unreviewed/stale requests, context boundaries, errors, denial and process teardown.
- [x] Exercise an actual Codex task in a disposable project, then package and install the verified update.
- [x] Publish source and a development release with accurate integration documentation.

Design check-in: Codex is a separate optional connection, never started by opening Live Coach or by audio/transcript events. Project context consists of explicitly selected UTF-8 files. Review is local; Send is the deliberate inference/task start. Plan uses read-only sandboxing; Task allows the selected project as the write scope. Codex provider processing can leave this Mac and is disclosed before Send. Pending command/change requests remain pending until user review. Unknown permission/tool requests fail closed. No automatic approval caches or permanent rules.

## Review
Installed 0.2.0 and verified actual packaged Codex inference plus the native review, Send, result and Disconnect flow. 67 Rust tests, 40 new Codex UI checks, 123 existing UI checks, 9 real permission checks, 6 legacy helper tests and 8 voice-engine tests pass. The source export scan reports 72 files and zero credential/personal-path findings. Existing voice profile metadata and 26 Meetily files remain unchanged.

Final hardening disables inherited telemetry export and rejects custom provider authentication commands. GitHub source is pushed to the review branch; the token cannot create a pull request. The verified development ZIP and SHA-256 are published at https://github.com/allinfinite/live-coach/releases/tag/v0.2.0. Final installed-build inference, project write and cancellation passed after hardening. Cold startup shows unchecked capture consent and local coach ready.

Limits: Codex CLI 0.161.0 tested; no history picker, Dot API or automatic transcript inclusion. ChatGPT sign-in tested; API-key routing only unit-tested.
