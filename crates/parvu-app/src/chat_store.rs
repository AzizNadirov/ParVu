//! The assistant's conversations saved per data file (`~/.ParVu/ai/chats`).
//!
//! One JSON file per source: a file tab's canonical path, plus the sheet
//! for Excel. Each holds the model history (what goes back to the provider
//! with the next question) and the panel's transcript as the UI built it
//! (text, tool calls with their input and result, errors, usage). The
//! transcript is kept as the UI's own JSON rather than rebuilt from the
//! history: the history lacks what the panel shows (tool result previews,
//! errors, stopped answers, costs) and carries what it doesn't (the table
//! context sent with each question).
//!
//! Files are written atomically (a uniquely named temporary file, then a
//! rename), so two windows on the same file can only overwrite each other,
//! never leave a broken file. A file that doesn't parse, or comes from a
//! newer ParVu, is ignored.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use parvu_ai::types::{Block, Role};
use parvu_ai::Message;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Version of the file format written.
pub const SCHEMA_VERSION: u32 = 1;
/// Questions kept per file; older ones are dropped whole.
pub const MAX_TURNS: usize = 50;
/// About the largest file written; older questions are dropped to fit.
pub const MAX_BYTES: usize = 1_000_000;

/// What a conversation belongs to: a data file, and the sheet for Excel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatKey {
    pub path: PathBuf,
    pub sheet: Option<String>,
}

impl ChatKey {
    /// The key of `path` (made absolute and canonical when it exists).
    pub fn new(path: &Path, sheet: Option<&str>) -> Self {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()));
        ChatKey { path, sheet: sheet.map(str::to_string) }
    }

    /// The file name in the chats folder: a stable hash of path and sheet.
    pub fn file_name(&self) -> String {
        let mut text = self.path.to_string_lossy().into_owned();
        if let Some(s) = &self.sheet {
            text.push('\0');
            text.push_str(s);
        }
        format!("{:016x}.json", fnv1a(text.as_bytes()))
    }

    fn matches(&self, c: &SavedChat) -> bool {
        c.path == self.path.to_string_lossy() && c.sheet == self.sheet
    }
}

/// FNV-1a, 64 bits: stable across Rust versions (unlike `DefaultHasher`).
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// One saved conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedChat {
    pub version: u32,
    /// The data file, so the folder can be listed without the hash.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<String>,
    /// Milliseconds since the Unix epoch.
    pub updated_ms: i64,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// The conversation as sent to the model.
    #[serde(default)]
    pub history: Vec<Message>,
    /// The panel's transcript (`{ items, usage, cost }`), as the UI sent it.
    #[serde(default)]
    pub display: Value,
}

impl SavedChat {
    pub fn new(key: &ChatKey, history: Vec<Message>, display: Value, provider: Option<String>, model: Option<String>) -> Self {
        SavedChat {
            version: SCHEMA_VERSION,
            path: key.path.to_string_lossy().into_owned(),
            sheet: key.sheet.clone(),
            updated_ms: chrono::Utc::now().timestamp_millis(),
            provider,
            model,
            history,
            display,
        }
    }
}

/// The folder of saved conversations.
#[derive(Debug, Clone)]
pub struct ChatStore {
    dir: PathBuf,
}

