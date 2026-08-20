use std::{
    collections::HashMap,
    env,
    io::SeekFrom,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader},
};

use crate::preferences;

const CAPTURE_VERSION: u8 = 1;
const PROVIDER: &str = "github-copilot-cli";
const MAX_INTERACTION_SCAN_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEventKind {
    Prompt,
    Queued,
    Steering,
    Completed,
    Stopped,
    Ended,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SessionEvent {
    pub id: String,
    pub session_id: String,
    pub timestamp: String,
    pub kind: SessionEventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<String>,
}

#[derive(Default, Serialize)]
pub struct SessionActivity {
    pub events: Vec<SessionEvent>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
struct SessionCapture {
    version: u8,
    provider: String,
    session_id: String,
    source: PathBuf,
    offset: u64,
    #[serde(default)]
    interaction_ids: Vec<String>,
    #[serde(default)]
    current_interaction_id: Option<String>,
    #[serde(default)]
    pending_interaction_ids: Vec<String>,
    events: Vec<SessionEvent>,
}

struct InteractionTarget {
    id: String,
    offset: u64,
}

#[derive(Deserialize)]
struct RawEvent {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    timestamp: String,
    #[serde(default)]
    data: Value,
}

pub async fn record_current(tickets_dir: &Path) -> Result<bool> {
    if !recording_enabled() {
        return Ok(false);
    }
    let Some(session_id) = env::var_os("COPILOT_AGENT_SESSION_ID") else {
        return Ok(false);
    };
    let session_id = session_id
        .into_string()
        .map_err(|_| anyhow::anyhow!("COPILOT_AGENT_SESSION_ID is not valid Unicode"))?;
    validate_session_id(&session_id)?;
    let source = copilot_session_root()?
        .join(&session_id)
        .join("events.jsonl");
    let Some(target) = current_interaction(&source).await? else {
        return Ok(false);
    };
    let workspace_key = preferences::workspace_key(tickets_dir).await;
    let path = capture_path(&preferences::config_root()?, &workspace_key, &session_id);
    let (mut capture, existed) = load_capture(&path).await?.map_or_else(
        || {
            (
                SessionCapture {
                    version: CAPTURE_VERSION,
                    provider: PROVIDER.to_owned(),
                    session_id: session_id.clone(),
                    source: source.clone(),
                    offset: target.offset,
                    interaction_ids: Vec::new(),
                    current_interaction_id: None,
                    pending_interaction_ids: Vec::new(),
                    events: Vec::new(),
                },
                false,
            )
        },
        |capture| (capture, true),
    );
    let mut changed = false;
    let source_reset = capture.source != source;
    if source_reset {
        capture.source = source;
        capture.offset = target.offset;
        capture.current_interaction_id = None;
        capture.pending_interaction_ids.clear();
        capture.events.clear();
        changed = true;
    }
    if existed && !source_reset {
        changed = sync_capture(&mut capture).await? || changed;
    }
    let newly_registered = !capture
        .interaction_ids
        .iter()
        .any(|recorded| recorded == &target.id);
    if newly_registered {
        capture.interaction_ids.push(target.id.clone());
        changed = true;
        if existed && !source_reset {
            changed = backfill_interaction(&mut capture, &target).await? || changed;
        }
    }
    if !existed || source_reset {
        changed = sync_capture(&mut capture).await? || changed;
    }
    if changed || !existed {
        save_capture(&path, &capture).await?;
    }
    Ok(true)
}

pub async fn load(workspace_key: &str) -> Result<SessionActivity> {
    load_from_root(&preferences::config_root()?, workspace_key).await
}

async fn load_from_root(config_root: &Path, workspace_key: &str) -> Result<SessionActivity> {
    let directory = captures_dir(config_root, workspace_key);
    let mut entries = match fs::read_dir(&directory).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SessionActivity::default());
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("reading session captures from {}", directory.display()));
        }
    };
    let mut activity = SessionActivity::default();
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let capture = match load_capture(&path).await {
            Ok(Some(capture)) => capture,
            Ok(None) => continue,
            Err(error) => {
                activity.warnings.push(format!(
                    "Could not read session capture {}: {error}",
                    path.file_name()
                        .map(|name| name.to_string_lossy())
                        .unwrap_or_default()
                ));
                continue;
            }
        };
        activity.events.extend(capture.events);
    }
    activity.events.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then(left.id.cmp(&right.id))
    });
    Ok(activity)
}

