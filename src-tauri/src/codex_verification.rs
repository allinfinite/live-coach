//! Explicit opt-in smoke test against the user's signed-in Codex, using only synthetic context.
use crate::{
    codex::ApprovalDecision,
    codex_bridge::{Bridge, Event},
    project_context::Mode,
};
use std::{
    path::Path,
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
    let _ = Path::new("synthetic smoke only");
    result
}