impl ChatStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        ChatStore { dir: dir.into() }
    }

    fn file(&self, key: &ChatKey) -> PathBuf {
        self.dir.join(key.file_name())
    }

    /// The saved conversation of `key`, repaired so it can be sent again.
    /// `None` when there is none, or the file is unreadable, broken or from
    /// a newer ParVu (logged).
    pub fn load(&self, key: &ChatKey) -> Option<SavedChat> {
        let path = self.file(key);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                tracing::warn!("saved AI chat {}: {e}", path.display());
                return None;
            }
        };
        let mut chat = match parse(&text) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("saved AI chat {} ignored: {e}", path.display());
                return None;
            }
        };
        if !key.matches(&chat) {
            // A hash collision: someone else's conversation.
            return None;
        }
        repair_history(&mut chat.history);
        Some(chat)
    }

    /// Save `chat` (capped to [`MAX_TURNS`] and about [`MAX_BYTES`]),
    /// replacing the file atomically.
    pub fn save(&self, key: &ChatKey, mut chat: SavedChat) -> std::io::Result<()> {
        repair_history(&mut chat.history);
        cap(&mut chat, MAX_TURNS, MAX_BYTES);
        let text = serde_json::to_string(&chat).map_err(std::io::Error::other)?;
        write_atomic(&self.file(key), &text)
    }

    /// Forget the conversation of `key`. True if there was one.
    pub fn delete(&self, key: &ChatKey) -> bool {
        std::fs::remove_file(self.file(key)).is_ok()
    }

    fn files(&self) -> Vec<PathBuf> {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return Vec::new() };
        rd.filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json") && p.is_file())
            .collect()
    }

    /// How many conversations are saved.
    pub fn count(&self) -> usize {
        self.files().len()
    }

    /// Delete every saved conversation; returns how many were deleted.
    pub fn delete_all(&self) -> usize {
        self.files().into_iter().filter(|p| std::fs::remove_file(p).is_ok()).count()
    }
}

/// Parse a saved file, refusing one written by a newer ParVu.
fn parse(text: &str) -> Result<SavedChat, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let version = v.get("version").and_then(Value::as_u64).ok_or("no version")?;
    if version > u64::from(SCHEMA_VERSION) {
        return Err(format!("written by a newer ParVu (format {version})"));
    }
    serde_json::from_value(v).map_err(|e| e.to_string())
}

fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}-{}.tmp", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
    let tmp = PathBuf::from(tmp);
    if let Err(e) = std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, path)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

// ---------------------------------------------------------------- repair and cap