async fn load_capture(path: &Path) -> Result<Option<SessionCapture>> {
    let contents = match fs::read(path).await {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("reading {}", path.display()));
        }
    };
    secure_capture_file(path).await?;
    let capture: SessionCapture =
        serde_json::from_slice(&contents).with_context(|| format!("parsing {}", path.display()))?;
    if capture.version != CAPTURE_VERSION {
        bail!(
            "unsupported session capture version {} in {}",
            capture.version,
            path.display()
        );
    }
    if capture.provider != PROVIDER {
        bail!(
            "unsupported session provider '{}' in {}",
            capture.provider,
            path.display()
        );
    }
    validate_session_id(&capture.session_id)?;
    Ok(Some(capture))
}

async fn sync_capture(capture: &mut SessionCapture) -> Result<bool> {
    let length = match fs::metadata(&capture.source).await {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(error).with_context(|| format!("reading {}", capture.source.display()));
        }
    };
    let mut next = capture.clone();
    let mut changed = false;
    if length < next.offset {
        next.offset = 0;
        next.current_interaction_id = None;
        next.events.clear();
        changed = true;
    }
    if length == next.offset {
        if changed {
            *capture = next;
        }
        return Ok(changed);
    }
    let mut file = fs::File::open(&next.source)
        .await
        .with_context(|| format!("opening {}", next.source.display()))?;
    file.seek(SeekFrom::Start(next.offset)).await?;
    let mut reader = BufReader::new(file);
    loop {
        let mut line = Vec::new();
        let read = reader.read_until(b'\n', &mut line).await?;
        if read == 0 || !line.ends_with(b"\n") {
            break;
        }
        line.pop();
        if line.is_empty() {
            next.offset += read as u64;
            changed = true;
            continue;
        }
        let raw: RawEvent = serde_json::from_slice(&line)
            .with_context(|| format!("parsing {}", next.source.display()))?;
        apply_raw_event(&mut next, raw);
        next.offset += read as u64;
        changed = true;
    }
    if changed {
        *capture = next;
    }
    Ok(changed)
}

fn apply_raw_event(capture: &mut SessionCapture, raw: RawEvent) {
    let is_abort = raw.kind == "abort";
    if raw.kind == "assistant.turn_start"
        && let Some(interaction_id) = raw.data.get("interactionId").and_then(Value::as_str)
    {
        capture.current_interaction_id = Some(interaction_id.to_owned());
        capture
            .pending_interaction_ids
            .retain(|pending| pending != interaction_id);
    }
    let human_interaction = human_interaction_id(&raw);
    if let Some(interaction_id) = human_interaction.as_ref() {
        if human_delivery(&raw) == Some("queued") {
            if !capture
                .pending_interaction_ids
                .iter()
                .any(|pending| pending == interaction_id)
            {
                capture.pending_interaction_ids.push(interaction_id.clone());
            }
        } else if capture.current_interaction_id.as_deref() != Some(interaction_id) {
            capture.current_interaction_id = Some(interaction_id.clone());
        }
    }
    let event_interaction = if is_abort {
        capture
            .current_interaction_id
            .clone()
            .or_else(|| capture.pending_interaction_ids.first().cloned())
    } else {
        human_interaction.or_else(|| capture.current_interaction_id.clone())
    };
    let interaction_recorded = event_interaction.as_ref().is_some_and(|interaction_id| {
        capture
            .interaction_ids
            .iter()
            .any(|recorded| recorded == interaction_id)
    });
    let previous = capture
        .events
        .iter()
        .rev()
        .find(|event| event.interaction_id.as_ref() == event_interaction.as_ref());
    if let Some(event) = session_event(
        &capture.session_id,
        raw,
        event_interaction.as_deref(),
        interaction_recorded,
        previous,
    ) {
        capture.events.push(event);
    }
    if is_abort && let Some(interaction_id) = event_interaction {
        if capture.current_interaction_id.as_deref() == Some(&interaction_id) {
            capture.current_interaction_id = None;
        }
        capture
            .pending_interaction_ids
            .retain(|pending| pending != &interaction_id);
    }
}

