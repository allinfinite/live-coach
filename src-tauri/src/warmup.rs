//! Load an already-installed local model without audio, transcript, or credentials.
use serde_json::{json, Value};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{watch, Mutex},
    task::JoinHandle,
};

pub const KEEP_ALIVE: &str = "30m";
const CACHE_FOR: Duration = Duration::from_secs(10 * 60);
const MAX_RESPONSE: usize = 1024 * 1024;
type Outcome = Result<(), String>;

struct Pending {
    model: String,
    result: watch::Sender<Option<Outcome>>,
    task: JoinHandle<()>,
}
struct State {
    generation: u64,
    pending: Option<Pending>,
    ready: Option<(String, Instant)>,
}
pub struct Warmup {
    state: Mutex<State>,
    client: reqwest::Client,
    base_url: String,
}
impl Warmup {
    pub fn new() -> Arc<Self> {
        Self::with_url("http://127.0.0.1:11434".into())
    }
    fn with_url(base_url: String) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                generation: 0,
                pending: None,
                ready: None,
            }),
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(120))
                .build()
                .expect("local warmup HTTP client"),
            base_url,
        })
    }
    pub async fn warm(self: &Arc<Self>, model: String) -> Outcome {
        let model = model.trim().to_string();
        let mut state = self.state.lock().await;
        if model.is_empty()
            || model.len() > 200
            || model.chars().any(char::is_control)
            || is_cloud(&model)
        {
            cancel_pending(&mut state);
            return Err("Choose an installed local Ollama model to warm up.".into());
        }
        let mut receiver = if let Some(pending) = &state.pending {
            if canonical(&pending.model) == canonical(&model) {
                Some(pending.result.subscribe())
            } else {
                None
            }
        } else {
            None
        };
        if receiver.is_none() {
            if state.ready.as_ref().is_some_and(|(name, at)| {
                canonical(name) == canonical(&model) && at.elapsed() < CACHE_FOR
            }) {
                return Ok(());
            }
            cancel_pending(&mut state);
            let generation = state.generation;
            let (sender, rx) = watch::channel(None);
            let worker = Arc::clone(self);
            let task_model = model.clone();
            let task_sender = sender.clone();
            let task = tokio::spawn(async move {
                let result = worker.load(&task_model).await;
                let mut state = worker.state.lock().await;
                if state.generation != generation {
                    return;
                }
                if result.is_ok() {
                    state.ready = Some((task_model, Instant::now()));
                }
                state.pending = None;
                let _ = task_sender.send(Some(result));
            });
            state.pending = Some(Pending {
                model,
                result: sender,
                task,
            });
            receiver = Some(rx);
        }
        drop(state);
        let mut receiver = receiver.expect("warmup receiver");
        loop {
            if let Some(result) = receiver.borrow().clone() {
                return result;
            }
            receiver
                .changed()
                .await
                .map_err(|_| "Local model warmup was canceled.".to_string())?;
        }
    }
    pub async fn cancel(&self) {
        cancel_pending(&mut *self.state.lock().await);
    }
    async fn load(&self, model: &str) -> Outcome {
        let response = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map_err(|_| "Ollama is unavailable. Start Ollama and retry.".to_string())?;
        let tags = bounded_json(response).await?;
        let requested = canonical(model);
        let entry = tags.get("models").and_then(Value::as_array)
            .and_then(|models| models.iter().find(|entry| {
                ["name", "model"].iter().any(|key| entry.get(key).and_then(Value::as_str)
                    .is_some_and(|name| canonical(name) == requested))
            }))
            .ok_or_else(|| format!("Ollama model '{model}' is not installed. Choose an installed model; no model was downloaded."))?;
        if ["name", "model"]
            .iter()
            .any(|key| entry.get(key).and_then(Value::as_str).is_some_and(is_cloud))
            || ["remote_model", "remote_host"].iter().any(|key| {
                entry
                    .get(key)
                    .is_some_and(|v| !v.is_null() && v.as_str() != Some(""))
            })
        {
            return Err("Cloud-backed Ollama models cannot be warmed locally.".into());
        }
        let response = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&json!({"model": model, "prompt": "", "stream": false, "keep_alive": KEEP_ALIVE}))
            .send()
            .await
            .map_err(|_| "Local model warmup failed. Retry when Ollama is ready.".to_string())?;
        let body = bounded_json(response).await?;
        if body.get("done").and_then(Value::as_bool) != Some(true) || body.get("error").is_some() {
            return Err("Ollama did not finish loading the local model. Retry warmup.".into());
        }
        Ok(())
    }
}
fn cancel_pending(state: &mut State) {
    state.generation = state.generation.wrapping_add(1);
    state.ready = None;
    if let Some(pending) = state.pending.take() {
        let _ = pending
            .result
            .send(Some(Err("Local model warmup was canceled.".into())));
        pending.task.abort();
    }
}
fn canonical(model: &str) -> String {
    if model.rsplit('/').next().unwrap_or(model).contains(':') {
        model.into()
    } else {
        format!("{model}:latest")
    }
}
fn is_cloud(model: &str) -> bool {
    let name = model.to_ascii_lowercase();
    name.ends_with(":cloud")
        || name.ends_with("-cloud")
        || name
            .split(':')
            .next()
            .is_some_and(|n| n.ends_with("-cloud"))
}
async fn bounded_json(mut response: reqwest::Response) -> Result<Value, String> {
    if !response.status().is_success() {
        return Err(format!(
            "Ollama warmup returned HTTP {}. Retry when Ollama is ready.",
            response.status()
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Could not read Ollama warmup response.".to_string())?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
            return Err("Ollama warmup response exceeded the size limit.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "Ollama returned an invalid warmup response.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex as StdMutex,
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    struct Server {
        url: String,
        requests: Arc<StdMutex<Vec<(String, Value)>>>,
        task: JoinHandle<()>,
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.task.abort();
        }
    }
    async fn server(tags: Value, delay: Duration, fail_first: bool) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(StdMutex::new(Vec::new()));
        let captured = requests.clone();
        let generations = Arc::new(AtomicUsize::new(0));
        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let captured = captured.clone();
                let tags = tags.clone();
                let generations = generations.clone();
                tokio::spawn(async move {
                    let mut bytes = Vec::new();
                    let (end, length) = loop {
                        let mut buf = [0; 4096];
                        let n = socket.read(&mut buf).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        bytes.extend_from_slice(&buf[..n]);
                        let text = String::from_utf8_lossy(&bytes);
                        if let Some(end) = text.find("\r\n\r\n") {
                            let length = text[..end]
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .and_then(|n| n.trim().parse::<usize>().ok())
                                })
                                .unwrap_or(0);
                            if bytes.len() >= end + 4 + length {
                                break (end, length);
                            }
                        }
                    };
                    let path = String::from_utf8_lossy(&bytes[..end])
                        .lines()
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .nth(1)
                        .unwrap()
                        .to_string();
                    let body = if length == 0 {
                        Value::Null
                    } else {
                        serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap()
                    };
                    captured.lock().unwrap().push((path.clone(), body));
                    let (status, body) = if path == "/api/tags" {
                        ("200 OK", tags)
                    } else {
                        let index = generations.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(delay).await;
                        if fail_first && index == 0 {
                            ("500 Error", json!({"error":"failed"}))
                        } else {
                            ("200 OK", json!({"done":true}))
                        }
                    };
                    let body = body.to_string();
                    let response = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}", body.len());
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        });
        Server {
            url,
            requests,
            task,
        }
    }
    fn tags() -> Value {
        json!({"models":[{"name":"test:latest"},{"name":"other:latest"}]})
    }
    async fn wait_generate(server: &Server) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if server
                    .requests
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|(path, _)| path == "/api/generate")
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn empty_local_payload_coalesces_and_caches() {
        let server = server(tags(), Duration::from_millis(30), false).await;
        let warmup = Warmup::with_url(server.url.clone());
        let (a, b) = tokio::join!(warmup.warm("test".into()), warmup.warm("test".into()));
        assert!(a.is_ok() && b.is_ok());
        warmup.warm("test".into()).await.unwrap();
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].0, "/api/tags");
        assert_eq!(
            requests[1],
            (
                "/api/generate".into(),
                json!({"model":"test", "prompt":"", "stream":false, "keep_alive":"30m"})
            )
        );
    }
    #[tokio::test]
    async fn missing_and_remote_models_never_generate() {
        for tags in [
            json!({"models":[]}),
            json!({"models":[{"name":"test:latest", "remote_model":"remote"}]}),
            json!({"models":[{"name":"test:latest", "remote_host":"https://example.com"}]}),
        ] {
            let server = server(tags, Duration::ZERO, false).await;
            let warmup = Warmup::with_url(server.url.clone());
            assert!(warmup.warm("test".into()).await.is_err());
            assert_eq!(server.requests.lock().unwrap().len(), 1);
        }
        let server = server(tags(), Duration::ZERO, false).await;
        let warmup = Warmup::with_url(server.url.clone());
        for model in ["", "gpt:cloud", "gpt-cloud", "gpt-cloud:latest"] {
            assert!(warmup.warm(model.into()).await.is_err());
        }
        assert!(server.requests.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn failures_retry_and_offline_is_actionable() {
        let server = server(tags(), Duration::ZERO, true).await;
        let warmup = Warmup::with_url(server.url.clone());
        assert!(warmup.warm("test".into()).await.is_err());
        warmup.warm("test".into()).await.unwrap();
        assert_eq!(server.requests.lock().unwrap().len(), 4);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let error = Warmup::with_url(url).warm("test".into()).await.unwrap_err();
        assert!(error.contains("Start Ollama"));
    }
    #[tokio::test]
    async fn model_change_and_cancel_do_not_cache_stale_completion() {
        let server = server(tags(), Duration::from_millis(150), false).await;
        let warmup = Warmup::with_url(server.url.clone());
        let old = warmup.clone();
        let first = tokio::spawn(async move { old.warm("test".into()).await });
        wait_generate(&server).await;
        warmup.warm("other".into()).await.unwrap();
        assert!(first.await.unwrap().is_err());
        let old = warmup.clone();
        let canceled = tokio::spawn(async move { old.warm("test".into()).await });
        tokio::time::sleep(Duration::from_millis(30)).await;
        warmup.cancel().await;
        assert!(canceled.await.unwrap().is_err());
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(warmup.state.lock().await.ready.is_none());
        warmup.warm("test".into()).await.unwrap();
        assert_eq!(warmup.state.lock().await.ready.as_ref().unwrap().0, "test");
    }
    #[tokio::test]
    async fn expired_cache_refreshes() {
        let server = server(tags(), Duration::ZERO, false).await;
        let warmup = Warmup::with_url(server.url.clone());
        warmup.warm("test".into()).await.unwrap();
        warmup.state.lock().await.ready.as_mut().unwrap().1 = Instant::now() - CACHE_FOR;
        warmup.warm("test".into()).await.unwrap();
        assert_eq!(server.requests.lock().unwrap().len(), 4);
    }
}