fn tool_use_ids(m: &Message) -> Vec<&str> {
    m.content
        .iter()
        .filter_map(|b| match b {
            Block::ToolUse { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect()
}

fn tool_result_ids(m: &Message) -> Vec<&str> {
    m.content
        .iter()
        .filter_map(|b| match b {
            Block::ToolResult { tool_use_id, .. } => Some(tool_use_id.as_str()),
            _ => None,
        })
        .collect()
}

/// Make a history safe to send again. Providers reject a tool call without
/// its result (and a result without its call): the history is cut at the
/// first such call. An answer cut off mid tool call keeps its text, without
/// the call. It then ends with an answer, never with an unanswered question.
pub fn repair_history(history: &mut Vec<Message>) {
    let mut i = 0;
    let mut cut: Option<(usize, bool)> = None; // (index, strip its tool calls and keep it)
    while i < history.len() {
        let m = &history[i];
        match m.role {
            Role::User => {
                if !tool_result_ids(m).is_empty() {
                    // A result whose call isn't right before it.
                    cut = Some((i, false));
                    break;
                }
                i += 1;
            }
            Role::Assistant => {
                let uses: HashSet<&str> = tool_use_ids(m).into_iter().collect();
                if uses.is_empty() {
                    i += 1;
                    continue;
                }
                let answered = history.get(i + 1).is_some_and(|next| {
                    next.role == Role::User && tool_result_ids(next).into_iter().collect::<HashSet<_>>() == uses
                });
                if !answered {
                    cut = Some((i, true));
                    break;
                }
                i += 2;
            }
        }
    }
    if let Some((at, keep)) = cut {
        history.truncate(if keep { at + 1 } else { at });
        if keep && let Some(last) = history.last_mut() {
            last.content.retain(|b| !matches!(b, Block::ToolUse { .. }));
            if !last.content.iter().any(|b| matches!(b, Block::Text { text } if !text.trim().is_empty())) {
                history.pop();
            }
        }
    }
    while history.last().is_some_and(|m| m.role == Role::User) {
        history.pop();
    }
}

/// Where each question starts in the history: a user message with text (a
/// message of tool results continues the turn before it).
fn history_turns(h: &[Message]) -> Vec<usize> {
    h.iter()
        .enumerate()
        .filter(|(_, m)| m.role == Role::User && m.content.iter().any(|b| matches!(b, Block::Text { .. })))
        .map(|(i, _)| i)
        .collect()
}

/// Where each question starts in the transcript: an item `{"kind": "user"}`.
fn display_turns(items: &[Value]) -> Vec<usize> {
    items.iter().enumerate().filter(|(_, it)| it.get("kind").and_then(Value::as_str) == Some("user")).map(|(i, _)| i).collect()
}

/// Drop the oldest question of a history: everything before the second
/// question. False when there is at most one.
fn drop_oldest_history_turn(h: &mut Vec<Message>) -> bool {
    match history_turns(h).get(1) {
        Some(&next) => {
            h.drain(..next);
            true
        }
        None => false,
    }
}

fn drop_oldest_display_turn(display: &mut Value) -> bool {
    let Some(items) = display.get_mut("items").and_then(Value::as_array_mut) else { return false };
    match display_turns(items).get(1) {
        Some(&next) => {
            items.drain(..next);
            true
        }
        None => false,
    }
}

/// Keep at most `max_turns` questions and about `max_bytes` of JSON,
/// dropping the oldest questions whole: a tool call always stays with its
/// result. The latest question is always kept.
pub fn cap(chat: &mut SavedChat, max_turns: usize, max_bytes: usize) {
    let display_count = |d: &Value| d.get("items").and_then(Value::as_array).map(|i| display_turns(i).len()).unwrap_or(0);
    loop {
        let (ht, dt) = (history_turns(&chat.history).len(), display_count(&chat.display));
        let over = ht.max(dt) > max_turns.max(1) || serde_json::to_string(chat).map(|s| s.len()).unwrap_or(0) > max_bytes;
        if !over {
            return;
        }
        // The longer of the two loses its oldest question (both when even).
        let mut dropped = false;
        if ht >= dt {
            dropped |= drop_oldest_history_turn(&mut chat.history);
        }
        if dt >= ht {
            dropped |= drop_oldest_display_turn(&mut chat.display);
        }
        if !dropped && !drop_oldest_history_turn(&mut chat.history) && !drop_oldest_display_turn(&mut chat.display) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn user(text: &str) -> Message {
        Message::user_text(text)
    }
    fn answer(text: &str) -> Message {
        Message { role: Role::Assistant, content: vec![Block::Text { text: text.into() }] }
    }
    fn call(id: &str, extra: Option<Value>) -> Message {
        Message {
            role: Role::Assistant,
            content: vec![
                Block::Text { text: "Let me look.".into() },
                Block::ToolUse { id: id.into(), name: "query".into(), input: json!({"sql": "SELECT 1"}), invalid: None, extra },
            ],
        }
    }
    fn result(id: &str, content: &str) -> Message {
        Message { role: Role::User, content: vec![Block::ToolResult { tool_use_id: id.into(), content: content.into(), is_error: false }] }
    }
    /// One question with a tool call: 4 messages.
    fn turn(n: usize, payload: &str) -> Vec<Message> {
        let id = format!("call_{n}");
        vec![user(&format!("question {n}")), call(&id, None), result(&id, payload), answer(&format!("answer {n}"))]
    }
    fn display_for(n: usize) -> Value {
        let items: Vec<Value> = (0..n)
            .flat_map(|i| {
                [
                    json!({"kind": "user", "text": format!("question {i}")}),
                    json!({"kind": "assistant", "done": true, "parts": [
                        {"type": "tool", "id": format!("call_{i}"), "name": "query", "input": {"sql": "SELECT 1"}, "result": "{}"},
                        {"type": "text", "text": format!("answer {i}")}
                    ]}),
                ]
            })
            .collect();
        json!({"items": items, "usage": {"input_tokens": 1, "output_tokens": 2, "cache_read_tokens": 0, "cache_write_tokens": 0}, "cost": 0.5})
    }
    fn key(dir: &Path, name: &str, sheet: Option<&str>) -> ChatKey {
        let p = dir.join(name);
        std::fs::write(&p, "a,b\n1,2\n").unwrap();
        ChatKey::new(&p, sheet)
    }
    fn paired(h: &[Message]) -> bool {
        let mut copy = h.to_vec();
        repair_history(&mut copy);
        copy == h
    }

    #[test]
    fn round_trip_keeps_history_display_and_tool_extra() {
        let dir = tempfile::tempdir().unwrap();
        let store = ChatStore::new(dir.path().join("chats"));
        let k = key(dir.path(), "sales.csv", None);
        let extra = json!({"google": {"thought_signature": "c2lnbmF0dXJl"}});
        let history = vec![user("q"), call("c1", Some(extra.clone())), result("c1", "[1]"), answer("one")];
        let chat = SavedChat::new(&k, history.clone(), display_for(1), Some("gemini".into()), Some("gemini-3".into()));
        store.save(&k, chat.clone()).unwrap();
        let back = store.load(&k).unwrap();
        assert_eq!(back, chat);
        let Block::ToolUse { extra: got, .. } = &back.history[1].content[1] else { panic!() };
        assert_eq!(got.as_ref(), Some(&extra));
        assert_eq!(store.count(), 1);
        // No temporary files left behind.
        assert_eq!(std::fs::read_dir(dir.path().join("chats")).unwrap().count(), 1);
        assert!(store.delete(&k));
        assert!(store.load(&k).is_none());
        assert!(!store.delete(&k));
    }

    #[test]
    fn excel_sheets_have_their_own_key() {
        let dir = tempfile::tempdir().unwrap();
        let a = key(dir.path(), "book.xlsx", Some("Q1"));
        let b = key(dir.path(), "book.xlsx", Some("Q2"));
        let none = key(dir.path(), "book.xlsx", None);
        assert_ne!(a.file_name(), b.file_name());
        assert_ne!(a.file_name(), none.file_name());
        assert_eq!(a.file_name(), ChatKey::new(&dir.path().join("book.xlsx"), Some("Q1")).file_name());
        // The same file by another spelling of its path.
        let other = dir.path().join(".").join("book.xlsx");
        assert_eq!(ChatKey::new(&other, Some("Q1")), a);

        let store = ChatStore::new(dir.path().join("chats"));
        store.save(&a, SavedChat::new(&a, vec![user("q"), answer("a")], display_for(1), None, None)).unwrap();
        assert!(store.load(&a).is_some());
        assert!(store.load(&b).is_none());
        assert_eq!(store.load(&a).unwrap().sheet.as_deref(), Some("Q1"));
    }

    #[test]
    fn corrupt_and_newer_files_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let store = ChatStore::new(dir.path().join("chats"));
        let k = key(dir.path(), "x.parquet", None);
        std::fs::create_dir_all(dir.path().join("chats")).unwrap();
        let file = dir.path().join("chats").join(k.file_name());
        for text in ["", "{not json", "[]", r#"{"version": "one"}"#, r#"{"version": 1, "path": 5}"#] {
            std::fs::write(&file, text).unwrap();
            assert!(store.load(&k).is_none(), "{text}");
        }
        let mut newer = serde_json::to_value(SavedChat::new(&k, vec![user("q"), answer("a")], display_for(1), None, None)).unwrap();
        newer["version"] = json!(SCHEMA_VERSION + 1);
        std::fs::write(&file, newer.to_string()).unwrap();
        assert!(store.load(&k).is_none());
        // Another file's conversation under the same name (a collision).
        let other = key(dir.path(), "y.parquet", None);
        let theirs = SavedChat::new(&other, vec![user("q"), answer("a")], display_for(1), None, None);
        std::fs::write(&file, serde_json::to_string(&theirs).unwrap()).unwrap();
        assert!(store.load(&k).is_none());
    }

    #[test]
    fn a_trailing_tool_call_without_result_is_dropped() {
        // Cut off mid tool call: the text stays, the call goes.
        let mut h = [turn(0, "[1]"), vec![user("q2"), call("c9", None)]].concat();
        repair_history(&mut h);
        assert_eq!(h.len(), 6);
        assert_eq!(h[5], answer("Let me look."));
        assert!(paired(&h));

        // A call with no text: the message goes, then the unanswered question.
        let mut h = turn(0, "[1]");
        h.push(user("q2"));
        h.push(Message { role: Role::Assistant, content: vec![Block::ToolUse { id: "c".into(), name: "query".into(), input: json!({}), invalid: None, extra: None }] });
        repair_history(&mut h);
        assert_eq!(h, turn(0, "[1]"));

        // A result for some other call, and one call answered only in part.
        let mut h = [turn(0, "[1]"), vec![user("q2"), call("a", None), result("b", "x"), answer("?")]].concat();
        repair_history(&mut h);
        assert_eq!(h.len(), 6);
        let mut h = [turn(0, "[1]"), vec![result("zz", "x")]].concat();
        repair_history(&mut h);
        assert_eq!(h, turn(0, "[1]"));

        // A complete history is left as it is.
        let mut h = [turn(0, "a"), turn(1, "b")].concat();
        let before = h.clone();
        repair_history(&mut h);
        assert_eq!(h, before);
    }

    #[test]
    fn saving_repairs_the_history() {
        let dir = tempfile::tempdir().unwrap();
        let store = ChatStore::new(dir.path().join("chats"));
        let k = key(dir.path(), "a.csv", None);
        let h = [turn(0, "[1]"), vec![user("q2"), call("c9", None)]].concat();
        store.save(&k, SavedChat::new(&k, h, display_for(2), None, None)).unwrap();
        let back = store.load(&k).unwrap();
        assert!(paired(&back.history));
        assert!(back.history.iter().all(|m| tool_use_ids(m).is_empty() || m.content.len() > 1));
    }

    #[test]
    fn capping_drops_oldest_whole_turns() {
        let dir = tempfile::tempdir().unwrap();
        let k = ChatKey::new(&dir.path().join("a.csv"), None);
        // By count.
        let h: Vec<Message> = (0..10).flat_map(|n| turn(n, "[1]")).collect();
        let mut c = SavedChat::new(&k, h, display_for(10), None, None);
        cap(&mut c, 3, usize::MAX);
        assert_eq!(history_turns(&c.history).len(), 3);
        assert_eq!(c.history[0], user("question 7"));
        assert!(paired(&c.history));
        let items = c.display["items"].as_array().unwrap();
        assert_eq!(items.len(), 6);
        assert_eq!(items[0]["text"], "question 7");
        assert_eq!(c.display["cost"], 0.5, "totals stay");

        // By size: each turn carries ~50 kB of tool result.
        let big = "x".repeat(50_000);
        let h: Vec<Message> = (0..10).flat_map(|n| turn(n, &big)).collect();
        let mut c = SavedChat::new(&k, h, display_for(10), None, None);
        cap(&mut c, 100, 200_000);
        let size = serde_json::to_string(&c).unwrap().len();
        assert!(size <= 200_000, "{size}");
        assert!(history_turns(&c.history).len() >= 3);
        assert!(paired(&c.history), "a tool call never loses its result");
        assert_eq!(c.history.last().unwrap(), &answer("answer 9"));

        // The latest turn stays even when it alone is too big.
        let h = turn(0, &"y".repeat(10_000));
        let mut c = SavedChat::new(&k, h.clone(), display_for(1), None, None);
        cap(&mut c, 100, 1_000);
        assert_eq!(c.history, h);

        // More transcript turns than history turns (failed questions aren't in the history).
        let h: Vec<Message> = (0..2).flat_map(|n| turn(n, "[1]")).collect();
        let mut c = SavedChat::new(&k, h, display_for(6), None, None);
        cap(&mut c, 3, usize::MAX);
        assert_eq!(c.display["items"].as_array().unwrap().len(), 6);
        assert_eq!(history_turns(&c.history).len(), 2);
    }

    #[test]
    fn delete_all_counts() {
        let dir = tempfile::tempdir().unwrap();
        let store = ChatStore::new(dir.path().join("chats"));
        assert_eq!(store.count(), 0);
        assert_eq!(store.delete_all(), 0);
        for name in ["a.csv", "b.csv", "c.csv"] {
            let k = key(dir.path(), name, None);
            store.save(&k, SavedChat::new(&k, vec![user("q"), answer("a")], display_for(1), None, None)).unwrap();
        }
        assert_eq!(store.count(), 3);
        assert_eq!(store.delete_all(), 3);
        assert_eq!(store.count(), 0);
    }
}
