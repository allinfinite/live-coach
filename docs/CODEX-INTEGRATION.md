# Codex and Dot integration

Status: researched integration design plus a tested, local-only handoff draft utility. Live Coach does **not** currently read Codex history, send tasks to Dot, or execute commands. Personal coaching is the first product priority.

## Available interfaces

The installed Codex CLI was inspected on 2026-10-16 (0.161.0): `app-server`, `exec`, `resume`, `queue`, and protocol schema generation are available. CLI availability does not establish account access or successful inference.

The documented app-server protocol supports stdio, initialization, thread listing/reading, starting turns, streaming events, cancellation and approval requests. Use generated bindings from the installed CLI; gate unsupported methods by version. Prefer `thread/list` scoped by selected project and `thread/read` only for a user-selected thread. Default listing filters omit some sources, so test actual desktop thread visibility. Do not parse private SQLite or rollout storage. [Official app-server protocol](https://learn.chatgpt.com/docs/app-server)

A child process on this computer does not imply local inference. Codex's configured provider determines where prompts go. Show the destination before forwarding conversation text.

The Codex app tools available to this development chat can inspect and message tasks under user authorization; they are not automatically callable APIs inside Live Coach. Likewise, app-server is not evidence of a public Dot messaging endpoint. Dot documents messaging/calling in ChatGPT and connected Slack/Teams channels, plus iOS shortcuts; this does not establish an external macOS messaging API. [Dot messaging](https://learn.chatgpt.com/docs/dots/channels)

## Use the draft helper now

From the Live Coach source directory:

```sh
python3 scripts/codex-handoff.py \
  --project /absolute/path/to/chosen/project \
  --goal 'Help me prepare a clear boundary for this conversation.' \
  --context notes-for-this-conversation.md
```

`--context` is optional and repeatable. The helper reads only those selected UTF-8 files, validates containment (including resolved symlinks), rejects hidden/key files, and limits each file to 32 KiB and the total to 64 KiB. It prints a JSON draft to stdout without saving, sending, invoking Codex or running a shell. Review the content, then paste it into a chosen Codex chat yourself. Explicitly selected ordinary files may still contain secrets: the filename checks are not a secret scanner. Avoid selecting such files. Do not save drafts in this public repository.

Test:

```sh
python3 -m unittest discover -s tests -p test_codex_handoff.py -v
```

Six tests cover exact file selection, traversal, symlink escape/hidden targets, size limits, binary rejection, and shell-like goal text being kept as data. No provider requests, history reads or task dispatch were made during verification.

## Proposed product flow

1. Keep coaching and action execution separate. Conversation audio and other participants' words are untrusted data, never authorization to use tools.
2. Let the user choose a project and individual context sources. Keep private relationship context separate from project context. Do not enumerate all personal history on launch or continuously scrape the screen.
3. Produce a concrete handoff card with goal, target project/thread, selected excerpts, provider destination and requested scope. Allow edits and removal before a deliberate Send action. Selecting a project is not permission to send all its contents.
4. Start with plan-only handoffs. A future execution mode must name its allowed directories/actions and retain Codex sandbox enforcement. The UI must render command, file-change and permission requests rather than auto-approve them. Unrecognized requests fail closed; cancellation interrupts the active turn and tears down the child process.
5. Use the user-reviewed, bounded scope for routine implementation. Require a new deliberate action for sending messages, publishing, purchases, deletion or expanded access. Never infer authority from a cloned voice or transcript phrase.
6. Keep an optional minimal local action log (request, scope, outcome), with deletion and retention controls. Do not persist conversation excerpts by default or bundle history/tokens with the app.

Read-only sandboxing alone is not a guarantee against data disclosure or side effects through configured connectors. The future adapter needs an isolated, reviewed tool configuration, provider disclosure and tests for every exposed action. Never pass a transcript-derived string to `sh -c`, and never use sandbox/approval bypass flags.

## Before enabling an app-server adapter

- Verify initialization and supported schema against the installed CLI without exposing a network listener.
- Test selected-project history filtering, pagination and thread identity; handle unavailable desktop/Dot history honestly.
- Test denial, pending approvals, interruption, process crash, stale replies and application teardown.
- Verify that provider credentials remain managed by Codex; do not copy authentication files into Live Coach.
- Run an actual approved end-to-end task in an expendable project, checking sandbox boundaries and outcomes before advertising execution.

Autonomous conversation continuation is a separate future mode. It needs explicit participant disclosure, turn-taking/interrupt handling, verified audio routing and a stop control. The current cloned voice is for private coaching playback; it is not permission to impersonate the user to call participants.
