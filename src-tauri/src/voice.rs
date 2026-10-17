//! Explicit, microphone-only voice enrollment and private local coaching playback.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{watch, Mutex as AsyncMutex},
};

pub const REFERENCE_TEXT: &str = "I am recording my own voice for private coaching. I can pause, think clearly, and choose words that reflect what matters to me. I want to listen carefully and keep my boundaries.";
const MAX_SECONDS: u64 = 30;
#[derive(Debug, Serialize)]
pub struct VoiceStatus {
    pub runtime_ready: bool,
    pub has_voice: bool,
    pub recording: bool,
    pub pending_sample: bool,
    pub playing: bool,
    pub seconds: f64,
    pub elapsed_seconds: f64,
    pub reference_text: &'static str,
}
#[derive(Serialize, Deserialize)]
struct Profile {
    version: u32,
    consent: bool,
    created_at: u64,
    reference_text: String,
}
struct Samples {
    values: Vec<f32>,
    rate: u32,
}
struct Enrollment {
    stop: Arc<AtomicBool>,
    started: Instant,
    handle: thread::JoinHandle<Result<Samples, String>>,
}
struct Engine {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}
pub struct Voice {
    base: PathBuf,
    enrollment: AsyncMutex<Option<Enrollment>>,
    engine: AsyncMutex<Option<Engine>>,
    generation: watch::Sender<u64>,
    playing: AtomicBool,
}
impl Voice {
    pub fn new() -> Arc<Self> {
        let base = dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("com.dnalevity.livecoach");
        Self::at(base)
    }
    fn at(base: PathBuf) -> Arc<Self> {
        cleanup_generated(&base.join("voice-output"));
        let (generation, _) = watch::channel(0);
        Arc::new(Self {
            base,
            enrollment: AsyncMutex::new(None),
            engine: AsyncMutex::new(None),
            generation,
            playing: AtomicBool::new(false),
        })
    }
    fn reference(&self) -> PathBuf {
        self.base.join("voice-profiles/reference.wav")
    }
    fn python(&self) -> PathBuf {
        self.base.join("voice-runtime/.venv/bin/python")
    }
    fn ready(&self) -> bool {
        self.python().is_file()
            && [
                "config.json",
                "model.safetensors",
                "speech_tokenizer/model.safetensors",
            ]
            .iter()
            .all(|p| self.base.join("voice-model").join(p).is_file())
    }
    fn has_voice(&self) -> bool {
        let profile = fs::read(self.base.join("voice-profiles/profile.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Profile>(&b).ok());
        hound::WavReader::open(self.reference())
            .ok()
            .map(|reader| {
                let spec = reader.spec();
                spec.channels == 1
                    && spec.sample_rate > 0
                    && reader.duration() as u64 >= spec.sample_rate as u64 * 5
                    && reader.duration() as u64 <= spec.sample_rate as u64 * MAX_SECONDS
            })
            .unwrap_or(false)
            && profile
                .map(|p| p.version == 1 && p.consent && p.reference_text == REFERENCE_TEXT)
                .unwrap_or(false)
    }
    pub async fn status(&self) -> VoiceStatus {
        let enrollment = self.enrollment.lock().await;
        VoiceStatus {
            runtime_ready: self.ready(),
            has_voice: self.has_voice(),
            recording: enrollment.is_some(),
            pending_sample: enrollment.is_some(),
            playing: self.playing.load(Ordering::SeqCst),
            seconds: enrollment
                .as_ref()
                .map(|e| e.started.elapsed().as_secs_f64().min(MAX_SECONDS as f64))
                .unwrap_or(0.),
            elapsed_seconds: enrollment
                .as_ref()
                .map(|e| e.started.elapsed().as_secs_f64().min(MAX_SECONDS as f64))
                .unwrap_or(0.),
            reference_text: REFERENCE_TEXT,
        }
    }
    pub async fn start_reference(&self, consent: bool) -> Result<(), String> {
        require_consent(consent)?;
        if self.playing.load(Ordering::SeqCst) {
            return Err("Stop voice playback before recording your voice".into());
        }
        let mut enrollment = self.enrollment.lock().await;
        if enrollment.is_some() {
            return Err("Finish or cancel the existing voice recording first".into());
        }
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let handle = thread::spawn(move || record_reference(thread_stop, ready_tx));
        let ready =
            tokio::task::spawn_blocking(move || ready_rx.recv_timeout(Duration::from_secs(10)))
                .await
                .map_err(|e| e.to_string())?;
        match ready {
            Ok(Ok(())) => {
                *enrollment = Some(Enrollment {
                    stop,
                    started: Instant::now(),
                    handle,
                });
                Ok(())
            }
            result => {
                stop.store(true, Ordering::SeqCst);
                let _ = tokio::task::spawn_blocking(move || handle.join()).await;
                Err(match result {
                    Ok(Err(e)) => e,
                    Err(e) => format!("Microphone did not initialize: {e}"),
                    _ => "Microphone did not initialize".into(),
                })
            }
        }
    }
    pub async fn finish_reference(&self) -> Result<(), String> {
        let enrollment = self
            .enrollment
            .lock()
            .await
            .take()
            .ok_or("No voice recording is in progress")?;
        enrollment.stop.store(true, Ordering::SeqCst);
        let samples = tokio::task::spawn_blocking(move || enrollment.handle.join())
            .await
            .map_err(|e| e.to_string())?
            .map_err(|_| "Voice recording worker stopped unexpectedly")??;
        validate_samples(&samples.values, samples.rate)?;
        let directory = self.base.join("voice-profiles");
        private_dir(&directory)?;
        // Retain the actual microphone rate: the local model resamples its reference.
        let temporary = directory.join("reference.pending.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: samples.rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let result = (|| {
            let mut writer =
                hound::WavWriter::create(&temporary, spec).map_err(|e| e.to_string())?;
            private_file(&temporary)?;
            for value in samples.values {
                writer
                    .write_sample((value.clamp(-1., 1.) * i16::MAX as f32) as i16)
                    .map_err(|e| e.to_string())?;
            }
            writer.finalize().map_err(|e| e.to_string())?;
            let profile = Profile {
                version: 1,
                consent: true,
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                reference_text: REFERENCE_TEXT.into(),
            };
            let profile_path = directory.join("profile.pending.json");
            fs::write(
                &profile_path,
                serde_json::to_vec_pretty(&profile).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            private_file(&profile_path)?;
            fs::rename(&temporary, self.reference()).map_err(|e| e.to_string())?;
            fs::rename(profile_path, directory.join("profile.json")).map_err(|e| e.to_string())?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
            let _ = fs::remove_file(directory.join("profile.pending.json"));
        }
        result
    }
    pub async fn cancel_reference(&self) {
        let enrollment = self.enrollment.lock().await.take();
        if let Some(e) = enrollment {
            e.stop.store(true, Ordering::SeqCst);
            let _ = tokio::task::spawn_blocking(move || e.handle.join()).await;
        }
    }
    pub async fn forget(&self) -> Result<(), String> {
        self.stop().await;
        self.cancel_reference().await;
        cleanup_generated(&self.base.join("voice-output"));
        for name in [
            "reference.wav",
            "profile.json",
            "reference.pending.wav",
            "profile.pending.json",
        ] {
            match fs::remove_file(self.base.join("voice-profiles").join(name)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(())
    }
    // Reserve while the application's operation lock still protects accepting the
    // request. Stop must invalidate this ticket even before synthesis is polled.
    pub fn reserve_playback(&self) -> u64 {
        let mut ticket = 0;
        self.generation.send_modify(|n| {
            *n = n.wrapping_add(1);
            ticket = *n;
        });
        ticket
    }
    #[cfg(test)]
    async fn speak(&self, text: String, runtime_script: PathBuf) -> Result<(), String> {
        let ticket = self.reserve_playback();
        self.speak_reserved(ticket, text, runtime_script).await
    }
    pub async fn speak_reserved(
        &self,
        current: u64,
        text: String,
        runtime_script: PathBuf,
    ) -> Result<(), String> {
        // Subscribe before checking so a stop racing with this check remains
        // observable by changed() once synthesis starts.
        let mut cancellation = self.generation.subscribe();
        if *self.generation.borrow() != current {
            return Err("Voice playback canceled".into());
        }
        if text.trim().is_empty() || text.chars().count() > 2000 {
            return Err("A coaching suggestion of at most 2,000 characters is required".into());
        }
        if !self.ready() {
            return Err("Install the local voice model and runtime before enabling voice".into());
        }
        if !self.has_voice() {
            return Err("Record and save your own voice before enabling voice".into());
        }
        if self.enrollment.lock().await.is_some() {
            return Err("Finish or cancel voice recording first".into());
        }
        let mut engine_guard = self.engine.lock().await;
        if *self.generation.borrow() != current {
            return Err("Voice playback canceled".into());
        }
        self.playing.store(true, Ordering::SeqCst);
        let output_dir = self.base.join("voice-output");
        let output = output_dir.join(format!("{}.wav", uuid::Uuid::new_v4()));
        let result = self
            .speak_inner(
                text,
                runtime_script,
                output.clone(),
                &mut engine_guard,
                &mut cancellation,
            )
            .await;
        let _ = fs::remove_file(&output);
        if *self.generation.borrow() == current {
            self.playing.store(false, Ordering::SeqCst);
        }
        result
    }
    async fn speak_inner(
        &self,
        text: String,
        script: PathBuf,
        output: PathBuf,
        engine: &mut Option<Engine>,
        cancellation: &mut watch::Receiver<u64>,
    ) -> Result<(), String> {
        private_dir(output.parent().unwrap())?;
        if engine.is_none() {
            let mut command = Command::new(self.python());
            command
                .arg(script)
                .arg("--model")
                .arg(self.base.join("voice-model"));
            // Isolated integration tests never read or overwrite an enrolled user profile.
            #[cfg(test)]
            command.arg("--test-root").arg(&self.base);
            let mut child = command
                .env("HF_HUB_OFFLINE", "1")
                .env("HF_HUB_DISABLE_TELEMETRY", "1")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| format!("Could not start the local voice runtime: {e}"))?;
            let stdin = child
                .stdin
                .take()
                .ok_or("Local voice runtime input is unavailable")?;
            let stdout = BufReader::new(
                child
                    .stdout
                    .take()
                    .ok_or("Local voice runtime output is unavailable")?,
            );
            *engine = Some(Engine {
                child,
                stdin,
                stdout,
            });
        }
        let active = engine.as_mut().unwrap();
        let request = serde_json::json!({"id": uuid::Uuid::new_v4().to_string(), "op":"synthesize", "text":text, "reference_audio":self.reference(), "reference_text":REFERENCE_TEXT, "output":output});
        let request_id = request["id"].clone();
        let inference = async {
            active
                .stdin
                .write_all(format!("{request}\n").as_bytes())
                .await
                .map_err(|e| e.to_string())?;
            active.stdin.flush().await.map_err(|e| e.to_string())?;
            let mut line = String::new();
            active
                .stdout
                .read_line(&mut line)
                .await
                .map_err(|e| e.to_string())?;
            if line.len() > 65536 {
                return Err("Local voice runtime returned an oversized response".into());
            }
            let response: serde_json::Value = serde_json::from_str(&line)
                .map_err(|_| "Local voice runtime stopped or returned an invalid response")?;
            if response["id"] != request_id {
                return Err("Local voice runtime returned an unexpected response".into());
            }
            if response["ok"] != true {
                return Err(response["error"]
                    .as_str()
                    .unwrap_or("Local voice generation failed")
                    .to_string());
            }
            // macOS aliases /var to /private/var; the local engine deliberately
            // returns canonical paths. Verify the actual requested file identity.
            let returned_path = response["path"]
                .as_str()
                .and_then(|p| fs::canonicalize(p).ok());
            let expected_path = fs::canonicalize(&output).ok();
            if !output.is_file() || returned_path.is_none() || returned_path != expected_path {
                return Err("Local voice runtime did not return the requested audio file".into());
            }
            private_file(&output)?;
            Ok(())
        };
        let inference_result = tokio::select! {
            biased;
            _ = cancellation.changed() => Err("Voice playback canceled".into()),
            value = tokio::time::timeout(Duration::from_secs(180), inference) => value.unwrap_or_else(|_| Err("Local voice generation timed out".into())),
        };
        if let Err(e) = inference_result {
            if let Some(mut e) = engine.take() {
                let _ = e.child.kill().await;
                let _ = e.child.wait().await;
            }
            return Err(e);
        }
        let mut player = Command::new("/usr/bin/afplay")
            .arg(&output)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Could not play coaching audio: {e}"))?;
        tokio::select! {
            biased;
            _ = cancellation.changed() => { let _ = player.kill().await; let _ = player.wait().await; Err("Voice playback canceled".into()) },
            result = player.wait() => match result { Ok(status) if status.success() => Ok(()), Ok(_) => Err("Coaching audio playback failed".into()), Err(e) => Err(e.to_string()) },
        }
    }
    pub async fn stop(&self) {
        self.generation.send_modify(|n| *n = n.wrapping_add(1));
        let mut engine = self.engine.lock().await;
        if let Some(mut e) = engine.take() {
            let _ = e.child.kill().await;
            let _ = e.child.wait().await;
        }
        self.playing.store(false, Ordering::SeqCst);
    }
    pub async fn shutdown(&self) {
        self.cancel_reference().await;
        self.stop().await;
    }
}
fn require_consent(consent: bool) -> Result<(), String> {
    if consent {
        Ok(())
    } else {
        Err(
            "Confirm this is your own voice and that you consent to creating a local voice profile"
                .into(),
        )
    }
}
fn validate_samples(samples: &[f32], rate: u32) -> Result<(), String> {
    if rate == 0 || samples.len() < rate as usize * 5 {
        return Err("Read the displayed passage for at least five seconds".into());
    }
    if samples.len() > rate as usize * MAX_SECONDS as usize {
        return Err("Voice recordings must be at most 30 seconds".into());
    }
    if samples.iter().any(|v| !v.is_finite()) {
        return Err("Microphone audio contained invalid samples".into());
    }
    let rms =
        (samples.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / samples.len() as f64).sqrt();
    if rms < 0.003 {
        return Err(
            "The voice recording was too quiet. Check the microphone and read the passage again"
                .into(),
        );
    }
    Ok(())
}
// Recover only our UUID-named synthesis files after an interrupted process.
// Never traverse a symlink or touch reference samples, models, or user-named files.
fn cleanup_generated(directory: &Path) {
    let metadata = match fs::symlink_metadata(directory) {
        Ok(v) if v.is_dir() && !v.file_type().is_symlink() => v,
        _ => return,
    };
    let _ = metadata;
    if let Ok(entries) = fs::read_dir(directory) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ours = path.extension().and_then(|e| e.to_str()) == Some("wav")
                && path
                    .file_stem()
                    .and_then(|e| e.to_str())
                    .and_then(|s| uuid::Uuid::parse_str(s).ok())
                    .is_some();
            if ours
                && entry
                    .file_type()
                    .map(|t| t.is_file() && !t.is_symlink())
                    .unwrap_or(false)
            {
                let _ = fs::remove_file(path);
            }
        }
    }
}
fn private_dir(path: &Path) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("Voice storage must be a local directory, not a symbolic link".into());
        }
    }
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn private_file(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn record_reference(
    stop: Arc<AtomicBool>,
    ready: std::sync::mpsc::SyncSender<Result<(), String>>,
) -> Result<Samples, String> {
    let result = (|| {
        let device = cpal::default_host()
            .default_input_device()
            .ok_or("No microphone is available")?;
        let config = device.default_input_config().map_err(|e| e.to_string())?;
        let rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        let buffer = Arc::new(Mutex::new(Vec::with_capacity(
            rate as usize * MAX_SECONDS as usize,
        )));
        let error = Arc::new(Mutex::new(None::<String>));
        let error_slot = error.clone();
        let error_stop = stop.clone();
        let on_error = move |e: cpal::StreamError| {
            *error_slot.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(format!("Microphone error: {e}"));
            error_stop.store(true, Ordering::SeqCst);
        };
        let data_buffer = buffer.clone();
        let callback_stop = stop.clone();
        let limit = rate as usize * MAX_SECONDS as usize;
        let append = move |data: Vec<f32>| {
            let mut samples = data_buffer.lock().unwrap_or_else(|e| e.into_inner());
            let remaining = limit.saturating_sub(samples.len());
            samples.extend(data.into_iter().take(remaining));
            if samples.len() >= limit {
                callback_stop.store(true, Ordering::SeqCst);
            }
        };
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    append(
                        data.chunks(channels)
                            .map(|c| c.iter().sum::<f32>() / channels as f32)
                            .collect(),
                    )
                },
                on_error,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    append(
                        data.chunks(channels)
                            .map(|c| {
                                c.iter().map(|v| *v as f32 / 32768.).sum::<f32>() / channels as f32
                            })
                            .collect(),
                    )
                },
                on_error,
                None,
            ),
            _ => return Err("Microphone uses an unsupported sample format".into()),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        let _ = ready.send(Ok(()));
        let started = Instant::now();
        while !stop.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(MAX_SECONDS) {
            thread::sleep(Duration::from_millis(25));
        }
        drop(stream);
        if let Some(e) = error.lock().unwrap_or_else(|e| e.into_inner()).take() {
            return Err(e);
        }
        let values = std::mem::take(&mut *buffer.lock().unwrap_or_else(|e| e.into_inner()));
        Ok(Samples { values, rate })
    })();
    if let Err(e) = &result {
        let _ = ready.send(Err(e.clone()));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn base() -> PathBuf {
        std::env::temp_dir().join(format!("live-coach-voice-test-{}", uuid::Uuid::new_v4()))
    }
    fn fixture_reference(voice: &Voice) {
        private_dir(voice.reference().parent().unwrap()).unwrap();
        let mut writer = hound::WavWriter::create(
            voice.reference(),
            hound::WavSpec {
                channels: 1,
                sample_rate: 16000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..16000 * 6 {
            writer.write_sample(1000i16).unwrap();
        }
        writer.finalize().unwrap();
    }
    #[test]
    fn explicit_consent_required() {
        assert!(require_consent(false).is_err());
        assert!(require_consent(true).is_ok());
    }
    #[test]
    fn reference_rejects_silence_short_and_invalid_audio() {
        assert!(validate_samples(&vec![0.; 16000 * 6], 16000).is_err());
        assert!(validate_samples(&vec![0.1; 16000 * 4], 16000).is_err());
        assert!(validate_samples(&vec![f32::NAN; 16000 * 6], 16000).is_err());
        assert!(validate_samples(&vec![0.1; 16000 * 31], 16000).is_err());
        assert!(validate_samples(&vec![0.1; 16000 * 6], 16000).is_ok());
    }
    #[tokio::test]
    async fn cold_status_never_starts_capture_or_playback() {
        let path = base();
        let voice = Voice::at(path.clone());
        let state = voice.status().await;
        assert!(!state.recording && !state.playing && !state.runtime_ready && !state.has_voice);
        assert!(!path.exists());
        assert!(voice.start_reference(false).await.is_err());
        assert!(!path.exists());
        voice.shutdown().await;
    }
    #[tokio::test]
    async fn profile_requires_matching_consent_and_can_be_deleted() {
        let path = base();
        let voice = Voice::at(path.clone());
        let folder = path.join("voice-profiles");
        private_dir(&folder).unwrap();
        fixture_reference(&voice);
        let profile = Profile {
            version: 1,
            consent: false,
            created_at: 1,
            reference_text: REFERENCE_TEXT.into(),
        };
        fs::write(
            folder.join("profile.json"),
            serde_json::to_vec(&profile).unwrap(),
        )
        .unwrap();
        assert!(!voice.has_voice());
        let profile = Profile {
            consent: true,
            ..profile
        };
        fs::write(
            folder.join("profile.json"),
            serde_json::to_vec(&profile).unwrap(),
        )
        .unwrap();
        assert!(voice.has_voice());
        voice.forget().await.unwrap();
        assert!(!voice.has_voice());
        fs::remove_dir_all(path).unwrap();
    }
    #[tokio::test]
    async fn cancellation_advances_and_missing_runtime_fails_safely() {
        let voice = Voice::at(base());
        let before = *voice.generation.borrow();
        voice.stop().await;
        assert!(*voice.generation.borrow() > before);
        assert!(voice
            .speak("A suggestion".into(), "missing.py".into())
            .await
            .is_err());
        assert!(!voice.status().await.playing);
    }
    #[tokio::test]
    async fn stop_invalidates_playback_accepted_before_synthesis_starts() {
        let path = base();
        let voice = Voice::at(path.clone());
        // Deterministically reproduce the gap after request acceptance but before
        // speak is first polled. No runtime/profile is needed: cancellation must
        // win before either is accessed, and no output directory may be created.
        let ticket = voice.reserve_playback();
        let delayed = voice.speak_reserved(ticket, "A suggestion".into(), "missing.py".into());
        voice.stop().await;
        assert_eq!(delayed.await.unwrap_err(), "Voice playback canceled");
        assert!(!voice.status().await.playing);
        assert!(!path.exists());
    }
    #[tokio::test]
    async fn teardown_invalidates_playback_accepted_before_synthesis_starts() {
        let path = base();
        let voice = Voice::at(path.clone());
        let ticket = voice.reserve_playback();
        voice.shutdown().await;
        assert_eq!(
            voice
                .speak_reserved(ticket, "A suggestion".into(), "missing.py".into())
                .await
                .unwrap_err(),
            "Voice playback canceled"
        );
        assert!(!path.exists());
    }
    #[tokio::test]
    async fn stop_cancels_actual_subprocess_before_playback() {
        let path = base();
        let voice = Voice::at(path.clone());
        fixture_reference(&voice);
        let profile = Profile {
            version: 1,
            consent: true,
            created_at: 1,
            reference_text: REFERENCE_TEXT.into(),
        };
        fs::write(
            path.join("voice-profiles/profile.json"),
            serde_json::to_vec(&profile).unwrap(),
        )
        .unwrap();
        private_dir(voice.python().parent().unwrap()).unwrap();
        // A real blocking child validates cancellation without relying on Apple's
        // name-sensitive /usr/bin/python3 developer-tools shim or Python installation.
        fs::write(
            voice.python(),
            "#!/bin/sh\nIFS= read -r request\n: > \"$1.started\"\nexec /bin/sleep 60\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(voice.python(), fs::Permissions::from_mode(0o700)).unwrap();
        for name in [
            "config.json",
            "model.safetensors",
            "speech_tokenizer/model.safetensors",
        ] {
            let target = path.join("voice-model").join(name);
            private_dir(target.parent().unwrap()).unwrap();
            fs::write(target, b"fixture").unwrap();
        }
        let script = path.join("slow.py");
        fs::write(
            &script,
            "import sys,time,pathlib
line=sys.stdin.readline()
pathlib.Path(__file__).with_suffix('.started').touch()
time.sleep(60)
",
        )
        .unwrap();
        let speak_voice = voice.clone();
        let mut task = tokio::spawn(async move {
            speak_voice
                .speak("A short coaching suggestion".into(), script)
                .await
        });
        tokio::time::timeout(Duration::from_secs(15), async {
            while !path.join("slow.py.started").is_file() {
                tokio::select! {
                    result = &mut task => panic!("Synthesis exited before the cancellation checkpoint: {result:?}"),
                    _ = tokio::time::sleep(Duration::from_millis(10)) => {},
                }
            }
        })
        .await
        .unwrap();
        assert!(voice.status().await.playing);
        tokio::time::timeout(Duration::from_secs(3), voice.stop())
            .await
            .unwrap();
        assert!(task.await.unwrap().unwrap_err().contains("canceled"));
        assert!(!voice.status().await.playing);
        assert_eq!(fs::read_dir(path.join("voice-output")).unwrap().count(), 0);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn startup_cleanup_is_scoped_and_does_not_follow_symlinks() {
        let path = base();
        let output = path.join("voice-output");
        private_dir(&output).unwrap();
        let old_output = output.join(format!("{}.wav", uuid::Uuid::new_v4()));
        fs::write(&old_output, b"generated").unwrap();
        let user_file = output.join("user.wav");
        fs::write(&user_file, b"keep").unwrap();
        let reference = path.join("voice-profiles/reference.wav");
        private_dir(reference.parent().unwrap()).unwrap();
        fs::write(&reference, b"keep reference").unwrap();
        let link = output.join(format!("{}.wav", uuid::Uuid::new_v4()));
        std::os::unix::fs::symlink(&reference, &link).unwrap();
        let _voice = Voice::at(path.clone());
        assert!(!old_output.exists());
        assert!(user_file.exists());
        assert!(link.is_symlink());
        assert!(reference.exists());
        let external = path.join("external");
        private_dir(&external).unwrap();
        let external_audio = external.join(format!("{}.wav", uuid::Uuid::new_v4()));
        fs::write(&external_audio, b"keep external").unwrap();
        let alias = path.join("alias");
        std::os::unix::fs::symlink(&external, &alias).unwrap();
        cleanup_generated(&alias);
        assert!(external_audio.exists());
        assert!(private_dir(&alias).is_err());
        fs::remove_dir_all(path).unwrap();
    }
    #[tokio::test]
    #[ignore = "Plays a short synthetic voice fixture through the installed local model; run explicitly after notifying the user"]
    async fn real_local_engine_playback_and_cleanup() {
        let installed = dirs::data_dir().unwrap().join("com.dnalevity.livecoach");
        let path = base();
        let voice = Voice::at(path.clone());
        private_dir(&path.join("voice-runtime")).unwrap();
        std::os::unix::fs::symlink(
            installed.join("voice-runtime/.venv"),
            path.join("voice-runtime/.venv"),
        )
        .unwrap();
        std::os::unix::fs::symlink(installed.join("voice-model"), path.join("voice-model"))
            .unwrap();
        assert!(
            voice.ready(),
            "Install the local voice runtime/model before running this explicit smoke test"
        );
        private_dir(voice.reference().parent().unwrap()).unwrap();
        let aiff = path.join("synthetic-reference.aiff");
        let status = std::process::Command::new("/usr/bin/say")
            .args(["-r", "160", "-o"])
            .arg(&aiff)
            .arg(REFERENCE_TEXT)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "Could not synthesize the test reference; no live recording was attempted"
        );
        let status = std::process::Command::new("/usr/bin/afconvert")
            .args(["-f", "WAVE", "-d", "LEI16@16000", "-c", "1"])
            .arg(&aiff)
            .arg(voice.reference())
            .status()
            .unwrap();
        assert!(status.success());
        let profile = Profile {
            version: 1,
            consent: true,
            created_at: 1,
            reference_text: REFERENCE_TEXT.into(),
        };
        fs::write(
            path.join("voice-profiles/profile.json"),
            serde_json::to_vec(&profile).unwrap(),
        )
        .unwrap();
        assert!(voice.has_voice());
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("voice-runtime/engine.py");
        let started = Instant::now();
        voice
            .speak(
                "I can pause and choose what works for me.".into(),
                script.clone(),
            )
            .await
            .expect("Rust → real local model → afplay failed");
        let cold = started.elapsed().as_secs_f64();
        assert!(!voice.status().await.playing);
        assert_eq!(fs::read_dir(path.join("voice-output")).unwrap().count(), 0);
        assert!(
            voice.has_voice(),
            "Playback must not delete its reference profile"
        );
        let started = Instant::now();
        voice
            .speak("I can think before I answer.".into(), script)
            .await
            .expect("Persistent local model playback failed");
        let warm = started.elapsed().as_secs_f64();
        assert_eq!(fs::read_dir(path.join("voice-output")).unwrap().count(), 0);
        voice.shutdown().await;
        assert!(voice.engine.lock().await.is_none());
        eprintln!("REAL_LOCAL_VOICE_SMOKE {{\"cold_generation_and_playback_seconds\":{cold:.3},\"warm_generation_and_playback_seconds\":{warm:.3},\"outputs_remaining\":0,\"user_profile_touched\":false}}");
        fs::remove_dir_all(path).unwrap();
    }
}
