//! In-memory capture bridge. Core Audio and Parakeet originate in Meetily (MIT).
use crate::upstream::{core_audio::CoreAudioCapture, parakeet::ParakeetModel, resample::resample};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use futures_util::StreamExt;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc::{sync_channel, SyncSender},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};
pub type TranscriptCallback = Arc<dyn Fn(String, String) + Send + Sync>;
pub type StatusCallback = Arc<dyn Fn(String) + Send + Sync>;
// Whisper setup and final segment extraction adapted from Meetily whisper_engine.rs.
enum Decoder {
    Parakeet(ParakeetModel),
    Whisper(whisper_rs::WhisperContext),
}
impl Decoder {
    fn load(path: &std::path::Path, engine: &str) -> Result<Self, String> {
        match engine {
            "parakeet" => ParakeetModel::new(path, true)
                .map(Self::Parakeet)
                .map_err(|e| format!("Cannot load Parakeet model: {e}")),
            "whisper" => whisper_rs::WhisperContext::new_with_params(
                &path.to_string_lossy(),
                whisper_rs::WhisperContextParameters::default(),
            )
            .map(Self::Whisper)
            .map_err(|e| format!("Cannot load Whisper model: {e}")),
            _ => Err("Choose Parakeet or Whisper speech recognition".into()),
        }
    }
    fn transcribe(&mut self, samples: Vec<f32>) -> Result<String, String> {
        match self {
            Self::Parakeet(m) => m
                .transcribe_samples(samples)
                .map(|r| r.text)
                .map_err(|e| e.to_string()),
            Self::Whisper(m) => {
                let mut state = m.create_state().map_err(|e| e.to_string())?;
                let mut params =
                    whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy {
                        best_of: 1,
                    });
                params.set_language(None);
                params.set_no_timestamps(true);
                params.set_print_special(false);
                params.set_print_progress(false);
                params.set_print_realtime(false);
                params.set_print_timestamps(false);
                params.set_suppress_non_speech_tokens(true);
                state.full(params, &samples).map_err(|e| e.to_string())?;
                let mut text = String::new();
                for i in 0..state.full_n_segments().map_err(|e| e.to_string())? {
                    text.push_str(
                        &state
                            .full_get_segment_text_lossy(i)
                            .map_err(|e| e.to_string())?,
                    );
                }
                Ok(text.trim().to_string())
            }
        }
    }
}
struct Chunk {
    samples: Vec<f32>,
    rate: u32,
    source: String,
    epoch: u64,
}
#[derive(Default)]
struct Activity {
    mic: AtomicU64,
    system: AtomicU64,
    mic_peak: AtomicU32,
    system_peak: AtomicU32,
}
#[derive(Debug, serde::Serialize)]
pub struct CaptureActivity {
    pub microphone_samples: u64,
    pub system_samples: u64,
    pub microphone_peak: f32,
    pub system_peak: f32,
}
#[derive(Default)]
struct PauseGate {
    paused: AtomicBool,
    epoch: AtomicU64,
}
impl PauseGate {
    fn set(&self, value: bool) {
        if self.paused.swap(value, Ordering::SeqCst) != value {
            self.epoch.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn admits(&self, epoch: u64) -> bool {
        !self.paused.load(Ordering::SeqCst) && self.epoch.load(Ordering::SeqCst) == epoch
    }
}
pub struct CaptureHandle {
    pause: Arc<PauseGate>,
    activity: Arc<Activity>,
    stop: Arc<AtomicBool>,
    capture: Option<JoinHandle<()>>,
    worker: Option<JoinHandle<()>>,
}
impl CaptureHandle {
    pub fn set_paused(&self, value: bool) {
        self.pause.set(value);
    }
    pub fn activity(&self) -> CaptureActivity {
        CaptureActivity {
            microphone_samples: self.activity.mic.load(Ordering::Relaxed),
            system_samples: self.activity.system.load(Ordering::Relaxed),
            microphone_peak: f32::from_bits(self.activity.mic_peak.load(Ordering::Relaxed)),
            system_peak: f32::from_bits(self.activity.system_peak.load(Ordering::Relaxed)),
        }
    }
    pub fn is_finished(&self) -> bool {
        self.capture
            .as_ref()
            .map(|h| h.is_finished())
            .unwrap_or(true)
    }
    pub async fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let a = self.capture.take();
        let b = self.worker.take();
        let _ = tokio::task::spawn_blocking(move || {
            if let Some(h) = a {
                let _ = h.join();
            }
            if let Some(h) = b {
                let _ = h.join();
            }
        })
        .await;
    }
}
impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Loads existing model read-only before opening either capture device. No downloads or recording files.
pub async fn start(
    model_path: PathBuf,
    engine: String,
    callback: TranscriptCallback,
    status: StatusCallback,
) -> Result<CaptureHandle, String> {
    let model = tokio::task::spawn_blocking(move || Decoder::load(&model_path, &engine))
        .await
        .map_err(|e| e.to_string())??;
    let stop = Arc::new(AtomicBool::new(false));
    let pause = Arc::new(PauseGate::default());
    let worker_pause = pause.clone();
    let capture_pause = pause.clone();
    let (tx, rx) = sync_channel::<Chunk>(4);
    let worker_stop = stop.clone();
    let worker_status = status.clone();
    let worker = std::thread::spawn(move || {
        let mut model = model;
        while !worker_stop.load(Ordering::SeqCst) {
            let chunk = match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(v) => v,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => break,
            };
            if !worker_pause.admits(chunk.epoch) {
                continue;
            }
            let started = std::time::Instant::now();
            let result = resample(&chunk.samples, chunk.rate, 16000)
                .map_err(|e| e.to_string())
                .and_then(|v| model.transcribe(v));
            if worker_stop.load(Ordering::SeqCst) {
                break;
            }
            if !worker_pause.admits(chunk.epoch) {
                continue;
            }
            match result {
                Ok(text) if !text.trim().is_empty() => {
                    callback(text, chunk.source);
                    worker_status(format!(
                        "Speech recognized in {:.1}s",
                        started.elapsed().as_secs_f32()
                    ));
                }
                Ok(_) => {}
                Err(e) => worker_status(format!("Speech recognition failed: {e}")),
            }
        }
    });
    let capture_stop = stop.clone();
    let activity = Arc::new(Activity::default());
    let capture_activity = activity.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let capture = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move{
            let (sample_tx,mut sample_rx)=tokio::sync::mpsc::channel::<Vec<f32>>(64);
            let mic=match open_mic(sample_tx,status.clone(),capture_stop.clone()){Ok(v)=>v,Err(e)=>{let _=ready_tx.send(Err(e));return;}};
            let mut system=match CoreAudioCapture::new().and_then(|c|c.stream()){Ok(v)=>v,Err(e)=>{let _=ready_tx.send(Err(format!("System audio capture failed: {e}")));return;}};
            let system_rate=system.sample_rate();
            let mut mic_segment=Segmenter::new(mic.1,"Microphone",tx.clone(),status.clone());
            let mut system_segment=Segmenter::new(system_rate,"System audio",tx,status.clone());
            let _=ready_tx.send(Ok(()));
            let mut check=tokio::time::interval(Duration::from_millis(50));
            loop {
                tokio::select! {
                    _=check.tick()=>{if capture_stop.load(Ordering::SeqCst){break;}},
                    data=sample_rx.recv()=>{if let Some(data)=data {capture_activity.mic.fetch_add(data.len() as u64,Ordering::Relaxed);let peak=data.iter().map(|v|v.abs()).fold(0f32,f32::max);capture_activity.mic_peak.fetch_max(peak.to_bits(),Ordering::Relaxed);mic_segment.push_gated(&data,&capture_pause)}else{status("fatal: Microphone stream ended".into());break;}},
                    sample=system.next()=>{if let Some(v)=sample {capture_activity.system.fetch_add(1,Ordering::Relaxed);capture_activity.system_peak.fetch_max(v.abs().to_bits(),Ordering::Relaxed);system_segment.push_gated(&[v],&capture_pause)}else{status("fatal: System audio stream ended".into());break;}}
                }
            }
            capture_stop.store(true,Ordering::SeqCst);
            let _=mic.0.pause();drop(mic);drop(system);
        });
    });
    match tokio::task::spawn_blocking(move || ready_rx.recv())
        .await
        .map_err(|e| e.to_string())?
    {
        Ok(Ok(())) => Ok(CaptureHandle {
            pause,
            activity,
            stop,
            capture: Some(capture),
            worker: Some(worker),
        }),
        result => {
            stop.store(true, Ordering::SeqCst);
            let _ = tokio::task::spawn_blocking(move || {
                let _ = capture.join();
                let _ = worker.join();
            })
            .await;
            Err(match result {
                Ok(Err(e)) => e,
                _ => "Audio capture did not initialize".into(),
            })
        }
    }
}

