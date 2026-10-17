//! Local Codex app-server transport. Connecting never starts or resumes a turn.
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex as StdMutex,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{oneshot, watch, Mutex},
};

const MAX_LINE: usize = 2 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Serialize)]
pub struct CodexStatus {
    pub connected: bool,
    pub executable: Option<String>,
    pub message: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CodexEvent {
    State {
        status: CodexStatus,
    },
    Notification {
        method: String,
        params: Value,
    },
    Approval {
        id: Value,
        method: String,
        params: Value,
    },
}
pub type EventSink = Arc<dyn Fn(CodexEvent) + Send + Sync>;
pub struct ConnectOptions {
    pub executable: Option<PathBuf>,
    /// The authenticated Codex home selected by the trusted backend. Never copied.
    pub home: PathBuf,
    pub cwd: PathBuf,
    /// Trusted backend arguments only. Never forward frontend-provided flags.
    pub extra_args: Vec<String>,
}
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Accept,
    Decline,
    Cancel,
}
type Pending = HashMap<String, oneshot::Sender<Result<Value, String>>>;
struct Session {
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    pending: StdMutex<Pending>,
    approvals: StdMutex<HashMap<String, String>>,
    sequence: AtomicU64,
}
pub struct CodexClient {
    session: Mutex<Option<Arc<Session>>>,
    lifecycle: Mutex<()>,
    cancel_connect: watch::Sender<u64>,
    status: StdMutex<CodexStatus>,
    sink: EventSink,
}
impl CodexClient {
    pub fn new(sink: EventSink) -> Arc<Self> {
        Arc::new(Self {
            session: Mutex::new(None),
            lifecycle: Mutex::new(()),
            cancel_connect: watch::channel(0).0,
            sink,
            status: StdMutex::new(CodexStatus {
                connected: false,
                executable: None,
                message: "Disconnected".into(),
            }),
        })
    }
    pub fn status(&self) -> CodexStatus {
        self.status.lock().unwrap().clone()
    }
    fn set_status(&self, connected: bool, executable: Option<String>, message: &str) {
        let status = CodexStatus {
            connected,
            executable,
            message: message.into(),
        };
        *self.status.lock().unwrap() = status.clone();
        (self.sink)(CodexEvent::State { status });
    }
    pub async fn connect(self: &Arc<Self>, options: ConnectOptions) -> Result<CodexStatus, String> {
        let _guard = self.lifecycle.lock().await;
        let mut cancellation = self.cancel_connect.subscribe();
        let generation = *cancellation.borrow_and_update();
        if self.session.lock().await.is_some() {
            return Ok(self.status());
        }
        let executable = options
            .executable
            .or_else(detect_executable)
            .ok_or("Codex CLI not found. Install or open Codex first.")?;
        if !executable.is_absolute() || !executable.is_file() {
            return Err("Codex executable must be an existing absolute file path".into());
        }
        if !options.cwd.is_absolute() || !options.cwd.is_dir() || !options.home.is_absolute() {
            return Err("Codex requires an absolute project directory and isolated home".into());
        }
        if !options.home.is_dir() {
            return Err("Codex home does not exist. Open Codex and sign in first.".into());
        }
        let mut command = Command::new(&executable);
        // GUI launchers can inherit credentials for unrelated projects. Give
        // Codex only basic operating-system context and its own login home.
        command.env_clear();
        for key in [
            "HOME",
            "USER",
            "LOGNAME",
            "PATH",
            "TMPDIR",
            "LANG",
            "LC_ALL",
            "LC_CTYPE",
            "SYSTEMROOT",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command
            .args(options.extra_args)
            .arg("app-server")
            .arg("--listen")
            .arg("stdio://")
            .current_dir(options.cwd)
            .env("CODEX_HOME", options.home)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            // Never surface credentials or arbitrary stderr in app events.
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command
            .spawn()
            .map_err(|_| "Could not start Codex app-server")?;
        let stdin = child.stdin.take().ok_or("Missing Codex input")?;
        let stdout = child.stdout.take().ok_or("Missing Codex output")?;
        let session = Arc::new(Session {
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            pending: StdMutex::new(HashMap::new()),
            approvals: StdMutex::new(HashMap::new()),
            sequence: AtomicU64::new(1),
        });
        *self.session.lock().await = Some(session.clone());
        let weak_client = Arc::downgrade(self);
        let weak_session = Arc::downgrade(&session);
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                // take() bounds memory even if a broken/malicious child never emits a newline.
                let result = tokio::io::AsyncReadExt::take(&mut reader, (MAX_LINE + 1) as u64)
                    .read_until(b'\n', &mut line)
                    .await;
                match result {
                    Ok(n) if n > 0 && n <= MAX_LINE => {}
                    _ => break,
                }
                let Ok(value) = serde_json::from_slice::<Value>(&line) else {
                    break;
                };
                let (Some(client), Some(session)) = (weak_client.upgrade(), weak_session.upgrade())
                else {
                    return;
                };
                if let Err(()) = client.receive(&session, value).await {
                    break;
                }
            }
            if let Some(session) = weak_session.upgrade() {
                fail_pending(&session, "Codex connection closed");
                let mut child = session.child.lock().await;
                kill_process_group(&mut child).await;
            }
            if let Some(client) = weak_client.upgrade() {
                let mut current = client.session.lock().await;
                if current.as_ref().map(|s| Arc::as_ptr(s))
                    == weak_session.upgrade().as_ref().map(|s| Arc::as_ptr(s))
                {
                    *current = None;
                    client.set_status(false, None, "Codex connection closed");
                }
            }
        });
        let handshake = async {
            self.rpc("initialize", json!({"clientInfo":{"name":"live_coach","title":"Live Coach","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":false}})).await?;
            session.write(json!({"method":"initialized"})).await
        };
        let initialized = tokio::select! {
            biased;
            _ = cancellation.changed() => Err("Codex connection canceled".to_owned()),
            result = handshake => result,
        };
        if initialized.is_err() || *cancellation.borrow() != generation {
            self.disconnect_inner().await;
            return Err("Codex initialization stopped or failed. Reconnect after checking your Codex installation and sign-in.".into());
        }
        self.set_status(
            true,
            Some(executable.display().to_string()),
            "Connected to local Codex",
        );
        Ok(self.status())
    }
    pub async fn rpc(&self, method: &str, params: Value) -> Result<Value, String> {
        self.rpc_timeout(method, params, REQUEST_TIMEOUT).await
    }
    async fn rpc_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let session = self
            .session
            .lock()
            .await
            .clone()
            .ok_or("Connect Codex first")?;
        let id = format!(
            "live-coach-{}",
            session.sequence.fetch_add(1, Ordering::Relaxed)
        );
        let (tx, rx) = oneshot::channel();
        session.pending.lock().unwrap().insert(id.clone(), tx);
        if let Err(error) = session
            .write(json!({"id":id,"method":method,"params":params}))
            .await
        {
            session.pending.lock().unwrap().remove(&id);
            return Err(error);
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(value)) => value,
            _ => {
                session.pending.lock().unwrap().remove(&id);
                self.disconnect_inner().await;
                Err(format!(
                    "Codex {method} request timed out or disconnected; the connection was stopped"
                ))
            }
        }
    }
    async fn receive(&self, session: &Arc<Session>, message: Value) -> Result<(), ()> {
        if let Some(method) = message.get("method").and_then(Value::as_str) {
            let params = message.get("params").cloned().unwrap_or(Value::Null);
            if let Some(id) = message.get("id") {
                if !id.is_string() && !id.is_i64() && !id.is_u64() {
                    return Err(());
                }
                if matches!(
                    method,
                    "item/commandExecution/requestApproval"
                        | "item/fileChange/requestApproval"
                        | "execCommandApproval"
                        | "applyPatchApproval"
                ) {
                    let key = id.to_string();
                    let mut approvals = session.approvals.lock().unwrap();
                    if approvals.len() >= 32 || approvals.contains_key(&key) {
                        return Err(());
                    }
                    approvals.insert(key, method.into());
                    drop(approvals);
                    (self.sink)(CodexEvent::Approval {
                        id: id.clone(),
                        method: method.into(),
                        params,
                    });
                } else if method == "item/permissions/requestApproval" {
                    session.write(json!({"id":id,"result":{"permissions":{},"scope":"turn","strictAutoReview":true}})).await.map_err(|_| ())?;
                } else {
                    session.write(json!({"id":id,"error":{"code":-32601,"message":"Live Coach does not permit this server request"}})).await.map_err(|_| ())?;
                }
            } else {
                if method == "serverRequest/resolved" {
                    if let Some(id) = params.get("requestId") {
                        session.approvals.lock().unwrap().remove(&id.to_string());
                    }
                }
                if method == "turn/completed" {
                    session.approvals.lock().unwrap().clear();
                }
                (self.sink)(CodexEvent::Notification {
                    method: method.into(),
                    params,
                });
            }
        } else if let Some(id) = message.get("id").and_then(Value::as_str) {
            if let Some(tx) = session.pending.lock().unwrap().remove(id) {
                // The server's arbitrary error text may contain secrets or input; expose only its code.
                let result = if message.get("error").is_some() {
                    Err(format!(
                        "Codex request failed (code {})",
                        message["error"]["code"]
                    ))
                } else {
                    message
                        .get("result")
                        .cloned()
                        .ok_or_else(|| "Malformed Codex response".into())
                };
                let _ = tx.send(result);
            }
        } else {
            return Err(());
        }
        Ok(())
    }
    pub async fn reply_to_approval(
        &self,
        id: Value,
        decision: ApprovalDecision,
    ) -> Result<(), String> {
        let session = self
            .session
            .lock()
            .await
            .clone()
            .ok_or("Codex disconnected")?;
        let method = session
            .approvals
            .lock()
            .unwrap()
            .remove(&id.to_string())
            .ok_or("This approval has expired or was already answered")?;
        let value = approval_result(&method, decision);
        session.write(json!({"id":id,"result":value})).await
    }
    pub async fn cancel_turn(&self, thread_id: &str, turn_id: &str) -> Result<Value, String> {
        self.rpc(
            "turn/interrupt",
            json!({"threadId":thread_id,"turnId":turn_id}),
        )
        .await
    }
    pub async fn shutdown(&self) {
        // Wake a blocked initialize before waiting for its lifecycle lock.
        self.cancel_connect
            .send_modify(|generation| *generation = generation.wrapping_add(1));
        let _guard = self.lifecycle.lock().await;
        self.disconnect_inner().await;
    }
    async fn disconnect_inner(&self) {
        if let Some(session) = self.session.lock().await.take() {
            fail_pending(&session, "Codex disconnected");
            let mut child = session.child.lock().await;
            kill_process_group(&mut child).await;
        }
        self.set_status(false, None, "Disconnected");
    }
}
impl Session {
    async fn write(&self, message: Value) -> Result<(), String> {
        let mut bytes = serde_json::to_vec(&message).map_err(|_| "Invalid Codex request")?;
        if bytes.len() > MAX_LINE {
            return Err("Codex request exceeds size limit".into());
        }
        bytes.push(b'\n');
        let mut stdin = self.stdin.lock().await;
        tokio::time::timeout(Duration::from_secs(5), async {
            stdin.write_all(&bytes).await?;
            stdin.flush().await
        })
        .await
        .map_err(|_| "Codex input stalled")?
        .map_err(|_| "Codex input closed".into())
    }
}
async fn kill_process_group(child: &mut Child) {
    // Codex may have an active command. Keep its descendants in our own process
    // group so disconnect/cancellation cannot leave an unattended command behind.
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        // The child is spawned as leader of a fresh group; negative pid targets
        // only that group, never Live Coach or the user's other Codex sessions.
        unsafe {
            kill(-(pid as i32), 9);
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}
fn fail_pending(session: &Session, reason: &str) {
    for (_, tx) in session.pending.lock().unwrap().drain() {
        let _ = tx.send(Err(reason.into()));
    }
    session.approvals.lock().unwrap().clear();
}
fn approval_result(method: &str, decision: ApprovalDecision) -> Value {
    if matches!(method, "execCommandApproval" | "applyPatchApproval") {
        match decision {
            ApprovalDecision::Accept => json!({"decision":"approved"}),
            ApprovalDecision::Decline => {
                json!({"decision":{"denied":{"rejection":"Declined in Live Coach"}}})
            }
            ApprovalDecision::Cancel => json!({"decision":"abort"}),
        }
    } else {
        json!({"decision": match decision { ApprovalDecision::Accept => "accept", ApprovalDecision::Decline => "decline", ApprovalDecision::Cancel => "cancel" }})
    }
}
pub fn detect_executable() -> Option<PathBuf> {
    discovery_candidates(
        dirs::home_dir().as_deref(),
        std::env::var_os("PATH").as_deref(),
    )
    .into_iter()
    .find_map(|path| native_executable(&path))
}
fn discovery_candidates(
    home: Option<&Path>,
    search_path: Option<&std::ffi::OsStr>,
) -> Vec<PathBuf> {
    // Native app bundles work from Finder even when its PATH has no Node.js.
    // Prefer these over npm's JavaScript launcher and never search relative
    // PATH entries (which could resolve inside an untrusted selected project).
    let bundle_suffixes = [
        "ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex",
        "Codex.app/Contents/Resources/codex",
        "Codex.app/Contents/MacOS/codex",
    ];
    let mut paths = bundle_suffixes
        .iter()
        .map(|p| Path::new("/Applications").join(p))
        .collect::<Vec<_>>();
    if let Some(home) = home {
        paths.extend(
            bundle_suffixes
                .iter()
                .map(|p| home.join("Applications").join(p)),
        );
    }
    if let Some(path) = search_path {
        paths.extend(
            std::env::split_paths(path)
                .filter(|p| p.is_absolute())
                .map(|p| p.join("codex")),
        );
    }
    paths.extend(["/opt/homebrew/bin/codex", "/usr/local/bin/codex"].map(PathBuf::from));
    if let Some(home) = home {
        paths.push(home.join(".local/bin/codex"));
    }
    paths
}
fn native_executable(path: &Path) -> Option<PathBuf> {
    let path = std::fs::canonicalize(path).ok()?;
    if !is_executable(&path) {
        return None;
    }
    if has_native_header(&path) {
        return Some(path);
    }
    // npm installations may put the platform executable inside the main
    // package or an adjacent optional platform package. Never invoke a JS
    // wrapper whose /usr/bin/env node dependency is absent in a GUI session.
    let package = path.parent()?.parent()?;
    let (target, platform) = if cfg!(target_arch = "aarch64") {
        ("aarch64-apple-darwin", "codex-darwin-arm64")
    } else {
        ("x86_64-apple-darwin", "codex-darwin-x64")
    };
    let vendor = Path::new("vendor").join(target).join("codex/codex");
    let candidates = [
        package.join(&vendor),
        package.parent()?.join(platform).join(&vendor),
        package
            .join("node_modules/@openai")
            .join(platform)
            .join(&vendor),
    ];
    candidates
        .into_iter()
        .find(|p| is_executable(p) && has_native_header(p))
        .and_then(|p| std::fs::canonicalize(p).ok())
}
fn has_native_header(path: &Path) -> bool {
    use std::io::Read;
    let mut header = [0u8; 4];
    if std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut header))
        .is_err()
    {
        return false;
    }
    matches!(
        header,
        [0xcf, 0xfa, 0xed, 0xfe]
            | [0xfe, 0xed, 0xfa, 0xcf]
            | [0xce, 0xfa, 0xed, 0xfe]
            | [0xfe, 0xed, 0xfa, 0xce]
            | [0xca, 0xfe, 0xba, 0xbe]
            | [0xbe, 0xba, 0xfe, 0xca]
            | [0xca, 0xfe, 0xba, 0xbf]
            | [0xbf, 0xba, 0xfe, 0xca]
            | [0x7f, b'E', b'L', b'F']
    )
}
fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

