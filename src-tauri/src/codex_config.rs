//! Defensive configuration for the separately launched Live Coach Codex client.
//! Never serialize config contents or authentication into frontend events/logs.
use serde_json::{json, Map, Value};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct Isolation {
    pub args: Vec<String>,
    /// Flat config overrides accepted by thread/start. Reapply for every new thread.
    pub config: Map<String, Value>,
    pub profile: String,
    pub provider_base: String,
}

/// Read configuration metadata only. Values are parsed locally to discover MCP
/// names, never copied into Live Coach state or returned to the webview.
pub fn build(home: &Path, project: &Path) -> Result<Isolation, String> {
    build_for_scope(home, project, false)
}

pub fn build_for_scope(
    home: &Path,
    project: &Path,
    workspace_write: bool,
) -> Result<Isolation, String> {
    build_for_auth_scope(home, project, workspace_write, true)
}

pub fn build_for_auth_scope(
    home: &Path,
    project: &Path,
    workspace_write: bool,
    chatgpt: bool,
) -> Result<Isolation, String> {
    let provider_base = if chatgpt {
        CHATGPT_CODEX_BASE
    } else {
        OPENAI_API_BASE
    };
    if !home.is_absolute() || !project.is_absolute() {
        return Err("Codex home and project must be absolute paths.".into());
    }
    let mut files = BTreeSet::from([
        home.join("config.toml"),
        PathBuf::from("/etc/codex/config.toml"),
    ]);
    // Include ancestor project layers even when their trust status is unknown.
    for parent in project.ancestors() {
        files.insert(parent.join(".codex/config.toml"));
    }
    let mut names = BTreeSet::new();
    let mut environment_names = BTreeSet::new();
    for file in files {
        match std::fs::metadata(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("Cannot inspect Codex configuration safely.".into()),
            Ok(m) if !m.is_file() || m.len() > 2 * 1024 * 1024 => {
                return Err("Codex configuration is not a bounded regular file.".into())
            }
            Ok(_) => {}
        }
        let text = std::fs::read_to_string(&file)
            .map_err(|_| "Cannot read Codex configuration safely.")?;
        let value: toml::Value = text
            .parse()
            .map_err(|_| "Cannot parse Codex configuration safely.")?;
        reject_command_provider_auth(&value)?;
        collect_names(&value, &mut names);
        collect_environment_names(&value, &mut environment_names);
    }
    let mut config = Map::new();
    for name in names {
        // Installed Codex 0.161 splits override paths literally: TOML-quoted
        // names do not work. Reject names we cannot override unambiguously.
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(
                "A Codex connector name cannot be safely disabled by this CLI version.".into(),
            );
        }
        let key = format!("mcp_servers.{name}.enabled");
        config.insert(key, Value::Bool(false));
    }
    for feature in [
        "apps",
        "hooks",
        "plugins",
        "remote_plugin",
        "browser_use",
        "computer_use",
        "in_app_browser",
        "multi_agent",
        "multi_agent_v2",
        "memories",
        "skill_mcp_dependency_install",
        "skill_search",
        "tool_suggest",
        "shell_snapshot",
        "workspace_dependencies",
        "realtime_conversation",
    ] {
        config.insert(format!("features.{feature}"), Value::Bool(false));
    }
    config.insert(
        "features.skip_host_skill_discovery".into(),
        Value::Bool(true),
    );
    config.insert("notify".into(), json!([]));
    config.insert("web_search".into(), json!("disabled"));
    config.insert("project_doc_max_bytes".into(), json!(0));
    config.insert("allow_login_shell".into(), Value::Bool(false));
    config.insert("shell_environment_policy.inherit".into(), json!("none"));
    neutralize_environment(&mut config, environment_names)?;
    config.insert(
        "shell_environment_policy.experimental_use_profile".into(),
        Value::Bool(false),
    );
    config.insert("approval_policy".into(), json!("on-request"));
    config.insert("approvals_reviewer".into(), json!("user"));
    // A fresh profile prevents deep-merging user-defined grants into ours.
    let profile = format!("live_coach_{}", uuid::Uuid::new_v4().simple());
    let canonical_project =
        std::fs::canonicalize(project).map_err(|_| "The selected project does not exist.")?;
    let project_text = canonical_project
        .to_str()
        .ok_or("Project path must be UTF-8.")?;
    let access = if workspace_write { "write" } else { "read" };
    config.insert(
        format!("permissions.{profile}"),
        json!({
            "filesystem": {":minimal":"read", project_text:access},
            "network":{"enabled":false}
        }),
    );
    config.insert("default_permissions".into(), json!(profile));
    config.insert(
        "sandbox_workspace_write.network_access".into(),
        Value::Bool(false),
    );
    config.insert("analytics.enabled".into(), Value::Bool(false));
    for exporter in ["exporter", "trace_exporter", "metrics_exporter"] {
        config.insert(format!("otel.{exporter}"), json!("none"));
    }
    for field in [
        "log_user_prompt",
        "log_agent_responses",
        "log_guardian_assessments",
    ] {
        config.insert(format!("otel.{field}"), Value::Bool(false));
    }
    config.insert("history.persistence".into(), json!("none"));
    // This adapter is intentionally OpenAI-only. Do not execute a configured
    // custom provider authentication command or silently change destination.
    config.insert("model_provider".into(), json!("openai"));
    // An explicit base overrides Codex's auth-aware default. ChatGPT tokens
    // must use the Codex backend; API-key accounts use the public API endpoint.
    config.insert("openai_base_url".into(), json!(provider_base));
    config.insert(
        "chatgpt_base_url".into(),
        json!("https://chatgpt.com/backend-api/"),
    );
    let mut args = Vec::new();
    for (key, value) in &config {
        args.push("-c".into());
        args.push(format!("{key}={}", toml_value(value)?));
    }
    Ok(Isolation {
        args,
        config,
        profile,
        provider_base: provider_base.into(),
    })
}