// Adapted from Meetily audio/stream.rs: CPAL sample conversion and stream lifecycle.
fn open_mic(
    sender: tokio::sync::mpsc::Sender<Vec<f32>>,
    status: StatusCallback,
    stop: Arc<AtomicBool>,
) -> Result<(cpal::Stream, u32), String> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or("No microphone is available")?;
    let config = device.default_input_config().map_err(|e| e.to_string())?;
    let rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let err = move |e| {
        stop.store(true, Ordering::SeqCst);
        status(format!("fatal: Microphone error: {e}"));
    };
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let _ = sender.try_send(
                    data.chunks(channels)
                        .map(|v| v.iter().sum::<f32>() / channels as f32)
                        .collect(),
                );
            },
            err,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let _ = sender.try_send(
                    data.chunks(channels)
                        .map(|v| {
                            v.iter().map(|s| *s as f32 / 32768.).sum::<f32>() / channels as f32
                        })
                        .collect(),
                );
            },
            err,
            None,
        ),
        _ => return Err("Microphone uses an unsupported sample format".into()),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, rate))
}

/// Energy segmentation with 0.6s endpoint silence, 5s maximum and 0.5s minimum.
/// Labels describe capture channels; they are not inferred speaker identities.
struct Segmenter {
    epoch: u64,
    rate: u32,
    source: String,
    tx: SyncSender<Chunk>,
    status: StatusCallback,
    window: Vec<f32>,
    speech: Vec<f32>,
    silent: usize,
}
impl Segmenter {
    fn new(rate: u32, source: &str, tx: SyncSender<Chunk>, status: StatusCallback) -> Self {
        Self {
            epoch: 0,
            rate,
            source: source.into(),
            tx,
            status,
            window: vec![],
            speech: vec![],
            silent: 0,
        }
    }
    fn push_gated(&mut self, data: &[f32], gate: &PauseGate) {
        let epoch = gate.epoch.load(Ordering::SeqCst);
        if epoch != self.epoch || gate.paused.load(Ordering::SeqCst) {
            self.window.clear();
            self.speech.clear();
            self.silent = 0;
            self.epoch = epoch;
        }
        if gate.admits(epoch) {
            self.push(data);
        }
    }
    fn push(&mut self, data: &[f32]) {
        self.window.extend_from_slice(data);
        let frame = (self.rate / 50).max(1) as usize;
        while self.window.len() >= frame {
            let samples: Vec<_> = self.window.drain(..frame).collect();
            let rms = (samples.iter().map(|v| v * v).sum::<f32>() / frame as f32).sqrt();
            if rms > 0.006 {
                self.silent = 0;
                self.speech.extend(samples);
            } else if !self.speech.is_empty() {
                self.silent += frame;
                self.speech.extend(samples);
            }
            if self.speech.len() >= self.rate as usize * 5
                || self.silent >= self.rate as usize * 3 / 5
            {
                self.flush();
            }
        }
    }
    fn flush(&mut self) {
        let samples = std::mem::take(&mut self.speech);
        self.silent = 0;
        if samples.len() < self.rate as usize / 2 {
            return;
        }
        if self
            .tx
            .try_send(Chunk {
                epoch: self.epoch,
                samples,
                rate: self.rate,
                source: self.source.clone(),
            })
            .is_err()
        {
            (self.status)("Speech recognition is busy; a segment was dropped. Use a faster model or pause other applications.".into());
        }
    }
}

