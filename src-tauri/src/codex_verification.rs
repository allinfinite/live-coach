//! Explicit opt-in smoke test against the user's signed-in Codex, using only synthetic context.
use crate::{
    codex::{ApprovalDecision, CodexClient, CodexEvent, ConnectOptions},
    codex_bridge::{Bridge, Event},
    codex_config,
    project_context::Mode,
};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
pub fn run_if_requested() -> bool {
    if std::env::args().nth(1).as_deref() != Some("--codex-self-test") {
        return false;
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let result = runtime.block_on(run());
    match result {
        Ok(()) => println!("CODEX_SELF_TEST_PASS"),
        Err(e) => {
            eprintln!("CODEX_SELF_TEST_FAIL: {e}");
            std::process::exit(1);
        }
    }
    true
}
async fn wait(
    bridge: &Arc<Bridge>,
    events: &Arc<Mutex<Vec<Event>>>,
    id: &str,
) -> Result<String, String> {
    let started = Instant::now();
    let mut cursor = 0;
    let mut output = String::new();
    loop {
        let batch = {
            let all = events.lock().unwrap();
            let b = all[cursor..].to_vec();
            cursor = all.len();
            b
        };
        for event in batch {
            if event.run_id != id {
                continue;
            }
            match event.kind.as_str() {
                "output" => output.push_str(event.text.as_deref().unwrap_or("")),
                "approval" => {
                    // The smoke test never expands permissions or blindly accepts a proposal.
                    if let Some(approval) = event.approval_id {
                        bridge
                            .approval(id, &approval, ApprovalDecision::Decline)
                            .await?;
                        println!("SMOKE_DECLINED_PERMISSION_REQUEST");
                    }
                }
                "completed" => return Ok(output),
                "error" => return Err(event.text.unwrap_or_default()),
                _ => {}
            }
        }
        if started.elapsed() > Duration::from_secs(120) {
            let _ = bridge.cancel(id).await;
            return Err("Actual Codex turn timed out".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
/// Persist one explicitly owned synthetic conversation via the official app-server
/// protocol. No existing conversation is resumed, renamed, archived, or edited.
async fn synthetic_history_fixture(
    project: &Path,
    model: &str,
) -> Result<(Arc<CodexClient>, String), String> {
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".codex")))
        .ok_or("Codex home unavailable")?
        .canonicalize()
        .map_err(|_| "Codex home unavailable")?;
    let events = Arc::new(Mutex::new(Vec::new()));
    let output = events.clone();
    let client = CodexClient::new(Arc::new(move |event| output.lock().unwrap().push(event)));
    // Metadata-only discovery of the current authentication type. Credentials stay
    // in the existing Codex-managed home; there is no copied auth or separate login.
    let mut isolation = codex_config::build(&home, project)?;
    client
        .connect(ConnectOptions {
            executable: None,
            home: home.clone(),
            cwd: project.to_owned(),
            extra_args: isolation.args.clone(),
        })
        .await?;
    let result = async {
        let account = client.rpc("account/read", json!({"refreshToken":false})).await?;
        let chatgpt = match account.pointer("/account/type").and_then(serde_json::Value::as_str) {
            Some("chatgpt") => true,
            Some("apiKey") => false,
            _ => return Err("Unsupported fixture authentication".into()),
        };
        if !chatgpt {
            isolation = codex_config::build_for_auth_scope(&home, project, false, false)?;
            client.shutdown().await;
            client.connect(ConnectOptions { executable: None, home: home.clone(), cwd: project.to_owned(), extra_args: isolation.args.clone() }).await?;
        }
        let config = client.rpc("config/read", json!({"includeLayers":true,"cwd":project})).await?;
        codex_config::verify_config_for(&config, &isolation)?;
        let response = client.rpc("thread/start", json!({"cwd":project,"model":model,"modelProvider":"openai","approvalPolicy":"on-request","approvalsReviewer":"user","ephemeral":false,"config":isolation.config,"developerInstructions":"This is an explicitly authorized synthetic Live Coach history test. Use no tools. Reply only: Recorded the synthetic history fact."})).await?;
        codex_config::verify_thread(&response, &isolation)?;
        let thread = response.pointer("/thread/id").and_then(serde_json::Value::as_str).ok_or("Missing synthetic thread ID")?.to_owned();
        // Archive cleanup is performed even when the synthetic turn fails.
        let turn_result = async {
            client.rpc("turn/start", json!({"threadId":thread,"cwd":project,"model":model,"approvalPolicy":"on-request","approvalsReviewer":"user","input":[{"type":"text","text":"Synthetic history fixture only. The conversation-only secret project nickname is cobalt-heron-731. It is not in any project file. Remember this fact for a later review test. Do not run tools or change files."}]})).await?;
            let start = Instant::now();
            let mut cursor = 0;
            loop {
                let batch = { let all = events.lock().unwrap(); let batch = all[cursor..].to_vec(); cursor = all.len(); batch };
                for event in batch {
                    match event {
                        CodexEvent::Approval { id, .. } => { client.reply_to_approval(id, ApprovalDecision::Decline).await?; },
                        CodexEvent::Notification { method, params } if method == "turn/completed" && params.get("threadId").and_then(serde_json::Value::as_str) == Some(thread.as_str()) => {
                            if params.pointer("/turn/status").and_then(serde_json::Value::as_str) != Some("completed") { return Err("Synthetic fixture turn did not complete".into()); }
                            return Ok(());
                        },
                        _ => {},
                    }
                }
                if start.elapsed() > Duration::from_secs(120) { return Err("Synthetic history fixture timed out".into()); }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }.await;
        if let Err(error) = turn_result {
            let _ = client.rpc("thread/archive", json!({"threadId":thread})).await;
            return Err(error);
        }
        Ok(thread)
    }.await;
    match result {
        Ok(thread) => Ok((client, thread)),
        Err(error) => {
            client.shutdown().await;
            Err(error)
        }
    }
}

async fn verify_history(
    bridge: &Arc<Bridge>,
    events: &Arc<Mutex<Vec<Event>>>,
    project: &Path,
    model: &str,
) -> Result<(), String> {
    // Real metadata discovery, with no titles, paths, or user content in logs.
    let page = bridge.history_list(None, None, None).await?;
    println!(
        "REAL_HISTORY_LIST_PASS metadata_count={}",
        page["threads"].as_array().map_or(0, Vec::len)
    );
    let (fixture_client, fixture_id) = synthetic_history_fixture(project, model).await?;
    let result = async {
        // Filter at the server to our owned disposable project, then hydrate only
        // its known synthetic thread. Never read existing private conversation text.
        let mut visible = false;
        for _ in 0..20 {
            let page = bridge.history_list(None, None, Some(project.display().to_string())).await?;
            if page["threads"].as_array().is_some_and(|threads| threads.iter().any(|thread| thread["id"].as_str() == Some(fixture_id.as_str()))) { visible = true; break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if !visible { return Err("Synthetic persisted history was not listed".into()); }
        let snapshot = bridge.history_read(fixture_id.clone()).await?;
        let messages = snapshot["messages"].as_array().ok_or("Missing history messages")?;
        let selected = messages.iter().find(|message| message["role"] == "user" && message["text"].as_str().is_some_and(|text| text.contains("cobalt-heron-731"))).ok_or("Synthetic history message was not available")?;
        let before = fixture_client.rpc("thread/items/list", json!({"threadId":fixture_id,"limit":16,"sortDirection":"asc"})).await?;
        println!("REAL_HISTORY_READ_PASS synthetic_messages={}", messages.len());
        let review = bridge.prepare_with_history(project.display().to_string(), vec![], "What is the conversation-only secret project nickname in the selected history? Reply with the nickname only. Use no tools; do not read or change files.".into(), model.to_owned(), Mode::Plan, Some(snapshot["snapshot_id"].as_str().ok_or("Missing snapshot ID")?.to_owned()), vec![selected["id"].as_str().ok_or("Missing history message ID")?.to_owned()])?;
        if review.history.len() != 1 || !review.prompt.contains("cobalt-heron-731") { return Err("Reviewed history was missing from the immutable prompt".into()); }
        let run = bridge.send(&review.review_id).await?;
        let output = wait(bridge, events, run["run_id"].as_str().ok_or("Missing history run ID")?).await?;
        if !output.contains("cobalt-heron-731") { return Err("Selected synthetic history did not affect the streamed answer".into()); }
        let after = fixture_client.rpc("thread/items/list", json!({"threadId":fixture_id,"limit":16,"sortDirection":"asc"})).await?;
        if before["data"] != after["data"] { return Err("Source history changed during the fresh handoff".into()); }
        println!("REAL_HISTORY_REVIEWED_STREAM_PASS source_unchanged=true");
        if std::fs::read_dir(project).map_err(|_| "Cannot verify fixture project")?.count() != 1 { return Err("History Plan changed project files".into()); }
        Ok(())
    }.await;
    // This ID came directly from our own thread/start above, never from user history.
    let archived = fixture_client
        .rpc("thread/archive", json!({"threadId":fixture_id}))
        .await;
    fixture_client.shutdown().await;
    if archived.is_err() {
        return Err("Could not archive owned synthetic history fixture".into());
    }
    println!("SYNTHETIC_HISTORY_ARCHIVED");
    result
}

async fn run() -> Result<(), String> {
    let parent =
        std::env::temp_dir().join(format!("live-coach-codex-smoke-{}", uuid::Uuid::new_v4()));
    let project = parent.join("project");
    std::fs::create_dir_all(&project).map_err(|e| e.to_string())?;
    let project = project.canonicalize().map_err(|e| e.to_string())?;
    std::fs::write(
        project.join("notes.md"),
        "The project color is indigo. The launch day is Tuesday.\n",
    )
    .map_err(|e| e.to_string())?;
    let events = Arc::new(Mutex::new(Vec::new()));
    let out = events.clone();
    let bridge = Bridge::new(Arc::new(move |e| out.lock().unwrap().push(e)));
    let result=async{
  if bridge.status().connected||bridge.send("unreviewed").await.is_ok(){return Err("Unreviewed task was accepted".into());}
  let connected=bridge.connect().await?;let models=connected["models"].as_array().ok_or("No Codex model catalog")?;
  let model=models.iter().find(|m|m["is_default"]==true).or_else(||models.first()).and_then(|m|m["id"].as_str()).ok_or("No available Codex models")?.to_owned();println!("CODEX_CONNECTED catalog_count={}",models.len());
  let review=bridge.prepare(project.display().to_string(),vec!["notes.md".into()],"State the two project facts in one short sentence from the selected reference; do not run commands or change files.".into(),model.clone(),Mode::Plan)?;
  println!("REVIEWED_PLAN selected_files={} mode=plan",review.files.len());
  let run=bridge.send(&review.review_id).await?;let id=run["run_id"].as_str().ok_or("Missing run id")?;
  if bridge.send(&review.review_id).await.is_ok(){return Err("Review token replay was accepted".into());}
  let output=wait(&bridge,&events,id).await?;if !output.to_lowercase().contains("indigo")||!output.to_lowercase().contains("tuesday"){return Err(format!("Context did not reach Codex: {output}"));}
  if std::fs::read_dir(&project).map_err(|e|e.to_string())?.count()!=1{return Err("Plan changed project files".into());}println!("REAL_PLAN_CONTEXT_PASS output={output}");
  verify_history(&bridge, &events, &project, &model).await?;
  let review=bridge.prepare(project.display().to_string(),vec![],"Create result.txt in the chosen project containing exactly REVIEWED_TASK_OK followed by a newline. Use a file change tool. Do not read unrelated files, access the network, or delegate.".into(),model.clone(),Mode::Task)?;
  println!("REVIEWED_TASK target=result.txt mode=task");let run=bridge.send(&review.review_id).await?;wait(&bridge,&events,run["run_id"].as_str().unwrap()).await?;
  if std::fs::read_to_string(project.join("result.txt")).map_err(|_|"Task did not create the requested file")?!="REVIEWED_TASK_OK\n"{return Err("Task file did not match the reviewed task".into());}println!("REAL_PROJECT_WRITE_PASS");
  let review=bridge.prepare(project.display().to_string(),vec![],"Reply with canceled-test only, without tools.".into(),model,Mode::Plan)?;let run=bridge.send(&review.review_id).await?;bridge.cancel(run["run_id"].as_str().unwrap()).await?;if bridge.status().connected{return Err("Cancel left Codex connected".into());}println!("REAL_CANCEL_STARTUP_PASS");Ok(())
 }.await;
    bridge.disconnect().await;
    // Delete only our disposable directory; user projects and application data are not involved.
    if parent.starts_with(std::env::temp_dir())
        && parent
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("live-coach-codex-smoke-"))
    {
        let _ = std::fs::remove_dir_all(&parent);
    }
    result
}