fn collect_names(value: &toml::Value, names: &mut BTreeSet<String>) {
    if let Some(table) = value.as_table() {
        if let Some(servers) = table.get("mcp_servers").and_then(toml::Value::as_table) {
            names.extend(servers.keys().cloned());
        }
        // Legacy profile tables can contribute inherited server entries.
        if let Some(profiles) = table.get("profiles").and_then(toml::Value::as_table) {
            for profile in profiles.values() {
                collect_names(profile, names);
            }
        }
    }
}
pub const CHATGPT_CODEX_BASE: &str = "https://chatgpt.com/backend-api/codex";
pub const OPENAI_API_BASE: &str = "https://api.openai.com/v1";
const SAFE_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

fn reject_command_provider_auth(value: &toml::Value) -> Result<(), String> {
    if value
        .get("model_providers")
        .and_then(|v| v.get("openai"))
        .and_then(|v| v.get("auth"))
        .is_some()
    {
        return Err("A custom OpenAI authentication command is configured. This integration requires Codex-managed authentication.".into());
    }
    if let Some(profiles) = value.get("profiles").and_then(toml::Value::as_table) {
        for profile in profiles.values() {
            reject_command_provider_auth(profile)?;
        }
    }
    Ok(())
}

fn verify_provider_telemetry(cfg: &Map<String, Value>) -> Result<(), String> {
    if cfg
        .get("model_providers")
        .and_then(|v| v.get("openai"))
        .and_then(|v| v.get("auth"))
        .is_some_and(|v| !v.is_null())
    {
        return Err("Codex OpenAI authentication is not isolated.".into());
    }
    let otel = cfg
        .get("otel")
        .and_then(Value::as_object)
        .ok_or("Codex telemetry configuration could not be verified.")?;
    if ["exporter", "trace_exporter", "metrics_exporter"]
        .iter()
        .any(|key| otel.get(*key).and_then(Value::as_str) != Some("none"))
        || [
            "log_user_prompt",
            "log_agent_responses",
            "log_guardian_assessments",
        ]
        .iter()
        .any(|key| otel.get(*key).and_then(Value::as_bool) != Some(false))
    {
        return Err("Codex telemetry export was not disabled.".into());
    }
    Ok(())
}

