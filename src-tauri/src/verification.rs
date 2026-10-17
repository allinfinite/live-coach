use crate::{
    capture,
    coach::{Coach, CoachEvent, CoachSettings, Segment},
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
pub fn run_if_requested() -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_none_or(|s| s != "--self-test") {
        return false;
    }
    let result = run(&args);
    match result {
        Ok(()) => {
            println!("SELF_TEST_PASS");
        }
        Err(e) => {
            eprintln!("SELF_TEST_FAIL: {e}");
            std::process::exit(1);
        }
    }
    true
}
fn run(args: &[String]) -> Result<(), String> {
    let model = args
        .get(2)
        .ok_or("Usage: --self-test MODEL_DIR FIXTURE.wav [--capture]")?;
    let wav = args.get(3).ok_or("WAV fixture required")?;
    let mut reader = hound_reader(wav)?;
    let rate = reader.0;
    let samples = std::mem::take(&mut reader.1);
    let started = Instant::now();
    let lines =
        capture::transcribe_fixture(std::path::Path::new(model), "parakeet", samples, rate)?;
    println!("REAL_ASR_SECONDS={:.2}", started.elapsed().as_secs_f32());
    if lines.is_empty() || !lines.iter().any(|s| s.to_lowercase().contains("time")) {
        return Err(format!("Speech fixture not recognized: {lines:?}"));
    }
    println!("REAL_ASR_SEGMENTS={lines:?}");
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    runtime.block_on(async move{
  let events=Arc::new(Mutex::new(vec![]));let out=events.clone();let coach=Coach::new(CoachSettings{model:"gemma4:latest".into(),..Default::default()},Arc::new(move|e|{out.lock().unwrap().push(e);}));
  if coach.start(false).await.is_ok(){return Err("Consent bypass".into());}
  let token=coach.start(true).await?;
  let inference_started=Instant::now();
  for (i,text) in lines.into_iter().enumerate(){coach.ingest_for_session(token,Segment{id:format!("fixture-{i}"),text,speaker:Some("Synthetic fixture".into()),timestamp_ms:i as u64*6000}).await?;}
  loop{
   {let guard=events.lock().unwrap();if let Some(e)=guard.iter().find(|e|matches!(e,CoachEvent::Insight{..})){println!("OLLAMA_SECONDS={:.2}",inference_started.elapsed().as_secs_f32());println!("COACH_RESULT={}",serde_json::to_string(e).unwrap());break;}
    if let Some(CoachEvent::Error{message})=guard.iter().find(|e|matches!(e,CoachEvent::Error{..})){return Err(message.clone());}}
   if inference_started.elapsed()>Duration::from_secs(75){return Err("Ollama coaching timeout".into());}tokio::time::sleep(Duration::from_millis(100)).await;
  }
  coach.stop().await;coach.clear().await;
  if args.iter().any(|a|a=="--capture"){
   println!("CAPTURE_PROBE_START: synthetic fixture only; no participant conversation");
   events.lock().unwrap().clear();let token=coach.start(true).await?;let capture_coach=coach.clone();let rt=tokio::runtime::Handle::current();
   let segments=Arc::new(std::sync::atomic::AtomicUsize::new(0));let count=segments.clone();
   let callback=Arc::new(move|text:String,source:String|{println!("CAPTURE_SEGMENT {source}: {text}");count.fetch_add(1,std::sync::atomic::Ordering::SeqCst);let coach=capture_coach.clone();rt.spawn(async move{let _=coach.ingest_for_session(token,Segment{id:uuid::Uuid::new_v4().to_string(),text,speaker:Some(source),timestamp_ms:0}).await;});});
   let handle=capture::start(model.into(),"parakeet".into(),callback,Arc::new(|m|println!("CAPTURE_STATUS {m}"))).await?;
   println!("CAPTURE_DEVICES_READY");
   let mut playback=std::process::Command::new("/usr/bin/afplay").arg(wav).spawn().map_err(|e|e.to_string())?;
   tokio::time::sleep(Duration::from_secs(12)).await;
   let metrics=handle.activity();println!("CAPTURE_ACTIVITY={}",serde_json::to_string(&metrics).unwrap());
   let failed=handle.is_finished();handle.stop().await;let _=playback.wait();
   let coach_wait=Instant::now();
   loop {
    {let guard=events.lock().unwrap();if let Some(e)=guard.iter().find(|e|matches!(e,CoachEvent::Insight{..})){println!("CAPTURE_COACH_RESULT={}",serde_json::to_string(e).unwrap());break;}if let Some(CoachEvent::Error{message})=guard.iter().find(|e|matches!(e,CoachEvent::Error{..})){return Err(message.clone());}}
    if coach_wait.elapsed()>Duration::from_secs(60){return Err("No coaching result from hardware transcript stream".into());}tokio::time::sleep(Duration::from_millis(100)).await;
   }
   let before=segments.load(std::sync::atomic::Ordering::SeqCst);tokio::time::sleep(Duration::from_millis(500)).await;
   if segments.load(std::sync::atomic::Ordering::SeqCst)!=before{return Err("Late segment after teardown".into());}
   coach.stop().await;coach.clear().await;
   if failed || before==0 || metrics.microphone_samples==0 || metrics.system_samples==0 || metrics.system_peak<0.006{return Err("Capture probe did not prove simultaneous mic/system sample flow and system transcription".into());}
   println!("CAPTURE_PROBE_STOPPED no late segments; {before} finalized segments");
  }
  Ok(())
 })
}
// Small explicit PCM WAV reader used only by the opt-in CLI verification, never session capture.
fn hound_reader(path: &str) -> Result<(u32, Vec<f32>), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() < 44 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("Expected PCM WAV".into());
    }
    let mut pos = 12;
    let (mut channels, mut rate, mut bits, mut encoding) = (0, 0, 0, 0);
    let mut audio = &[][..];
    while pos + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let end = (pos + 8 + size).min(bytes.len());
        let data = &bytes[pos + 8..end];
        match &bytes[pos..pos + 4] {
            b"fmt " if data.len() >= 16 => {
                encoding = u16::from_le_bytes(data[0..2].try_into().unwrap());
                channels = u16::from_le_bytes(data[2..4].try_into().unwrap());
                rate = u32::from_le_bytes(data[4..8].try_into().unwrap());
                bits = u16::from_le_bytes(data[14..16].try_into().unwrap());
            }
            b"data" => audio = data,
            _ => {}
        }
        pos += 8 + size + (size % 2);
    }
    if encoding != 1 || bits != 16 || channels == 0 || rate == 0 {
        return Err("Use 16-bit PCM WAV".into());
    }
    let samples: Vec<f32> = audio
        .chunks_exact(channels as usize * 2)
        .map(|v| {
            v.chunks_exact(2)
                .map(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.)
                .sum::<f32>()
                / channels as f32
        })
        .collect();
    Ok((rate, samples))
}
