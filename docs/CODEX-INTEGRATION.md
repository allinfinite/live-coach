# Reviewed Codex project handoffs

Live Coach can connect to the installed Codex CLI, preview selected project context, and send a task after you review its exact prompt and access. The connection is optional and starts only when you click **Connect Codex**. Conversation capture and voice playback do not connect to Codex or authorize tasks.

## First use

1. Open Codex and sign in, or run `codex login` in Terminal. Live Coach reuses that Codex-managed login without reading or copying authentication files.
2. Open Live Coach's **Codex projects** section and click **Connect Codex**.
3. Click **Choose project folder** to use the macOS folder picker. Alternatively, click **Browse Codex history**, then **Use this project** beside a conversation. Select only the project files needed for the task. Manual path entry remains available as an optional shortcut. Select a model from those returned by your Codex installation.
4. Optionally search Codex history and click **Preview messages**. Check only the user or assistant messages you want to include; none are selected automatically. The preview is local and does not run or resume a task.
5. Choose **Plan** for advice without project changes, or **Task** to let Codex make changes within that project. Describe the desired outcome.
6. Prepare the review. Read the project, provider, access, complete prompt and selected file contents. Click **Send reviewed task** to begin.
7. Read the streamed result. If Codex requests approval, inspect the command or proposed change before choosing **Allow once**, **Decline**, or **Cancel task**. An incomplete or truncated approval cannot be accepted.

A useful first task is: “Read these project notes and suggest three concrete next steps. Do not change any files.” Use a disposable project for your first Task-mode experiment.

Codex inference uses **OpenAI through your existing Codex sign-in**. Sending a task sends its prompt and selected context off this Mac; your Codex account's usage applies. The local Ollama conversation coach remains a separate service. This integration currently requires the OpenAI provider and does not silently use custom provider endpoints.

## Context and review

The file chooser lists ordinary project text files and excludes hidden files, symbolic links, dependency/build folders, model folders and voice data. Context is limited to eight files, 32 KiB each and 64 KiB total. The review contains a snapshot of the selected text. Editing the task, project, mode, model or selection invalidates the preview; the backend issues a single-use review identifier with a five-minute lifetime.

The selected files are the **initial prompt context**. Once a reviewed task starts, Codex's sandbox permits reading the chosen project, including files you did not include in that prompt. Select a narrow project folder and review its scope accordingly. Filename exclusions in the initial file list are not a project-wide secret scanner.

Live Coach lists and reads local Codex history through the installed app-server protocol. It does not scrape private SQLite databases or rollout files. Browsing never resumes or messages an existing chat. Each handoff starts a new ephemeral thread; only the messages you select are copied into its reviewed prompt. No screen contents or Dot messages are collected. Search covers history available to this local Codex installation, not cloud ChatGPT conversations or projects on another computer.

History previews include user and assistant text only. Tools, command output and private reasoning are excluded. Previews hold up to 32 messages and 32 KiB of text, with 8 KiB per message, and expire after five minutes. A notice identifies truncated previews. Switching context invalidates the task review. A selected history snapshot remains fixed in the final review, even if the original conversation changes. Historical instructions are reference data, never authorization to run commands. Project choices from history can be unavailable if a folder was moved or deleted; use Choose project folder to locate it.

No audio, rolling transcript or relationship profile is automatically added to a handoff. You can deliberately copy selected finalized words through [Draft Codex task](CONVERSATION-TASKS.md), then edit, review and send them. Text from a selected document, transcript or other participant is reference material, never permission to use tools.

## Execution boundaries

The adapter launches `codex app-server --listen stdio://` as its own child process, with no network listener. It preserves the existing `CODEX_HOME` for Codex-managed authentication and launches from a neutral Live Coach support folder. It does not modify the user's Codex configuration. Inherited telemetry exporters and sensitive-text telemetry logging are disabled; custom OpenAI authentication commands are rejected.

A fresh named Codex permission profile grants the operating system's minimal runtime reads plus the chosen project's read access in Plan mode, or read/write access in Task mode. Command network access is disabled. The profile does not grant general home-directory reads or `/tmp` writes. The backend verifies the effective configuration and the thread's acknowledged profile before starting a turn. It never replaces this profile with the broader legacy `sandbox` or `sandboxPolicy` fields.

Optional apps, MCP connectors, plugins, hooks, browser/computer tools, memory tools, skill installation, notification commands and web search are disabled for these handoffs. The adapter discovers configured MCP **names** locally, disables each one before starting the child, and checks the effective configuration afterward. An empty `mcp_servers={}` override is insufficient because Codex merges configuration tables. Connector names that this CLI cannot override unambiguously, unsupported managed layers, custom model instruction files and unverifiable permissions fail closed.

Send is the authorization to run the displayed task within its displayed scope. Codex can run permitted sandboxed commands after Send; it does not ask about every safe command. Requests that require command/file approval wait for a deliberate response and support one-time approval only. Permission grants, dynamic tools and unsupported server requests are rejected rather than automatically approved. Unknown server requests are rejected; invalid protocol data stops the connection. Plan mode cannot approve scope expansion; re-review a task in Task mode to authorize changes. One-time Task approvals can authorize the displayed exception, so inspect their access and command carefully.

**Cancel task** interrupts the active turn and disconnects the child. Disconnecting or closing Live Coach also terminates its Codex process group, including its active child commands. It does not terminate the user's other Codex sessions.

Codex may retain account-side request data according to the user's account settings. Ephemeral threads and `history.persistence="none"` avoid normal local handoff history, but do not promise that Codex creates no operational logs. Live Coach keeps review/run state in memory and does not copy provider credentials into its own storage or public repository.

## Verification

The installed protocol was generated from Codex CLI **0.161.0**. Its schema advertises `untrusted` approval policy, but the actual runtime rejects explicit `approval_policy="untrusted"`; this adapter uses `on-request` and routes approvals to the user. A schema alone is not proof that a configuration works.

Run the no-inference isolation check:

```sh
python3 scripts/verify-codex-isolation.py
```

It uses an expendable Codex home with no copied credentials. It executes a real sandboxed read inside the test project, verifies that a sibling outside read fails with `Operation not permitted`, then initializes the installed app-server and creates an ephemeral thread. It verifies the restricted named profile, disabled command networking, disabled connector/features and absence of a configured MCP startup marker, including when the test's global sandbox mode is `danger-full-access`. No model turn is started.

Application verification additionally covers review tokens, context traversal/symlink limits, approval denial/replay, stale events, process failure, cancellation and actual provider streaming in a disposable project. See [verification](VERIFICATION.md) for the tested build's results.

## Dot and future modes

The Codex tools available inside a Codex development chat are not a public API automatically available to Live Coach. This app uses the installed app-server protocol. It does not claim a direct Dot messaging endpoint. [Official app-server protocol](https://learn.chatgpt.com/docs/app-server), [configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference), [Dot channels](https://learn.chatgpt.com/docs/dots/channels).

Autonomous conversation continuation remains separate future work. The cloned voice currently reads private coaching suggestions; it does not speak to other call participants or approve project tasks.