async fn backfill_interaction(
    capture: &mut SessionCapture,
    target: &InteractionTarget,
) -> Result<bool> {
    if target.offset >= capture.offset {
        return Ok(false);
    }
    let end = capture.offset;
    let mut file = fs::File::open(&capture.source)
        .await
        .with_context(|| format!("opening {}", capture.source.display()))?;
    file.seek(SeekFrom::Start(target.offset)).await?;
    let mut reader = BufReader::new(file);
    let mut position = target.offset;
    let mut active = false;
    let mut additions = Vec::new();
    while position < end {
        let mut line = Vec::new();
        let read = reader.read_until(b'\n', &mut line).await?;
        if read == 0 || !line.ends_with(b"\n") || position + read as u64 > end {
            break;
        }
        position += read as u64;
        line.pop();
        if line.is_empty() {
            continue;
        }
        let raw: RawEvent = serde_json::from_slice(&line)
            .with_context(|| format!("parsing {}", capture.source.display()))?;
        let kind = raw.kind.clone();
        let human_interaction = human_interaction_id(&raw);
        if human_interaction.as_deref() == Some(&target.id) {
            if human_delivery(&raw) != Some("queued") {
                active = true;
            }
        } else if human_interaction.is_some() && human_delivery(&raw) != Some("queued") {
            active = false;
        }
        if kind == "assistant.turn_start"
            && let Some(interaction_id) = raw.data.get("interactionId").and_then(Value::as_str)
        {
            active = interaction_id == target.id;
        }
        let relevant = human_interaction.as_deref() == Some(&target.id)
            || (active
                && matches!(
                    kind.as_str(),
                    "session.task_complete" | "abort" | "session.shutdown"
                ));
        if relevant {
            let previous = additions
                .iter()
                .rev()
                .chain(capture.events.iter().rev())
                .find(|event| event.interaction_id.as_deref() == Some(&target.id));
            if let Some(event) =
                session_event(&capture.session_id, raw, Some(&target.id), true, previous)
                && !capture
                    .events
                    .iter()
                    .chain(additions.iter())
                    .any(|existing| existing.id == event.id)
            {
                additions.push(event);
            }
        }
        if kind == "abort" && active {
            active = false;
        }
    }
    let changed = !additions.is_empty();
    capture.events.extend(additions);
    Ok(changed)
}

fn session_event(
    session_id: &str,
    raw: RawEvent,
    interaction_id: Option<&str>,
    interaction_recorded: bool,
    previous: Option<&SessionEvent>,
) -> Option<SessionEvent> {
    if !interaction_recorded {
        return None;
    }
    match raw.kind.as_str() {
        "user.message" => {
            if raw
                .data
                .get("source")
                .is_some_and(|source| !source.is_null())
            {
                return None;
            }
            let content = raw.data.get("content")?.as_str()?;
            if content.trim().is_empty() {
                return None;
            }
            let kind = match raw.data.get("delivery").and_then(Value::as_str) {
                Some("queued") => SessionEventKind::Queued,
                Some("steering") => SessionEventKind::Steering,
                _ => SessionEventKind::Prompt,
            };
            Some(SessionEvent {
                id: raw.id,
                session_id: session_id.to_owned(),
                timestamp: raw.timestamp,
                kind,
                content: Some(content.to_owned()),
                interaction_id: interaction_id.map(str::to_owned),
            })
        }
        "session.task_complete" => {
            let content = raw
                .data
                .get("summary")
                .and_then(Value::as_str)
                .filter(|summary| !summary.trim().is_empty())
                .map(str::to_owned);
            Some(SessionEvent {
                id: raw.id,
                session_id: session_id.to_owned(),
                timestamp: raw.timestamp,
                kind: if raw.data.get("success").and_then(Value::as_bool) == Some(true) {
                    SessionEventKind::Completed
                } else {
                    SessionEventKind::Ended
                },
                content,
                interaction_id: interaction_id.map(str::to_owned),
            })
        }
        "abort" => Some(SessionEvent {
            id: raw.id,
            session_id: session_id.to_owned(),
            timestamp: raw.timestamp,
            kind: match raw.data.get("reason").and_then(Value::as_str) {
                Some("user_initiated" | "user_abort") => SessionEventKind::Stopped,
                _ => SessionEventKind::Ended,
            },
            content: raw
                .data
                .get("reason")
                .and_then(Value::as_str)
                .map(|reason| format!("Session aborted: {}.", reason.replace('_', " "))),
            interaction_id: interaction_id.map(str::to_owned),
        }),
        "session.shutdown"
            if previous.is_some_and(|event| {
                matches!(
                    event.kind,
                    SessionEventKind::Prompt
                        | SessionEventKind::Queued
                        | SessionEventKind::Steering
                )
            }) =>
        {
            Some(SessionEvent {
                id: raw.id,
                session_id: session_id.to_owned(),
                timestamp: raw.timestamp,
                kind: SessionEventKind::Ended,
                content: Some("Session ended before a completion event was recorded.".to_owned()),
                interaction_id: interaction_id.map(str::to_owned),
            })
        }
        _ => None,
    }
}

