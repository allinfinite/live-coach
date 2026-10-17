//! Read-only history projection. Only explicitly selected plain conversation text can enter a review.
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
pub const MAX_MESSAGES: usize = 32;
pub const MAX_BYTES: usize = 32_768;
const MAX_MESSAGE_BYTES: usize = 8_192;
#[derive(Clone, Debug, Serialize)]
pub struct HistoryThread {
    pub id: String,
    pub title: String,
    pub project: String,
    pub updated_at: i64,
}
#[derive(Clone, Debug, Serialize)]
pub struct HistoryPage {
    pub threads: Vec<HistoryThread>,
    pub next_cursor: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct HistoryMessage {
    pub id: String,
    pub role: String,
    pub text: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct HistorySnapshot {
    pub snapshot_id: String,
    pub thread_id: String,
    pub title: String,
    pub messages: Vec<HistoryMessage>,
    pub truncated: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct HistoryContext {
    pub thread_id: String,
    pub title: String,
    pub id: String,
    pub role: String,
    pub text: String,
}
pub struct History {
    listed: HashMap<String, HistoryThread>,
    snapshots: HashMap<String, (HistorySnapshot, Instant)>,
}
impl History {
    pub fn new() -> Self {
        Self {
            listed: HashMap::new(),
            snapshots: HashMap::new(),
        }
    }
    pub fn clear_snapshots(&mut self) {
        self.snapshots.clear();
    }
    pub fn clear(&mut self) {
        self.listed.clear();
        self.snapshots.clear();
    }
    pub fn remember(&mut self, page: &HistoryPage) {
        if self.listed.len() + page.threads.len() > 1000 {
            self.listed.clear();
        }
        for thread in &page.threads {
            self.listed.insert(thread.id.clone(), thread.clone());
        }
    }
    pub fn thread(&self, id: &str) -> Result<HistoryThread, String> {
        self.listed
            .get(id)
            .cloned()
            .ok_or_else(|| "Choose a conversation from the history list first.".into())
    }
    pub fn insert(&mut self, snapshot: HistorySnapshot) -> HistorySnapshot {
        self.snapshots
            .retain(|_, (_, at)| at.elapsed() < Duration::from_secs(300));
        if self.snapshots.len() >= 8 {
            self.snapshots.clear();
        }
        self.snapshots.insert(
            snapshot.snapshot_id.clone(),
            (snapshot.clone(), Instant::now()),
        );
        snapshot
    }
    pub fn select(
        &self,
        id: Option<&str>,
        items: &[String],
    ) -> Result<Vec<HistoryContext>, String> {
        if items.is_empty() {
            return Ok(vec![]);
        }
        if items.len() > MAX_MESSAGES {
            return Err("Choose at most 32 history messages.".into());
        }
        let (snapshot, at) = self
            .snapshots
            .get(id.ok_or("Choose history messages again.")?)
            .ok_or("History context expired. Choose the conversation again.")?;
        if at.elapsed() >= Duration::from_secs(300) {
            return Err("History context expired. Choose the conversation again.".into());
        }
        let selected: HashSet<&str> = items.iter().map(String::as_str).collect();
        if selected.len() != items.len()
            || selected
                .iter()
                .any(|id| !snapshot.messages.iter().any(|m| m.id == *id))
        {
            return Err("History selection changed. Choose messages again.".into());
        }
        Ok(snapshot
            .messages
            .iter()
            .filter(|m| selected.contains(m.id.as_str()))
            .map(|m| HistoryContext {
                thread_id: snapshot.thread_id.clone(),
                title: snapshot.title.clone(),
                id: m.id.clone(),
                role: m.role.clone(),
                text: m.text.clone(),
            })
            .collect())
    }
}
fn bounded(s: &str, max: usize) -> String {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_owned()
}
pub fn list_params(
    search: Option<String>,
    cursor: Option<String>,
    project: Option<String>,
) -> Result<Value, String> {
    if search.as_ref().is_some_and(|s| s.len() > 200)
        || cursor.as_ref().is_some_and(|s| s.len() > 4096)
    {
        return Err("Search or pagination value is too long.".into());
    }
    let project = project
        .map(|p| crate::project_context::project_root(&p).map(|v| v.display().to_string()))
        .transpose()?;
    Ok(
        json!({"limit":30,"searchTerm":search.filter(|s|!s.trim().is_empty()),"cursor":cursor,"cwd":project,"sortKey":"updated_at","sortDirection":"desc","modelProviders":[],"sourceKinds":["cli","vscode","exec","appServer","unknown"],"useStateDbOnly":true}),
    )
}
pub fn page(value: &Value) -> Result<HistoryPage, String> {
    let rows = value["data"]
        .as_array()
        .ok_or("Codex history list is unavailable.")?;
    let threads = rows
        .iter()
        .take(30)
        .filter_map(|r| {
            let id = r["id"].as_str()?;
            if id.is_empty() || id.len() > 200 {
                return None;
            }
            Some(HistoryThread {
                id: id.into(),
                title: bounded(
                    r["name"]
                        .as_str()
                        .filter(|s| !s.trim().is_empty())
                        .or_else(|| r["preview"].as_str())
                        .unwrap_or("Untitled conversation"),
                    240,
                ),
                project: bounded(r["cwd"].as_str().unwrap_or(""), 4096),
                updated_at: r["updatedAt"].as_i64().unwrap_or(0),
            })
        })
        .collect();
    Ok(HistoryPage {
        threads,
        next_cursor: value["nextCursor"]
            .as_str()
            .filter(|s| s.len() <= 4096)
            .map(str::to_owned),
    })
}
pub fn snapshot(thread: HistoryThread, entries: &[Value], mut truncated: bool) -> HistorySnapshot {
    let mut messages = Vec::new();
    let mut bytes = 0;
    let mut seen = HashSet::new();
    // The RPC supplies newest items first. Keep the newest bounded text, then show chronological order.
    for entry in entries {
        let item = &entry["item"];
        let Some(id) = item["id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 200)
        else {
            continue;
        };
        if !seen.insert(id.to_owned()) {
            continue;
        }
        let (role, text) = match item["type"].as_str() {
            Some("agentMessage") => ("assistant", item["text"].as_str().unwrap_or("").to_owned()),
            Some("userMessage") => {
                let text = item["content"]
                    .as_array()
                    .map(|parts| {
                        parts
                            .iter()
                            .filter(|p| p["type"] == "text")
                            .filter_map(|p| p["text"].as_str())
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                ("user", text)
            }
            _ => continue,
        };
        if text.trim().is_empty() {
            continue;
        }
        if messages.len() >= MAX_MESSAGES || bytes >= MAX_BYTES {
            truncated = true;
            break;
        }
        let limit = MAX_MESSAGE_BYTES.min(MAX_BYTES - bytes);
        let short = bounded(&text, limit);
        truncated |= short.len() != text.len();
        bytes += short.len();
        messages.push(HistoryMessage {
            id: id.into(),
            role: role.into(),
            text: short,
        });
    }
    messages.reverse();
    HistorySnapshot {
        snapshot_id: uuid::Uuid::new_v4().to_string(),
        thread_id: thread.id,
        title: thread.title,
        messages,
        truncated,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn thread() -> HistoryThread {
        HistoryThread {
            id: "thread".into(),
            title: "Synthetic fixture".into(),
            project: "/tmp/synthetic".into(),
            updated_at: 1,
        }
    }
    #[test]
    fn only_plain_messages_and_attachments_never_selected() {
        let s = snapshot(
            thread(),
            &[
                json!({"item":{"type":"agentMessage","id":"a","text":"Reply"}}),
                json!({"item":{"type":"reasoning","id":"r","text":"private reasoning"}}),
                json!({"item":{"type":"commandExecution","id":"c","aggregatedOutput":"secret output"}}),
                json!({"item":{"type":"userMessage","id":"u","content":[{"type":"text","text":"Question"},{"type":"localImage","path":"private.png"}]}}),
            ],
            false,
        );
        assert_eq!(s.messages.len(), 2);
        assert_eq!(s.messages[0].role, "user");
        let text = serde_json::to_string(&s).unwrap();
        assert!(!text.contains("private"));
        assert!(!text.contains("secret"));
    }
    #[test]
    fn bounds_utf8_and_duplicate_message_ids() {
        let rows=(0..100).map(|i|json!({"item":{"type":"agentMessage","id":i.to_string(),"text":"界".repeat(5000)}})).collect::<Vec<_>>();
        let s = snapshot(thread(), &rows, false);
        assert!(s.truncated);
        assert!(s.messages.len() <= 32);
        assert!(s.messages.iter().map(|m| m.text.len()).sum::<usize>() <= MAX_BYTES);
        let row = json!({"item":{"type":"agentMessage","id":"same","text":"x"}});
        assert_eq!(
            snapshot(thread(), &[row.clone(), row], false)
                .messages
                .len(),
            1
        );
    }
    #[test]
    fn snapshots_expire_clear_and_reject_forged_selection() {
        let mut h = History::new();
        let s = h.insert(snapshot(
            thread(),
            &[json!({"item":{"type":"agentMessage","id":"a","text":"Selected"}})],
            false,
        ));
        assert_eq!(
            h.select(Some(&s.snapshot_id), &["a".into()]).unwrap().len(),
            1
        );
        assert!(h.select(Some(&s.snapshot_id), &["forged".into()]).is_err());
        assert!(h
            .select(Some(&s.snapshot_id), &["a".into(), "a".into()])
            .is_err());
        h.snapshots.get_mut(&s.snapshot_id).unwrap().1 = Instant::now() - Duration::from_secs(301);
        assert!(h.select(Some(&s.snapshot_id), &["a".into()]).is_err());
        h.clear();
        assert!(h.select(Some(&s.snapshot_id), &["a".into()]).is_err());
    }
    #[test]
    fn list_is_metadata_only_and_search_bounded() {
        assert!(list_params(Some("x".repeat(201)), None, None).is_err());
        let params = list_params(None, None, None).unwrap();
        assert_eq!(params["useStateDbOnly"], true);
        assert_eq!(params["limit"], 30);
        let p=page(&json!({"data":[{"id":"x","name":"Title","preview":"Do not use preview when title exists","cwd":"/tmp/demo","updatedAt":123,"turns":[{"secret":"excluded"}]}],"nextCursor":"next"})).unwrap();
        assert_eq!(p.threads[0].title, "Title");
        assert_eq!(p.next_cursor.as_deref(), Some("next"));
        assert!(!serde_json::to_string(&p).unwrap().contains("secret"));
    }
}