fn collect_environment_names(value: &toml::Value, names: &mut BTreeSet<String>) {
    if let Some(table) = value.as_table() {
        if let Some(set) = table
            .get("shell_environment_policy")
            .and_then(|v| v.get("set"))
            .and_then(toml::Value::as_table)
        {
            names.extend(set.keys().cloned());
        }
        if let Some(profiles) = table.get("profiles").and_then(toml::Value::as_table) {
            for profile in profiles.values() {
                collect_environment_names(profile, names);
            }
        }
    }
}

fn neutralize_environment(
    config: &mut Map<String, Value>,
    names: BTreeSet<String>,
) -> Result<(), String> {
    for name in names {
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err("A Codex command environment variable cannot be safely isolated by this CLI version.".into());
        }
        // Override names only: never copy their configured values into argv/state.
        config.insert(format!("shell_environment_policy.set.{name}"), json!(""));
    }
    config.insert("shell_environment_policy.set.PATH".into(), json!(SAFE_PATH));
    Ok(())
}

fn toml_key(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization")
}
fn toml_value(value: &Value) -> Result<String, String> {
    match value {
        Value::String(s) => Ok(toml_key(s)),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Array(v) if v.is_empty() => Ok("[]".into()),
        Value::Object(map) => {
            let pairs = map
                .iter()
                .map(|(k, v)| Ok(format!("{}={}", toml_key(k), toml_value(v)?)))
                .collect::<Result<Vec<_>, String>>()?;
            Ok(format!("{{{}}}", pairs.join(",")))
        }
        _ => Err("Unsupported Codex isolation setting.".into()),
    }
}