/// Exercises the same segmentation, resampling and real ONNX decoder used by capture.
pub fn transcribe_fixture(
    model_path: &std::path::Path,
    engine: &str,
    samples: Vec<f32>,
    rate: u32,
) -> Result<Vec<String>, String> {
    let (tx, rx) = sync_channel(64);
    let mut segmenter = Segmenter::new(rate, "Fixture", tx, Arc::new(|_| {}));
    segmenter.push(&samples);
    segmenter.flush();
    drop(segmenter);
    let mut model = Decoder::load(model_path, engine)?;
    rx.into_iter()
        .map(|c| {
            resample(&c.samples, c.rate, 16000)
                .map_err(|e| e.to_string())
                .and_then(|s| model.transcribe(s))
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn voice_playback_drops_partial_and_stale_chunks() {
        let (tx, rx) = sync_channel(10);
        let gate = PauseGate::default();
        let mut s = Segmenter::new(16000, "Test", tx, Arc::new(|_| {}));
        s.push_gated(&vec![0.1; 16000], &gate);
        gate.set(true);
        s.push_gated(&vec![0.2; 32000], &gate);
        gate.set(false);
        assert!(!gate.admits(0));
        assert!(!gate.admits(1));
        assert!(gate.admits(2));
        s.push_gated(&vec![0.; 16000], &gate);
        assert!(rx.try_recv().is_err());
        s.push_gated(&vec![0.1; 16000], &gate);
        s.push_gated(&vec![0.; 16000], &gate);
        assert_eq!(rx.try_recv().unwrap().epoch, 2);
    }
    #[test]
    fn silence_is_never_transcribed() {
        let (tx, rx) = sync_channel(10);
        let mut s = Segmenter::new(16000, "Test", tx, Arc::new(|_| {}));
        s.push(&vec![0.; 160000]);
        s.flush();
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn speech_finalizes_after_silence() {
        let (tx, rx) = sync_channel(10);
        let mut s = Segmenter::new(16000, "Test", tx, Arc::new(|_| {}));
        s.push(&vec![0.1; 16000]);
        s.push(&vec![0.; 16000]);
        assert_eq!(rx.try_recv().unwrap().source, "Test");
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn bounded_queue_warns_instead_of_blocking_capture() {
        use std::sync::atomic::AtomicUsize;
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        let (tx, _rx) = sync_channel(1);
        let mut s = Segmenter::new(
            16000,
            "Test",
            tx,
            Arc::new(move |_| {
                seen.fetch_add(1, Ordering::SeqCst);
            }),
        );
        s.push(&vec![0.1; 16000 * 11]);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn downsampling_preserves_time_and_rejects_ultrasonic_energy() {
        let samples: Vec<_> = (0..48000)
            .map(|i| (2. * std::f32::consts::PI * 12000. * i as f32 / 48000.).sin())
            .collect();
        let output = resample(&samples, 48000, 16000).unwrap();
        assert!((output.len() as i64 - 16000).abs() < 512);
        let rms = (output.iter().map(|v| v * v).sum::<f32>() / output.len() as f32).sqrt();
        assert!(rms < 0.05, "aliased energy: {rms}");
    }
}
