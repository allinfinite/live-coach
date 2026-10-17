#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod capture;
mod coach;
mod models;
mod storage;
mod upstream;
mod verification;
mod voice;
mod warmup;
use coach::{Action, Coach, CoachEvent, CoachSettings, Provider, Segment};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

// ort's bundled native runtime is linked at build time, with no runtime download.
pub fn ensure_onnx_runtime_available() -> Result<(), String> {
    Ok(())
}
#[derive(Clone, Deserialize)]
#[serde(default)]
struct Settings {
    #[serde(flatten)]
    coach: CoachSettings,
    retain: bool,
    retention_days: u32,
    timer_minutes: u32,
    stt_path: String,
    stt_engine: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            coach: CoachSettings::default(),
            retain: false,
            retention_days: 7,
            timer_minutes: 10,
            stt_path: String::new(),
            stt_engine: "parakeet".into(),
        }
    }
}
#[derive(Default)]
struct Session {
    capture: Option<capture::CaptureHandle>,
    simulation: Option<tokio::task::JoinHandle<()>>,
    settings: Option<Settings>,
}
struct AppState {
    coach: Arc<Coach>,
    warmup: Arc<warmup::Warmup>,
    voice: Arc<voice::Voice>,
    speaking: std::sync::atomic::AtomicBool,
    latest_suggestion: Arc<std::sync::Mutex<Option<String>>>,
    session: Mutex<Session>,
    operation: Mutex<()>,
    retained: Arc<std::sync::Mutex<Option<Vec<Segment>>>>,
}
const KEY_SERVICE: &str = "com.dnalevity.livecoach";
fn cloud_key() -> Option<String> {
    security_framework::passwords::get_generic_password(KEY_SERVICE, "api-key")
        .ok()
        .and_then(|v| String::from_utf8(v).ok())
}
#[tauri::command]
async fn environment() -> serde_json::Value {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let tags = match client.get("http://127.0.0.1:11434/api/tags").send().await {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };
    let models = tags
        .as_ref()
        .and_then(|v| v["models"].as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v["name"].as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    serde_json::json!({"models":models::discover(),"ollama":{"available":tags.is_some(),"models":models,"installed":std::path::Path::new("/Applications/Ollama.app").exists() || std::path::Path::new("/usr/local/bin/ollama").exists() || std::path::Path::new("/opt/homebrew/bin/ollama").exists()},"key_saved":cloud_key().is_some(),"message":"Speech stays on this Mac. Source labels identify audio channels, not people."})
}
// Startup model preparation is independent of consent, sessions and audio devices.
#[tauri::command]
async fn warm_ollama(
    state: tauri::State<'_, Arc<AppState>>,
    provider: Provider,
    model: String,
) -> Result<(), String> {
    if provider != Provider::Ollama {
        return Err("Automatic warm-up is available only for local Ollama.".into());
    }
    state.warmup.warm(model).await
}
#[tauri::command]
async fn cancel_warmup(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    state.warmup.cancel().await;
    Ok(())
}
#[tauri::command]
async fn voice_status(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<voice::VoiceStatus, String> {
    let mut status = state.voice.status().await;
    status.playing |= state.speaking.load(std::sync::atomic::Ordering::SeqCst);
    Ok(status)
}
#[tauri::command]
async fn start_voice_reference(
    state: tauri::State<'_, Arc<AppState>>,
    consent: bool,
) -> Result<(), String> {
    let _op = state.operation.lock().await;
    if state.coach.is_active().await || state.speaking.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(
            "Stop the conversation and voice playback before recording your voice sample.".into(),
        );
    }
    state.voice.start_reference(consent).await
}
#[tauri::command]
async fn finish_voice_reference(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let _op = state.operation.lock().await;
    state.voice.finish_reference().await
}
#[tauri::command]
async fn cancel_voice_reference(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let _op = state.operation.lock().await;
    state.voice.cancel_reference().await;
    Ok(())
}
#[tauri::command]
async fn forget_voice(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let _op = state.operation.lock().await;
    state.voice.forget().await
}
#[tauri::command]
async fn stop_voice(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let _op = state.operation.lock().await;
    state.voice.stop().await;
    Ok(())
}
async fn play_voice(state: &AppState, app: &tauri::AppHandle, text: String) -> Result<(), String> {
    let op = state.operation.lock().await;
    if state.voice.status().await.recording {
        return Err("Finish or cancel the voice recording first.".into());
    }
    if state
        .speaking
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return Err("A voice suggestion is already being read.".into());
    }
    let playback_ticket = state.voice.reserve_playback();
    if let Some(capture) = state.session.lock().await.capture.as_ref() {
        capture.set_paused(true);
    }
    drop(op);
    let result = match app.path().resource_dir() {
        Ok(dir) => {
            state
                .voice
                .speak_reserved(playback_ticket, text, dir.join("voice-runtime/engine.py"))
                .await
        }
        Err(e) => Err(e.to_string()),
    };
    tokio::time::sleep(Duration::from_millis(500)).await;
    let _op = state.operation.lock().await;
    if let Some(capture) = state.session.lock().await.capture.as_ref() {
        capture.set_paused(false);
    }
    state
        .speaking
        .store(false, std::sync::atomic::Ordering::SeqCst);
    result
}
#[tauri::command]
async fn speak_suggestion(
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let text = state
        .latest_suggestion
        .lock()
        .unwrap()
        .clone()
        .ok_or("Wait for a coaching suggestion before reading it aloud.")?;
    play_voice(&state, &app, text).await
}
#[tauri::command]
async fn preview_voice(
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    play_voice(
        &state,
        &app,
        "Let me take a moment to think about what matters to me.".into(),
    )
    .await
}
#[tauri::command]
async fn start_session(
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    consent: bool,
    simulation: bool,
    settings: Settings,
) -> Result<(), String> {
    let _op = state.operation.lock().await;
    if state.voice.status().await.recording
        || state.speaking.load(std::sync::atomic::Ordering::SeqCst)
    {
        return Err("Finish voice recording or playback before starting the conversation.".into());
    }
    if !consent {
        return Err("Inform participants as appropriate and acknowledge before starting.".into());
    }
    if state.coach.is_active().await {
        return Err("A conversation is already active.".into());
    }
    if settings.coach.provider == Provider::OpenaiCompatible && !settings.coach.cloud_consent {
        return Err("Transcript text will leave this Mac. Explicit external-processing consent is required.".into());
    }
    if !simulation {
        models::validate(&settings.stt_path, &settings.stt_engine)?;
    }
    state.coach.update_settings(settings.coach.clone()).await?;
    state.coach.set_cloud_key(cloud_key()).await;
    let token = state.coach.start(consent).await?;
    let mut session = state.session.lock().await;
    session.settings = Some(settings.clone());
    *state.retained.lock().unwrap() = if settings.retain { Some(vec![]) } else { None };
    if simulation {
        let coach = state.coach.clone();
        session.simulation = Some(tokio::spawn(async move {
            for (index, line) in include_str!("../../tests/fixtures/conversation.txt")
                .lines()
                .filter(|s| !s.is_empty())
                .enumerate()
            {
                tokio::time::sleep(Duration::from_millis(2000)).await;
                let speaker = if index % 2 == 0 {
                    "Practice · A"
                } else {
                    "Practice · B"
                };
                let _ = coach
                    .ingest_for_session(
                        token,
                        Segment {
                            id: format!("sim-{token}-{index}"),
                            text: line.into(),
                            speaker: Some(speaker.into()),
                            timestamp_ms: index as u64 * 2000,
                        },
                    )
                    .await;
            }
        }));
    } else {
        let coach = state.coach.clone();
        let started = std::time::Instant::now();
        let callback: capture::TranscriptCallback = Arc::new(move |text, source| {
            let coach = coach.clone();
            let timestamp_ms = started.elapsed().as_millis() as u64;
            tauri::async_runtime::spawn(async move {
                let _ = coach
                    .ingest_for_session(
                        token,
                        Segment {
                            id: uuid::Uuid::new_v4().to_string(),
                            text,
                            speaker: Some(source),
                            timestamp_ms,
                        },
                    )
                    .await;
            });
        });
        let status_app = app.clone();
        let status: capture::StatusCallback = Arc::new(move |message| {
            let _ = status_app.emit("coach-event", CoachEvent::Status { message });
        });
        match capture::start(
            settings.stt_path.into(),
            settings.stt_engine,
            callback,
            status,
        )
        .await
        {
            Ok(handle) => {
                session.capture = Some(handle);
            }
            Err(error) => {
                *state.retained.lock().unwrap() = None;
                session.settings = None;
                state.coach.stop().await;
                state.coach.clear().await;
                return Err(error);
            }
        }
    }
    Ok(())
}
async fn stop(state: &AppState) -> Result<(), String> {
    let _op = state.operation.lock().await;
    stop_locked(state).await
}
async fn stop_locked(state: &AppState) -> Result<(), String> {
    state.voice.stop().await;
    state.voice.cancel_reference().await;
    state.coach.stop().await;
    let mut session = state.session.lock().await;
    if let Some(task) = session.simulation.take() {
        task.abort();
        let _ = task.await;
    }
    if let Some(capture) = session.capture.take() {
        capture.stop().await;
    }
    let settings = session.settings.take();
    let retained = state.retained.lock().unwrap().take().unwrap_or_default();
    let result = if let Some(settings) = settings {
        storage::save_if_enabled(&retained, settings.retain, settings.retention_days)
    } else {
        Ok(())
    };
    state.coach.clear().await;
    state.coach.set_cloud_key(None).await;
    result
}
#[tauri::command]
async fn stop_session(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    stop(&state).await
}
#[tauri::command]
async fn quick_action(
    state: tauri::State<'_, Arc<AppState>>,
    action: Action,
) -> Result<(), String> {
    state.coach.quick_action(action).await
}
#[tauri::command]
async fn delete_transcripts(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let _op = state.operation.lock().await;
    if let Some(settings) = state.session.lock().await.settings.as_mut() {
        settings.retain = false;
    }
    *state.retained.lock().unwrap() = None;
    stop_locked(&state).await?;
    storage::purge()
}
#[tauri::command]
async fn save_api_key(state: tauri::State<'_, Arc<AppState>>, key: String) -> Result<(), String> {
    if state.coach.is_active().await {
        return Err("Stop the conversation before changing API credentials".into());
    }
    state.coach.set_cloud_key(None).await;
    if key.is_empty() {
        return security_framework::passwords::delete_generic_password(KEY_SERVICE, "api-key")
            .or_else(|e| if e.code() == -25300 { Ok(()) } else { Err(e) })
            .map_err(|e| e.to_string());
    }
    if key.len() > 8192 {
        return Err("API key is too long".into());
    }
    security_framework::passwords::set_generic_password(KEY_SERVICE, "api-key", key.as_bytes())
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn load_history() -> Result<Vec<serde_json::Value>, String> {
    storage::load()
}
fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    if verification::run_if_requested() {
        return;
    }
    let application = tauri::Builder::default()
 .setup(|app|{
  storage::expire().map_err(std::io::Error::other)?;
  let handle=app.handle().clone();
  let retained:Arc<std::sync::Mutex<Option<Vec<Segment>>>>=Arc::new(std::sync::Mutex::new(None));let sink_retained=retained.clone();
  let latest_suggestion=Arc::new(std::sync::Mutex::new(None));let sink_suggestion=latest_suggestion.clone();
  let sink:coach::EventSink=Arc::new(move|event|{
   match &event {CoachEvent::Insight{suggested_response,..}=>{*sink_suggestion.lock().unwrap()=Some(suggested_response.clone());},CoachEvent::Stopped=>{*sink_suggestion.lock().unwrap()=None;},_=>{}}
   if let CoachEvent::Transcript{id,text,speaker,timestamp_ms}=&event {if let Some(segments)=sink_retained.lock().unwrap().as_mut(){if segments.len()>=10000{segments.remove(0);}segments.push(Segment{id:id.clone(),text:text.clone(),speaker:speaker.clone(),timestamp_ms:*timestamp_ms});}}
   let _=handle.emit("coach-event",event);
  });
  let state=Arc::new(AppState{coach:Coach::new(CoachSettings::default(),sink),warmup:warmup::Warmup::new(),voice:voice::Voice::new(),speaking:std::sync::atomic::AtomicBool::new(false),latest_suggestion,session:Mutex::new(Session::default()),operation:Mutex::new(()),retained});
  app.manage(state.clone());
  let app_handle=app.handle().clone();
  tauri::async_runtime::spawn(async move {
   loop {tokio::time::sleep(Duration::from_secs(1)).await;
    let _op=state.operation.lock().await;
    let finished=state.session.lock().await.capture.as_ref().is_some_and(|c|c.is_finished());
    if finished{let _=stop_locked(&state).await;let _=app_handle.emit("coach-event",CoachEvent::Error{message:"Audio capture ended. Review microphone and system-audio permissions before restarting.".into()});}
    let _=storage::expire();
   }
  });
  Ok(())
 })
 .invoke_handler(tauri::generate_handler![voice_status,start_voice_reference,finish_voice_reference,cancel_voice_reference,forget_voice,speak_suggestion,preview_voice,stop_voice,environment,warm_ollama,cancel_warmup,start_session,stop_session,quick_action,delete_transcripts,save_api_key,load_history])
 .on_window_event(|window,event|{if let tauri::WindowEvent::CloseRequested{api,..}=event{api.prevent_close();let state=window.state::<Arc<AppState>>().inner().clone();let handle=window.clone();tauri::async_runtime::spawn(async move{let _=stop(&state).await;let _=handle.destroy();handle.app_handle().exit(0);});}})
 .build(tauri::generate_context!()).expect("Live Coach failed to launch");
    let quitting = Arc::new(std::sync::atomic::AtomicBool::new(false));
    application.run(move |app, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if !quitting.swap(true, std::sync::atomic::Ordering::SeqCst) {
                api.prevent_exit();
                let state = app.state::<Arc<AppState>>().inner().clone();
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    state.voice.shutdown().await;
                    state.warmup.cancel().await;
                    let _ = stop(&state).await;
                    handle.exit(0);
                });
            }
        }
    });
}
