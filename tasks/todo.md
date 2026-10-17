# Project and Codex history pickers

- [x] Inspect installed history schemas and current review pipeline.
- [x] Add native folder choice and recent Codex project/history selection without starting turns.
- [x] Preview selected history messages and freeze only explicitly selected text into the immutable task review.
- [x] Verify stale selection, bounds, prompt injection boundaries, failures and cancellation.
- [x] Exercise real history listing/reading and an actual reviewed streaming handoff with synthetic context.
- [ ] Package, install and publish the tested development update; record results.

Design check-in: Browsing reads local Codex history through app-server, never resumes or changes an existing chat. History text is reference material, not authority. No task starts before the existing explicit review/Send step. Native folder selection does not require typing paths. Recent project choices come from local history metadata, without scanning arbitrary directories. No automatic transcript inclusion. Existing authentication is reused; no new credentials or API keys.

## Review
72 Rust tests pass (two existing native audio/voice probes remain explicit). All 192 browser checks pass: 29 picker, 40 Codex, 123 existing regressions. Nine real Codex isolation checks pass. Installed actual history listing/reading and reviewed history-only inference pass; source conversation unchanged. Owned synthetic fixture archived. Actual native folder selection, review, Send, streamed result, search and Disconnect pass. Capture remains stopped, voice metadata and 26 Meetily files unchanged. Public export audit reports 74 source files, zero findings. Installation and ZIP signature checks pass. Publishing follows final source commit.