/// Check config/read's result before thread/start. Never return its raw config,
/// origins or layers: they may include provider credentials and private paths.
pub fn verify_config(response: &Value) -> Result<(), String> {
    let result = response.get("result").unwrap_or(response);
    let cfg = result
        .get("config")
        .and_then(Value::as_object)
        .ok_or("Codex did not return a verifiable configuration.")?;
    for key in ["apps", "hooks", "plugins", "remote_plugin"] {
        if cfg
            .get("features")
            .and_then(|v| v.get(key))
            .and_then(Value::as_bool)
            != Some(false)
        {
            return Err(format!(
                "Codex did not disable {key}; handoff is unavailable."
            ));
        }
    }
    let servers = cfg
        .get("mcp_servers")
        .and_then(Value::as_object)
        .ok_or("Codex connector configuration could not be verified.")?;
    if servers
        .values()
        .any(|server| server.get("enabled").and_then(Value::as_bool) != Some(false))
    {
        return Err("Codex has an enabled connector; handoff is unavailable.".into());
    }
    if cfg
        .get("notify")
        .and_then(Value::as_array)
        .is_none_or(|v| !v.is_empty())
    {
        return Err("Codex notification commands were not disabled.".into());
    }
    let environment = cfg
        .get("shell_environment_policy")
        .and_then(Value::as_object)
        .ok_or("Codex command environment policy could not be verified.")?;
    if environment.get("inherit").and_then(Value::as_str) != Some("none")
        || environment
            .get("experimental_use_profile")
            .and_then(Value::as_bool)
            != Some(false)
        || cfg.get("allow_login_shell").and_then(Value::as_bool) != Some(false)
    {
        return Err("Codex command environment inheritance was not disabled.".into());
    }
    // Deep-merged variables must be blank, except the fixed system PATH.
    let injected = environment
        .get("set")
        .and_then(Value::as_object)
        .ok_or("Codex command environment variables could not be verified.")?;
    if injected.get("PATH").and_then(Value::as_str) != Some(SAFE_PATH)
        || injected
            .iter()
            .any(|(key, value)| value.as_str() != Some(if key == "PATH" { SAFE_PATH } else { "" }))
    {
        return Err("Codex command environment variables were not isolated.".into());
    }
    if cfg.get("approval_policy").and_then(Value::as_str) != Some("on-request")
        || cfg.get("approvals_reviewer").and_then(Value::as_str) != Some("user")
    {
        return Err("Codex review and sandbox settings do not match the reviewed scope.".into());
    }
    let selected = cfg
        .get("default_permissions")
        .and_then(Value::as_str)
        .filter(|name| name.starts_with("live_coach_"))
        .ok_or("Codex did not select the restricted Live Coach permission profile.")?;
    let profile = cfg
        .get("permissions")
        .and_then(|p| p.get(selected))
        .ok_or("Codex permission profile is missing.")?;
    let fs: Map<String, Value> = profile
        .get("filesystem")
        .and_then(Value::as_object)
        .ok_or("Codex filesystem permissions are missing.")?
        .iter()
        .filter(|(k, v)| !(k.as_str() == "glob_scan_max_depth" && v.is_null()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if profile.get("extends").is_some_and(|v| !v.is_null())
        || profile.pointer("/network/enabled").and_then(Value::as_bool) != Some(false)
        || fs.len() != 2
        || fs.get(":minimal").and_then(Value::as_str) != Some("read")
        || fs
            .iter()
            .filter(|(k, _)| k.as_str() != ":minimal")
            .any(|(k, v)| {
                !Path::new(k).is_absolute() || !matches!(v.as_str(), Some("read" | "write"))
            })
    {
        return Err(
            "Codex permission profile grants more access than the reviewed project scope.".into(),
        );
    }
    verify_provider_telemetry(cfg)?;
    if cfg.get("model_provider").and_then(Value::as_str) != Some("openai") {
        return Err("This integration currently requires the OpenAI Codex provider.".into());
    }
    if !matches!(
        cfg.get("openai_base_url").and_then(Value::as_str),
        Some(CHATGPT_CODEX_BASE | OPENAI_API_BASE)
    ) || cfg.get("chatgpt_base_url").and_then(Value::as_str)
        != Some("https://chatgpt.com/backend-api/")
    {
        return Err("Codex provider destination could not be verified.".into());
    }
    if cfg
        .get("model_instructions_file")
        .is_some_and(|v| !v.is_null())
    {
        return Err("A custom Codex instruction file is configured. This integration cannot isolate it safely yet.".into());
    }
    if let Some(layers) = result.get("layers").and_then(Value::as_array) {
        for layer in layers {
            let kind = layer
                .pointer("/name/type")
                .and_then(Value::as_str)
                .unwrap_or("");
            if ![
                "sessionFlags",
                "user",
                "system",
                "project",
                "projectFile",
                "packagedDefaults",
            ]
            .contains(&kind)
            {
                return Err("An unsupported managed Codex configuration layer is active.".into());
            }
        }
    } else {
        return Err("Codex configuration layers were not supplied for verification.".into());
    }
    Ok(())
}

/// Match the exact generated profile, not merely a similarly named profile.
pub fn verify_config_for(response: &Value, isolation: &Isolation) -> Result<(), String> {
    verify_config(response)?;
    let cfg = response
        .get("result")
        .unwrap_or(response)
        .get("config")
        .ok_or("Missing Codex config")?;
    if cfg.get("openai_base_url").and_then(Value::as_str) != Some(isolation.provider_base.as_str())
    {
        return Err("Codex provider destination does not match the signed-in account type.".into());
    }
    if cfg.get("default_permissions").and_then(Value::as_str) != Some(&isolation.profile) {
        return Err("Codex selected a different permission profile.".into());
    }
    let actual = cfg
        .get("permissions")
        .and_then(|p| p.get(&isolation.profile))
        .ok_or("Missing Codex profile")?;
    let expected = &isolation.config[&format!("permissions.{}", isolation.profile)];
    let mut actual_fs = actual
        .get("filesystem")
        .and_then(Value::as_object)
        .cloned()
        .ok_or("Missing Codex filesystem profile")?;
    if actual_fs
        .get("glob_scan_max_depth")
        .is_some_and(Value::is_null)
    {
        actual_fs.remove("glob_scan_max_depth");
    }
    if Some(&Value::Object(actual_fs)) != expected.get("filesystem")
        || actual.pointer("/network/enabled") != Some(&Value::Bool(false))
    {
        return Err("Codex project permissions changed after review.".into());
    }
    Ok(())
}

pub fn verify_thread(response: &Value, isolation: &Isolation) -> Result<(), String> {
    let result = response.get("result").unwrap_or(response);
    if result
        .pointer("/activePermissionProfile/id")
        .and_then(Value::as_str)
        != Some(&isolation.profile)
        || result
            .pointer("/sandbox/networkAccess")
            .and_then(Value::as_bool)
            != Some(false)
        || result.get("approvalPolicy").and_then(Value::as_str) != Some("on-request")
        || result.get("approvalsReviewer").and_then(Value::as_str) != Some("user")
    {
        return Err("Codex did not acknowledge the reviewed project permissions.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_are_data_and_values_are_never_forwarded() {
        let v: toml::Value = "[mcp_servers.\"a.b\\\"x\"]\ncommand='secret-command'\n[profiles.work.mcp_servers.secondary]\nurl='https://secret.invalid'".parse().unwrap();
        let mut names = BTreeSet::new();
        collect_names(&v, &mut names);
        assert_eq!(names.len(), 2);
        let path = format!("mcp_servers.{}.enabled=false", toml_key("a.b\"x"));
        let parsed: toml::Value = path.parse().unwrap();
        assert_eq!(
            parsed["mcp_servers"]["a.b\"x"]["enabled"].as_bool(),
            Some(false)
        );
    }
    #[test]
    fn verification_rejects_an_enabled_server() {
        let cfg = json!({"features":{"apps":false,"hooks":false,"plugins":false,"remote_plugin":false},"mcp_servers":{"danger":{"enabled":true}}});
        assert!(verify_config(&json!({"config":cfg,"layers":[]}))
            .unwrap_err()
            .contains("connector"));
    }
    #[test]
    fn exact_profile_verification_normalizes_only_null_metadata() {
        let profile = "live_coach_test".to_string();
        let expected = json!({"filesystem":{":minimal":"read","/tmp/chosen":"write"},"network":{"enabled":false}});
        let mut config = Map::new();
        config.insert(format!("permissions.{profile}"), expected.clone());
        let isolation = Isolation {
            args: vec![],
            config,
            profile: profile.clone(),
            provider_base: CHATGPT_CODEX_BASE.into(),
        };
        let mut actual = expected;
        actual["filesystem"]["glob_scan_max_depth"] = Value::Null;
        let mut response = json!({"config":{
            "features":{"apps":false,"hooks":false,"plugins":false,"remote_plugin":false},
            "mcp_servers":{},"notify":[],"approval_policy":"on-request","approvals_reviewer":"user",
            "allow_login_shell":false,"shell_environment_policy":{"inherit":"none","experimental_use_profile":false,"set":{"PATH":"/usr/bin:/bin:/usr/sbin:/sbin"}},
            "otel":{"exporter":"none","trace_exporter":"none","metrics_exporter":"none","log_user_prompt":false,"log_agent_responses":false,"log_guardian_assessments":false},
            "default_permissions":profile,"permissions":{profile:actual},
            "model_provider":"openai","openai_base_url":"https://chatgpt.com/backend-api/codex","chatgpt_base_url":"https://chatgpt.com/backend-api/"
        },"layers":[{"name":{"type":"sessionFlags"}}]});
        assert!(verify_config_for(&response, &isolation).is_ok());
        response["config"]["openai_base_url"] = json!(OPENAI_API_BASE);
        assert!(verify_config_for(&response, &isolation)
            .unwrap_err()
            .contains("account type"));
        let mut api_isolation = isolation.clone();
        api_isolation.provider_base = OPENAI_API_BASE.into();
        assert!(verify_config_for(&response, &api_isolation).is_ok());
        response["config"]["openai_base_url"] = json!(CHATGPT_CODEX_BASE);
        response["config"]["shell_environment_policy"]["set"] =
            json!({"PRIVATE_SECRET":"fixture-only"});
        assert!(verify_config_for(&response, &isolation)
            .unwrap_err()
            .contains("environment variables"));
        response["config"]["shell_environment_policy"]["set"] =
            json!({"PATH":SAFE_PATH,"PRIVATE_SECRET":""});
        assert!(verify_config_for(&response, &isolation).is_ok());
        response["config"]["permissions"]["live_coach_test"]["filesystem"][":root"] = json!("read");
        assert!(verify_config_for(&response, &isolation).is_err());
    }
    #[test]
    fn configured_environment_values_are_never_forwarded() {
        let source: toml::Value="[shell_environment_policy.set]\nPRIVATE_SECRET='fixture-secret-value'\nPATH='/bad/path'\n[profiles.work.shell_environment_policy.set]\nANOTHER_SECRET='other-private-value'".parse().unwrap();
        let mut names = BTreeSet::new();
        collect_environment_names(&source, &mut names);
        let mut overrides = Map::new();
        neutralize_environment(&mut overrides, names).unwrap();
        assert_eq!(
            overrides["shell_environment_policy.set.PRIVATE_SECRET"],
            json!("")
        );
        assert_eq!(
            overrides["shell_environment_policy.set.ANOTHER_SECRET"],
            json!("")
        );
        assert_eq!(
            overrides["shell_environment_policy.set.PATH"],
            json!(SAFE_PATH)
        );
        let encoded = serde_json::to_string(&overrides).unwrap();
        assert!(
            !encoded.contains("fixture-secret-value")
                && !encoded.contains("other-private-value")
                && !encoded.contains("/bad/path")
        );
        assert!(neutralize_environment(&mut overrides, BTreeSet::from(["a.b".into()])).is_err());
    }
    #[test]
    fn telemetry_and_custom_auth_fail_closed() {
        let raw: toml::Value = "[model_providers.openai.auth]\ncommand='fixture-auth-command'"
            .parse()
            .unwrap();
        assert!(reject_command_provider_auth(&raw).is_err());
        let mut cfg=json!({"otel":{"exporter":"none","trace_exporter":"none","metrics_exporter":"none","log_user_prompt":false,"log_agent_responses":false,"log_guardian_assessments":false}}).as_object().unwrap().clone();
        assert!(verify_provider_telemetry(&cfg).is_ok());
        cfg["otel"]["trace_exporter"] =
            json!({"otlp-http":{"endpoint":"https://fixture.invalid","protocol":"binary"}});
        assert!(verify_provider_telemetry(&cfg).is_err());
        cfg["otel"]["trace_exporter"] = json!("none");
        cfg.insert(
            "model_providers".into(),
            json!({"openai":{"auth":{"command":"fixture-auth-command"}}}),
        );
        assert!(verify_provider_telemetry(&cfg).is_err());
    }
    #[test]
    fn thread_must_acknowledge_exact_profile() {
        let isolation = Isolation {
            args: vec![],
            config: Map::new(),
            profile: "live_coach_chosen".into(),
            provider_base: CHATGPT_CODEX_BASE.into(),
        };
        let response = json!({"activePermissionProfile":{"id":"different"},"sandbox":{"networkAccess":false},"approvalPolicy":"on-request","approvalsReviewer":"user"});
        assert!(verify_thread(&response, &isolation).is_err());
    }
    #[test]
    fn invalid_paths_fail_before_any_process() {
        assert!(build(Path::new("relative"), Path::new("/tmp")).is_err());
    }
}