fn human_interaction_id(raw: &RawEvent) -> Option<String> {
    if raw.kind != "user.message"
        || raw
            .data
            .get("source")
            .is_some_and(|source| !source.is_null())
        || raw
            .data
            .get("content")
            .and_then(Value::as_str)
            .is_none_or(|content| content.trim().is_empty())
    {
        return None;
    }
    Some(
        raw.data
            .get("interactionId")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("event:{}", raw.id)),
    )
}

fn human_delivery(raw: &RawEvent) -> Option<&str> {
    raw.data.get("delivery").and_then(Value::as_str)
}

async fn current_interaction(source: &Path) -> Result<Option<InteractionTarget>> {
    let length = match fs::metadata(source).await {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("reading {}", source.display()));
        }
    };
    let start = length.saturating_sub(MAX_INTERACTION_SCAN_BYTES);
    let mut file = fs::File::open(source)
        .await
        .with_context(|| format!("opening {}", source.display()))?;
    file.seek(SeekFrom::Start(start)).await?;
    let mut contents = Vec::with_capacity((length - start) as usize);
    file.read_to_end(&mut contents).await?;
    let (contents, base_offset) = if start == 0 {
        (contents.as_slice(), 0)
    } else {
        let Some(first_newline) = contents.iter().position(|byte| *byte == b'\n') else {
            return Ok(None);
        };
        (
            &contents[first_newline + 1..],
            start + first_newline as u64 + 1,
        )
    };
    interaction_target(contents, base_offset)
}

fn interaction_target(contents: &[u8], base_offset: u64) -> Result<Option<InteractionTarget>> {
    let complete_length = contents
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let mut offsets = HashMap::new();
    let mut latest_human = None;
    let mut active_turn: Option<(String, String, u64)> = None;
    let mut consumed = 0_u64;
    for terminated_line in contents[..complete_length].split_inclusive(|byte| *byte == b'\n') {
        let line_offset = base_offset + consumed;
        consumed += terminated_line.len() as u64;
        let line = terminated_line
            .strip_suffix(b"\n")
            .unwrap_or(terminated_line);
        if line.is_empty() {
            continue;
        }
        let raw: RawEvent =
            serde_json::from_slice(line).context("parsing Copilot session event")?;
        if let Some(interaction_id) = human_interaction_id(&raw) {
            offsets.entry(interaction_id.clone()).or_insert(line_offset);
            if human_delivery(&raw) != Some("queued") {
                latest_human = Some(interaction_id);
            }
        }
        match raw.kind.as_str() {
            "assistant.turn_start" => {
                if let (Some(turn_id), Some(interaction_id)) = (
                    raw.data.get("turnId").and_then(Value::as_str),
                    raw.data.get("interactionId").and_then(Value::as_str),
                ) {
                    active_turn =
                        Some((turn_id.to_owned(), interaction_id.to_owned(), line_offset));
                }
            }
            "assistant.turn_end" => {
                if let Some(turn_id) = raw.data.get("turnId").and_then(Value::as_str)
                    && active_turn
                        .as_ref()
                        .is_some_and(|(active, _, _)| active == turn_id)
                {
                    active_turn = None;
                }
            }
            _ => {}
        }
    }
    let Some((interaction_id, fallback_offset)) = active_turn
        .map(|(_, interaction_id, offset)| (interaction_id, Some(offset)))
        .or_else(|| latest_human.map(|interaction_id| (interaction_id, None)))
    else {
        return Ok(None);
    };
    let Some(offset) = offsets.get(&interaction_id).copied().or(fallback_offset) else {
        return Ok(None);
    };
    Ok(Some(InteractionTarget {
        id: interaction_id,
        offset,
    }))
}

