# Turn selected words into a reviewed task

You can talk normally with a friend. Live Coach does not listen for task commands, wake words or implied instructions. Nothing you say automatically connects to Codex, creates a task, requests a task review or sends a handoff. The conversation coach remains separate; Quiet mode keeps coaching updates on request.

## Use during a call

1. In Codex projects, connect, choose your project and select any files or Codex history you want to use. Keep the conversation consent and audio controls separate.
2. Start a consented conversation. Once finalized words appear, click **Draft Codex task…** below the transcript.
3. Choose one or more excerpts. None are selected automatically. Check that these are the words you intended to use: microphone/system audio labels describe channels and cannot verify who spoke. Correct recognition mistakes and edit the draft, up to 4,000 characters.
4. Click **Use as task draft**. If another draft exists, inspect the displayed existing draft before choosing **Replace task draft**. This copies the selected words locally; no Codex request is made. Selected project, files and history stay selected.
5. Check the task in Codex projects. Access resets to **Plan**, which permits read-only planning. Choose Task yourself only if you intend project changes.
6. Choose **Review task**, inspect the exact text, project, history, model, provider and scope, then **Send reviewed task**. The selected content now goes to OpenAI through your Codex sign-in. Permitted commands can run inside the reviewed scope; additional approval requests wait for your response.

Incoming speech never edits an open excerpt selection, task draft or review. New task transfer is blocked while Codex is busy. Historical instructions and conversation quotes cannot grant permission to tools.

## Stop, delete and cancellation

Stop, Delete now and a new conversation immediately clear the excerpt picker, copied conversation draft and its unsent review, including when transcript retention is enabled. The backend also revokes the pending speech-derived review token. A review from a different or stopped session cannot be prepared. Unrelated manually written drafts are preserved.

A task you already reviewed and sent continues if the conversation stops; use **Cancel task** to interrupt it. Deleting local text cannot retract context already sent to OpenAI. These controls never automatically speak into your call.

The picker holds at most 100 recent finalized excerpts and expires after five minutes. Evicted, cleared or old-session excerpts cannot be used. Codex history has its own snapshot expiry; refresh its preview if a selection expires or a task reconnects Codex. Read [Codex integration](CODEX-INTEGRATION.md) for execution limits and disclosure.