pub fn discover() -> Result<PathBuf, String> {
    detect_executable().ok_or_else(|| "Codex CLI not found. Install or open Codex first.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    struct Fixture {
        root: PathBuf,
        executable: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("live-coach-codex-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            let executable = root.join("fake-codex");
            std::fs::write(&executable, r#"#!/usr/bin/python3
import sys,json,os,subprocess,time
for line in sys.stdin:
 r=json.loads(line)
 if 'method' not in r: continue
 m=r['method']
 if m=='initialize' and os.path.exists('hang-initialize'):
  open('handshake-pid','w').write(str(os.getpid()))
  time.sleep(60)
 if m=='initialized': continue
 if m=='hang': continue
 if m=='environment':
  print(json.dumps({'id':r['id'],'result':{'names':sorted(os.environ)}}),flush=True);continue
 if m=='spawn':
  child=subprocess.Popen(['/bin/sleep','60'])
  print(json.dumps({'id':r['id'],'result':{'pid':child.pid}}),flush=True);continue
 if m=='oversize': print('x'*2200000,flush=True);continue
 if m=='malformed': print('not json',flush=True);continue
 if m=='approval':
  print(json.dumps({'id':17,'method':'item/commandExecution/requestApproval','params':{'command':'echo reviewed','threadId':'thread','turnId':'turn'}}),flush=True)
  response=json.loads(sys.stdin.readline())
  print(json.dumps({'id':r['id'],'result':response['result']}),flush=True);continue
 if m=='unknown':
  print(json.dumps({'id':'secret-request','method':'unknown/action','params':{}}),flush=True)
  response=json.loads(sys.stdin.readline())
  print(json.dumps({'id':r['id'],'result':response}),flush=True);continue
 if m=='permission':
  print(json.dumps({'id':18,'method':'item/permissions/requestApproval','params':{'permissions':{'network':{'enabled':True}}}}),flush=True)
  response=json.loads(sys.stdin.readline())
  print(json.dumps({'id':r['id'],'result':response['result']}),flush=True);continue
 if m=='event':print(json.dumps({'method':'item/agentMessage/delta','params':{'delta':'hello'}}),flush=True)
 print(json.dumps({'id':r['id'],'result':{'method':m,'pid':os.getpid(),'params':r.get('params')}}),flush=True)
"#).unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self { root, executable }
        }
        fn options(&self) -> ConnectOptions {
            ConnectOptions {
                executable: Some(self.executable.clone()),
                home: self.root.clone(),
                cwd: self.root.clone(),
                extra_args: vec![],
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    fn client() -> (
        Arc<CodexClient>,
        tokio::sync::mpsc::UnboundedReceiver<CodexEvent>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (
            CodexClient::new(Arc::new(move |event| {
                let _ = tx.send(event);
            })),
            rx,
        )
    }
    #[test]
    fn discovery_prefers_bundles_and_ignores_relative_search_paths() {
        let paths = discovery_candidates(
            Some(Path::new("/home/fixture")),
            Some(std::ffi::OsStr::new(
                ".:relative/bin:/opt/fixture/bin::/usr/bin",
            )),
        );
        assert_eq!(paths[0], Path::new("/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex"));
        assert!(paths.contains(&PathBuf::from(
            "/home/fixture/Applications/Codex.app/Contents/Resources/codex"
        )));
        assert!(paths.contains(&PathBuf::from("/opt/fixture/bin/codex")));
        assert!(paths.iter().all(|p| p.is_absolute()));
        assert!(!paths
            .iter()
            .any(|p| p == Path::new("relative/bin/codex") || p == Path::new("./codex")));
    }
    #[test]
    fn npm_launcher_is_resolved_to_native_vendor_without_node() {
        let fixture = Fixture::new();
        let package = fixture.root.join("node_modules/@openai/codex");
        std::fs::create_dir_all(package.join("bin")).unwrap();
        let wrapper = package.join("bin/codex.js");
        std::fs::write(&wrapper, "#!/usr/bin/env node\n").unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(native_executable(&wrapper).is_none());
        let target = if cfg!(target_arch = "aarch64") {
            "aarch64-apple-darwin"
        } else {
            "x86_64-apple-darwin"
        };
        let native = package.join("vendor").join(target).join("codex/codex");
        std::fs::create_dir_all(native.parent().unwrap()).unwrap();
        std::fs::write(&native, [0xcf, 0xfa, 0xed, 0xfe]).unwrap();
        std::fs::set_permissions(&native, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            native_executable(&wrapper),
            Some(native.canonicalize().unwrap())
        );
    }
    #[tokio::test]
    async fn shutdown_interrupts_hung_initialization_and_reaps_child() {
        let fixture = Fixture::new();
        std::fs::write(fixture.root.join("hang-initialize"), "fixture").unwrap();
        let (client, _) = client();
        let connecting = client.clone();
        let options = fixture.options();
        let job = tokio::spawn(async move { connecting.connect(options).await });
        let marker = fixture.root.join("handshake-pid");
        tokio::time::timeout(Duration::from_secs(3), async {
            while !marker.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let pid = std::fs::read_to_string(marker).unwrap();
        tokio::time::timeout(Duration::from_secs(1), client.shutdown())
            .await
            .unwrap();
        assert!(job.await.unwrap().is_err());
        assert!(!client.status().connected);
        let exit = Command::new("/bin/kill")
            .args(["-0", &pid])
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .unwrap();
        assert!(!exit.success(), "hung app-server child must be reaped");
        // A new explicit connection uses a fresh cancellation generation.
        std::fs::remove_file(fixture.root.join("hang-initialize")).unwrap();
        client.connect(fixture.options()).await.unwrap();
        assert!(client.status().connected);
        client.shutdown().await;
    }
    #[tokio::test]
    async fn explicit_connect_handshake_events_and_cleanup() {
        let fixture = Fixture::new();
        let (client, mut events) = client();
        assert!(!client.status().connected);
        assert!(client.session.lock().await.is_none());
        client.connect(fixture.options()).await.unwrap();
        let result = client.rpc("event", json!({"test":true})).await.unwrap();
        assert_eq!(result["params"]["test"], true);
        let pid = result["pid"].as_u64().unwrap();
        assert!(matches!(
            events.recv().await,
            Some(CodexEvent::State { .. })
        ));
        assert!(
            matches!(events.recv().await, Some(CodexEvent::Notification { method, .. }) if method == "item/agentMessage/delta")
        );
        client.shutdown().await;
        assert!(!client.status().connected);
        let exit = Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .unwrap();
        assert!(!exit.success(), "app-server child must be reaped");
    }
    #[tokio::test]
    async fn disconnect_stops_command_descendants_in_own_group() {
        let fixture = Fixture::new();
        let (client, _) = client();
        client.connect(fixture.options()).await.unwrap();
        let pid = client.rpc("spawn", json!({})).await.unwrap()["pid"]
            .as_u64()
            .unwrap();
        client.shutdown().await;
        let output = Command::new("/bin/ps")
            .args(["-o", "state=", "-p", &pid.to_string()])
            .output()
            .await
            .unwrap();
        let state = String::from_utf8_lossy(&output.stdout);
        assert!(
            state.trim().is_empty() || state.trim().starts_with('Z'),
            "command descendant still running: {state}"
        );
    }
    #[tokio::test]
    async fn unrelated_credentials_and_agent_sockets_are_not_inherited() {
        // Run the test in a fresh process with synthetic credentials. This
        // avoids changing any real environment value in parallel Rust tests.
        const CHILD: &str = "LIVE_COACH_CODEX_ENV_TEST_CHILD";
        let names = [
            "GITHUB_TOKEN",
            "OPENAI_API_KEY",
            "SSH_AUTH_SOCK",
            "LIVE_COACH_ENV_LEAK_TEST",
        ];
        if std::env::var_os(CHILD).is_none() {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "codex::tests::unrelated_credentials_and_agent_sockets_are_not_inherited",
                ])
                .env(CHILD, "1");
            for name in names {
                command.env(name, "synthetic-transport-test-only");
            }
            let result = command.output().await.unwrap();
            assert!(
                result.status.success(),
                "Environment isolation subprocess test failed"
            );
            return;
        }
        let fixture = Fixture::new();
        let (client, _) = client();
        client.connect(fixture.options()).await.unwrap();
        let result = client.rpc("environment", json!({})).await.unwrap();
        let inherited = result["names"].as_array().unwrap();
        for name in names {
            assert!(
                !inherited.contains(&json!(name)),
                "Unrelated environment variable was forwarded: {name}"
            );
        }
        assert!(inherited.contains(&json!("CODEX_HOME")));
        if std::env::var_os("HOME").is_some() {
            assert!(inherited.contains(&json!("HOME")));
        }
        client.shutdown().await;
    }
    #[tokio::test]
    async fn approval_waits_for_exact_user_decision_and_is_one_use() {
        let fixture = Fixture::new();
        let (client, mut events) = client();
        client.connect(fixture.options()).await.unwrap();
        let runner = client.clone();
        let task = tokio::spawn(async move { runner.rpc("approval", json!({})).await });
        loop {
            if let Some(CodexEvent::Approval { id, method, .. }) = events.recv().await {
                assert_eq!(method, "item/commandExecution/requestApproval");
                assert!(!task.is_finished());
                client
                    .reply_to_approval(id.clone(), ApprovalDecision::Accept)
                    .await
                    .unwrap();
                assert!(client
                    .reply_to_approval(id, ApprovalDecision::Accept)
                    .await
                    .is_err());
                break;
            }
        }
        assert_eq!(task.await.unwrap().unwrap()["decision"], "accept");
        client.shutdown().await;
    }
    #[tokio::test]
    async fn unsupported_requests_and_permissions_fail_closed() {
        let fixture = Fixture::new();
        let (client, _) = client();
        client.connect(fixture.options()).await.unwrap();
        assert_eq!(
            client.rpc("unknown", json!({})).await.unwrap()["error"]["code"],
            -32601
        );
        let permission = client.rpc("permission", json!({})).await.unwrap();
        assert_eq!(permission["permissions"], json!({}));
        assert_eq!(permission["strictAutoReview"], true);
        client.shutdown().await;
    }
    #[tokio::test]
    async fn request_timeout_removes_waiter_and_disconnect_is_prompt() {
        let fixture = Fixture::new();
        let (client, _) = client();
        client.connect(fixture.options()).await.unwrap();
        assert!(client
            .rpc_timeout("hang", json!({}), Duration::from_millis(40))
            .await
            .is_err());
        assert!(client.session.lock().await.is_none());
        tokio::time::timeout(Duration::from_secs(2), client.shutdown())
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn oversized_and_malformed_child_output_disconnect() {
        for method in ["oversize", "malformed"] {
            let fixture = Fixture::new();
            let (client, _) = client();
            client.connect(fixture.options()).await.unwrap();
            assert!(client
                .rpc_timeout(method, json!({}), Duration::from_secs(3))
                .await
                .is_err());
            client.shutdown().await;
        }
    }
    #[test]
    fn decisions_never_grant_session_or_persistent_permission() {
        for method in [
            "item/commandExecution/requestApproval",
            "item/fileChange/requestApproval",
        ] {
            assert_eq!(
                approval_result(method, ApprovalDecision::Accept),
                json!({"decision":"accept"})
            );
            assert_eq!(
                approval_result(method, ApprovalDecision::Decline),
                json!({"decision":"decline"})
            );
            assert_eq!(
                approval_result(method, ApprovalDecision::Cancel),
                json!({"decision":"cancel"})
            );
        }
        assert_eq!(
            approval_result("execCommandApproval", ApprovalDecision::Accept)["decision"],
            "approved"
        );
    }
}