async fn save_capture(path: &Path, capture: &SessionCapture) -> Result<()> {
    let parent = path
        .parent()
        .context("cannot locate session capture directory")?;
    fs::create_dir_all(parent).await?;
    if let Some(sessions) = parent.parent() {
        secure_capture_directory(sessions).await?;
    }
    secure_capture_directory(parent).await?;
    let temporary = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        rand::random::<u64>()
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = match options.open(&temporary).await {
        Ok(file) => file,
        Err(error) => return Err(error).with_context(|| format!("creating {}", path.display())),
    };
    if let Err(error) = file.write_all(&serde_json::to_vec_pretty(capture)?).await {
        let _ = fs::remove_file(&temporary).await;
        return Err(error).with_context(|| format!("writing {}", path.display()));
    }
    file.flush().await?;
    drop(file);
    if cfg!(windows)
        && fs::metadata(path).await.is_ok()
        && let Err(error) = fs::remove_file(path).await
    {
        let _ = fs::remove_file(&temporary).await;
        return Err(error).with_context(|| format!("replacing {}", path.display()));
    }
    match fs::rename(&temporary, path).await {
        Ok(()) => secure_capture_file(path).await,
        Err(error) => {
            let _ = fs::remove_file(&temporary).await;
            Err(error).with_context(|| format!("replacing {}", path.display()))
        }
    }
}

#[cfg(unix)]
async fn secure_capture_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .await
        .with_context(|| format!("securing {}", path.display()))
}

#[cfg(not(unix))]
async fn secure_capture_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
async fn secure_capture_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .await
        .with_context(|| format!("securing {}", path.display()))
}

#[cfg(not(unix))]
async fn secure_capture_file(_path: &Path) -> Result<()> {
    Ok(())
}

fn copilot_session_root() -> Result<PathBuf> {
    env::var_os("COPILOT_SESSION_STATE_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".copilot/session-state"))
        })
        .context("cannot locate the GitHub Copilot CLI session directory")
}

fn capture_path(config_root: &Path, workspace_key: &str, session_id: &str) -> PathBuf {
    captures_dir(config_root, workspace_key).join(format!("{session_id}.json"))
}

fn captures_dir(config_root: &Path, workspace_key: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(workspace_key.as_bytes());
    config_root
        .join("sessions")
        .join(format!("{:x}", hasher.finalize()))
}

fn recording_enabled() -> bool {
    !env::var("FER_SESSION_RECORDING")
        .ok()
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "0" | "false" | "off"))
}

