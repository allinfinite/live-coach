# Deliberate conversation-to-Codex drafts

- [x] Inspect transcript rendering and immutable Codex review boundaries.
- [x] Add an explicit local transcript excerpt picker and editable task draft; no voice triggers or automatic inference.
- [x] Keep selected project/history, prevent silent draft replacement, and default spoken drafts to read-only Plan.
- [x] Verify ordinary/hostile conversation, stale text, deletion, busy tasks, privacy and review/Send gating.
- [x] Exercise real finalized transcript ingress and a reviewed streaming handoff using synthetic speech.
- [ ] Package, install, publish and document the verified update.

Design check-in: Ordinary speech is data. Only an intentional button opens the draft picker; only selected words are copied locally. No wake words, speech intent classifiers, automatic Codex connection, task creation or Send. The person reviews/edit words regardless of uncertain speaker identity. Existing selected project/history are reused, with their expiry rules. An existing task draft cannot be silently replaced. Conversation-derived drafts default to Plan; Task access requires an explicit choice. Stopping any conversation, deleting transcripts or starting a new session clears unsent conversation-derived draft context. Nothing automatically speaks to a friend or runs commands.

## Review
74 Rust tests (two existing explicit native probes excluded), 54 new safety browser checks and all 192 existing UI checks pass. 120 ordinary/hostile/trigger-like segments produce zero Codex calls. Nine actual Codex isolation checks pass. Installed real synthetic speech recognition and finalized-session ingestion reached a reviewed Plan with selected project/history; actual provider streaming passed, cleared tokens were rejected, source chat unchanged. Native no-audio simulator/excerpt/edit/draft/review/Stop flow passed; Stop immediately removed unsent context. Existing settings restored. Meetily 26-file snapshot and voice metadata unchanged. Source export audit: 77 files, zero findings. Packaging/publication pending final source commit.
