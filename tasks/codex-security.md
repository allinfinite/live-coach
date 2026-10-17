# Codex isolation review

- [x] Inspect actual installed CLI 0.161.0 help and generated protocol.
- [x] Verify config layering against official upstream loader source and config reference.
- [x] Keep existing Codex authentication managed by Codex; no auth copying, database scraping or rollout parsing.
- [x] Disable every configured MCP name before startup; reject ambiguous override paths.
- [x] Disable optional hooks/connectors/plugins, external notification commands and unrelated tools.
- [x] Use fresh named permission profiles with minimal runtime + chosen-project reads/writes and no command network.
- [x] Verify actual sandbox outside-project denial, effective config and thread profile acknowledgment without inference.
- [x] Add exact-profile/backend boundary unit tests and documented limitations.
- [ ] Root integration verification: actual reviewed provider task, command/change denial, teardown and installed UI.

## Findings

- `mcp_servers={}` deep-merges and does not remove inherited servers. Disable server names individually and verify the effective configuration.
- Quoted dotted CLI override paths are not parsed as TOML paths by installed 0.161.0. Ordinary safe connector names work; dot/quote names must fail closed.
- Explicit `untrusted` approval policy is rejected by runtime despite generated schema. `on-request` plus user reviewer and sandbox enforcement is supported.
- Ordinary read-only/workspace-write sandbox modes grant broad reads. A named permission profile `{filesystem={":minimal"="read","/absolute/chosen/project"="read" or "write"},network={enabled=false}}` narrows actual reads. Do not override it with legacy thread `sandbox` or turn `sandboxPolicy`.
- Native `codex sandbox` test read of a project fixture succeeds; sibling fixture read fails `Operation not permitted`. App-server acknowledges the named profile even with inherited `sandbox_mode="danger-full-access"`.
- Config/read serializes nullable filesystem metadata; verification normalizes `glob_scan_max_depth:null` only, and compares the generated scope exactly.
- `CODEX_APP_SERVER_TEST_USER_CONFIG_FILE` is debug-only; not a release isolation mechanism. Public `--profile` layers user config and does not replace it. Do not depend on either for isolation.

## Evidence

`python3 scripts/verify-codex-isolation.py`: nine checks passed, no model turn, no user authentication read/copied. Sanitized result: `tasks/codex-isolation-verification.json`.

Official sources inspected 2026-10-16:
- https://learn.chatgpt.com/docs/app-server
- https://learn.chatgpt.com/docs/config-file/config-reference
- https://github.com/openai/codex/blob/main/codex-rs/config/src/loader/mod.rs
- https://github.com/openai/codex/blob/main/codex-rs/config/src/permissions_toml.rs
- https://github.com/openai/codex/blob/main/codex-rs/core/src/config/permissions.rs
- https://github.com/openai/codex/blob/main/codex-rs/app-server/src/lib.rs

Only the installed CLI probes establish version-specific behavior; upstream main may differ.