fn validate_session_id(session_id: &str) -> Result<()> {
    if session_id.is_empty()
        || session_id.len() > 128
        || !session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        bail!("invalid Copilot session ID");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    fn raw(value: &str) -> RawEvent {
        serde_json::from_str(value).unwrap()
    }

    #[test]
    fn captures_human_prompt_delivery_and_lifecycle_events() {
        let mut events = Vec::new();
        for event in [
            raw(
                r#"{"id":"1","type":"user.message","timestamp":"2026-01-01T00:00:00Z","data":{"content":"Start here","delivery":"idle","interactionId":"a"}}"#,
            ),
            raw(
                r#"{"id":"2","type":"user.message","timestamp":"2026-01-01T00:01:00Z","data":{"content":"Do this next","delivery":"queued","interactionId":"a"}}"#,
            ),
            raw(
                r#"{"id":"3","type":"user.message","timestamp":"2026-01-01T00:02:00Z","data":{"content":"Use the smaller version","delivery":"steering","interactionId":"a"}}"#,
            ),
            raw(
                r#"{"id":"4","type":"user.message","timestamp":"2026-01-01T00:03:00Z","data":{"content":"internal","source":"system","delivery":"steering"}}"#,
            ),
            raw(
                r#"{"id":"5","type":"session.task_complete","timestamp":"2026-01-01T00:04:00Z","data":{"summary":"Done","success":true}}"#,
            ),
            raw(
                r#"{"id":"6","type":"abort","timestamp":"2026-01-01T00:05:00Z","data":{"reason":"user_initiated"}}"#,
            ),
        ] {
            if let Some(event) = session_event("session-1", event, Some("a"), true, events.last()) {
                events.push(event);
            }
        }
        assert_eq!(
            events.iter().map(|event| &event.kind).collect::<Vec<_>>(),
            [
                &SessionEventKind::Prompt,
                &SessionEventKind::Queued,
                &SessionEventKind::Steering,
                &SessionEventKind::Completed,
                &SessionEventKind::Stopped,
            ]
        );
        assert_eq!(events[0].content.as_deref(), Some("Start here"));
        assert_eq!(events[2].interaction_id.as_deref(), Some("a"));
    }

    #[test]
    fn routine_shutdown_after_an_unfinished_prompt_is_ended() {
        let prompt = session_event(
            "session-1",
            raw(r#"{"id":"1","type":"user.message","timestamp":"2026-01-01T00:00:00Z","data":{"content":"Keep going"}}"#),
            Some("event:1"),
            true,
            None,
        )
        .unwrap();
        let stopped = session_event(
            "session-1",
            raw(r#"{"id":"2","type":"session.shutdown","timestamp":"2026-01-01T00:01:00Z","data":{"shutdownType":"routine"}}"#),
            Some("event:1"),
            true,
            Some(&prompt),
        )
        .unwrap();
        assert_eq!(stopped.kind, SessionEventKind::Ended);
    }

    #[test]
    fn active_turn_wins_over_a_distinct_queued_prompt() {
        let contents = concat!(
            "{\"id\":\"1\",\"type\":\"user.message\",\"timestamp\":\"2026-01-01T00:00:00Z\",\"data\":{\"content\":\"Active\",\"delivery\":\"idle\",\"interactionId\":\"a\"}}\n",
            "{\"id\":\"2\",\"type\":\"assistant.turn_start\",\"timestamp\":\"2026-01-01T00:00:01Z\",\"data\":{\"turnId\":\"1\",\"interactionId\":\"a\"}}\n",
            "{\"id\":\"3\",\"type\":\"user.message\",\"timestamp\":\"2026-01-01T00:00:02Z\",\"data\":{\"content\":\"Later\",\"delivery\":\"queued\",\"interactionId\":\"b\"}}\n"
        );
        let target = interaction_target(contents.as_bytes(), 0).unwrap().unwrap();
        assert_eq!(target.id, "a");
        assert_eq!(target.offset, 0);
    }

    #[test]
    fn consecutive_aborts_advance_from_active_to_queued_interaction() {
        let mut capture = SessionCapture {
            version: CAPTURE_VERSION,
            provider: PROVIDER.to_owned(),
            session_id: "session-1".to_owned(),
            source: PathBuf::new(),
            offset: 0,
            interaction_ids: vec!["a".to_owned(), "b".to_owned()],
            current_interaction_id: Some("a".to_owned()),
            pending_interaction_ids: Vec::new(),
            events: Vec::new(),
        };
        apply_raw_event(
            &mut capture,
            raw(
                r#"{"id":"1","type":"user.message","timestamp":"2026-01-01T00:00:00Z","data":{"content":"Later","delivery":"queued","interactionId":"b"}}"#,
            ),
        );
        apply_raw_event(
            &mut capture,
            raw(
                r#"{"id":"2","type":"abort","timestamp":"2026-01-01T00:01:00Z","data":{"reason":"user_initiated"}}"#,
            ),
        );
        apply_raw_event(
            &mut capture,
            raw(
                r#"{"id":"3","type":"abort","timestamp":"2026-01-01T00:02:00Z","data":{"reason":"user_abort"}}"#,
            ),
        );
        assert_eq!(
            capture
                .events
                .iter()
                .filter(|event| event.kind == SessionEventKind::Stopped)
                .filter_map(|event| event.interaction_id.as_deref())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert!(capture.current_interaction_id.is_none());
        assert!(capture.pending_interaction_ids.is_empty());
    }

    #[tokio::test]
    async fn syncs_only_new_complete_json_lines() {
        let root = env::temp_dir().join(format!(
            "ferricket-session-activity-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&root).await.unwrap();
        let source = root.join("events.jsonl");
        fs::write(
            &source,
            b"{\"id\":\"1\",\"type\":\"user.message\",\"timestamp\":\"2026-01-01T00:00:00Z\",\"data\":{\"content\":\"First\",\"interactionId\":\"a\"}}\n{\"id\":\"partial\"",
        )
        .await
        .unwrap();
        let mut capture = SessionCapture {
            version: CAPTURE_VERSION,
            provider: PROVIDER.to_owned(),
            session_id: "session-1".to_owned(),
            source: source.clone(),
            offset: 0,
            interaction_ids: vec!["a".to_owned()],
            current_interaction_id: None,
            pending_interaction_ids: Vec::new(),
            events: Vec::new(),
        };
        assert!(sync_capture(&mut capture).await.unwrap());
        assert_eq!(capture.events.len(), 1);
        let first_offset = capture.offset;

        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&source)
            .await
            .unwrap();
        file.write_all(
            b",\"type\":\"user.message\",\"timestamp\":\"2026-01-01T00:01:00Z\",\"data\":{\"content\":\"Second\",\"delivery\":\"steering\",\"interactionId\":\"a\"}}\n",
        )
        .await
        .unwrap();
        file.flush().await.unwrap();

        assert!(sync_capture(&mut capture).await.unwrap());
        assert!(capture.offset > first_offset);
        assert_eq!(capture.events.len(), 2);
        assert_eq!(capture.events[1].kind, SessionEventKind::Steering);
        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn queued_prompt_does_not_steal_active_completion() {
        let root = env::temp_dir().join(format!(
            "ferricket-session-scope-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&root).await.unwrap();
        let source = root.join("events.jsonl");
        fs::write(
            &source,
            concat!(
                "{\"id\":\"1\",\"type\":\"user.message\",\"timestamp\":\"2026-01-01T00:00:00Z\",\"data\":{\"content\":\"Project A\",\"delivery\":\"idle\",\"interactionId\":\"a\"}}\n",
                "{\"id\":\"2\",\"type\":\"assistant.turn_start\",\"timestamp\":\"2026-01-01T00:00:01Z\",\"data\":{\"turnId\":\"1\",\"interactionId\":\"a\"}}\n",
                "{\"id\":\"3\",\"type\":\"user.message\",\"timestamp\":\"2026-01-01T00:00:02Z\",\"data\":{\"content\":\"Project B\",\"delivery\":\"queued\",\"interactionId\":\"b\"}}\n",
                "{\"id\":\"4\",\"type\":\"session.task_complete\",\"timestamp\":\"2026-01-01T00:01:00Z\",\"data\":{\"summary\":\"Done A\",\"success\":true}}\n"
            ),
        )
        .await
        .unwrap();
        let mut capture = SessionCapture {
            version: CAPTURE_VERSION,
            provider: PROVIDER.to_owned(),
            session_id: "session-1".to_owned(),
            source,
            offset: 0,
            interaction_ids: vec!["a".to_owned()],
            current_interaction_id: None,
            pending_interaction_ids: Vec::new(),
            events: Vec::new(),
        };

        assert!(sync_capture(&mut capture).await.unwrap());
        assert_eq!(
            capture
                .events
                .iter()
                .filter_map(|event| event.content.as_deref())
                .collect::<Vec<_>>(),
            ["Project A", "Done A"]
        );
        assert!(capture.events.iter().all(|event| {
            event.interaction_id.as_deref() == Some("a") && event.kind != SessionEventKind::Queued
        }));
        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn backfill_keeps_prior_completion_on_prior_interaction() {
        let root = env::temp_dir().join(format!(
            "ferricket-session-backfill-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&root).await.unwrap();
        let source = root.join("events.jsonl");
        let first = "{\"id\":\"1\",\"type\":\"user.message\",\"timestamp\":\"2026-01-01T00:00:00Z\",\"data\":{\"content\":\"Project A\",\"delivery\":\"idle\",\"interactionId\":\"a\"}}\n";
        let queued = "{\"id\":\"2\",\"type\":\"user.message\",\"timestamp\":\"2026-01-01T00:00:01Z\",\"data\":{\"content\":\"Project B\",\"delivery\":\"queued\",\"interactionId\":\"b\"}}\n";
        fs::write(
            &source,
            format!(
                "{first}{queued}{}{}",
                "{\"id\":\"3\",\"type\":\"session.task_complete\",\"timestamp\":\"2026-01-01T00:00:02Z\",\"data\":{\"summary\":\"Done A\",\"success\":true}}\n",
                "{\"id\":\"4\",\"type\":\"assistant.turn_start\",\"timestamp\":\"2026-01-01T00:00:03Z\",\"data\":{\"turnId\":\"2\",\"interactionId\":\"b\"}}\n"
            ),
        )
        .await
        .unwrap();
        let mut capture = SessionCapture {
            version: CAPTURE_VERSION,
            provider: PROVIDER.to_owned(),
            session_id: "session-1".to_owned(),
            source,
            offset: first.len() as u64,
            interaction_ids: vec!["a".to_owned()],
            current_interaction_id: Some("a".to_owned()),
            pending_interaction_ids: Vec::new(),
            events: vec![SessionEvent {
                id: "1".to_owned(),
                session_id: "session-1".to_owned(),
                timestamp: "2026-01-01T00:00:00Z".to_owned(),
                kind: SessionEventKind::Prompt,
                content: Some("Project A".to_owned()),
                interaction_id: Some("a".to_owned()),
            }],
        };

        assert!(sync_capture(&mut capture).await.unwrap());
        capture.interaction_ids.push("b".to_owned());
        assert!(
            backfill_interaction(
                &mut capture,
                &InteractionTarget {
                    id: "b".to_owned(),
                    offset: first.len() as u64,
                },
            )
            .await
            .unwrap()
        );
        assert_eq!(
            capture
                .events
                .iter()
                .map(|event| (event.kind.clone(), event.interaction_id.as_deref()))
                .collect::<Vec<_>>(),
            [
                (SessionEventKind::Prompt, Some("a")),
                (SessionEventKind::Completed, Some("a")),
                (SessionEventKind::Queued, Some("b")),
            ]
        );
        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn missing_source_keeps_cached_events_without_warning() {
        let mut capture = SessionCapture {
            version: CAPTURE_VERSION,
            provider: PROVIDER.to_owned(),
            session_id: "session-1".to_owned(),
            source: env::temp_dir().join(format!(
                "missing-ferricket-session-{}",
                rand::random::<u64>()
            )),
            offset: 10,
            interaction_ids: vec!["a".to_owned()],
            current_interaction_id: Some("a".to_owned()),
            pending_interaction_ids: Vec::new(),
            events: vec![SessionEvent {
                id: "1".to_owned(),
                session_id: "session-1".to_owned(),
                timestamp: "2026-01-01T00:00:00Z".to_owned(),
                kind: SessionEventKind::Prompt,
                content: Some("Cached".to_owned()),
                interaction_id: Some("a".to_owned()),
            }],
        };
        assert!(!sync_capture(&mut capture).await.unwrap());
        assert_eq!(capture.events.len(), 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn capture_files_are_private() {
        use std::os::unix::fs::PermissionsExt;

        let root = env::temp_dir().join(format!(
            "ferricket-session-permissions-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let path = root.join("sessions/workspace/session.json");
        let capture = SessionCapture {
            version: CAPTURE_VERSION,
            provider: PROVIDER.to_owned(),
            session_id: "session-1".to_owned(),
            source: root.join("source.jsonl"),
            offset: 0,
            interaction_ids: vec!["a".to_owned()],
            current_interaction_id: Some("a".to_owned()),
            pending_interaction_ids: Vec::new(),
            events: Vec::new(),
        };
        save_capture(&path, &capture).await.unwrap();
        assert_eq!(
            fs::metadata(&path).await.unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .await
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        fs::remove_dir_all(root).await.unwrap();
    }
}
