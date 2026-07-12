use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{SecondsFormat, Utc};
use futures::{StreamExt, stream};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use rand::{Rng, distr::Alphanumeric};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub struct Ticket {
    pub path: PathBuf,
    pub raw: String,
    pub fields: BTreeMap<String, String>,
    pub title: String,
    pub modified: SystemTime,
}

#[derive(Clone, Debug, Default)]
pub struct CreateTicket {
    pub title: String,
    pub description: String,
    pub design: String,
    pub acceptance: String,
    pub issue_type: String,
    pub priority: u8,
    pub assignee: String,
    pub external_ref: String,
    pub parent: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WebTicket {
    pub id: String,
    pub revision: String,
    pub path: String,
    pub status: String,
    pub deps: Vec<String>,
    pub links: Vec<String>,
    pub created: String,
    pub updated: String,
    #[serde(rename = "type")]
    pub issue_type: String,
    pub priority: u8,
    pub assignee: Option<String>,
    pub external_ref: Option<String>,
    pub parent: Option<String>,
    pub tags: Vec<String>,
    pub title: String,
    pub description: String,
    pub raw: String,
    pub blocked: bool,
    pub notes: Vec<WebNote>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WebNote {
    pub timestamp: Option<String>,
    pub text: String,
}

impl Ticket {
    pub fn revision(&self) -> String {
        revision(&self.raw)
    }

    pub fn field(&self, name: &str) -> &str {
        self.fields.get(name).map(String::as_str).unwrap_or("")
    }

    pub fn id(&self) -> &str {
        self.field("id")
    }

    pub fn status(&self) -> &str {
        let status = self.field("status");
        if status.is_empty() { "open" } else { status }
    }

    pub fn priority(&self) -> u8 {
        self.field("priority").parse().unwrap_or(2)
    }

    pub fn array(&self, name: &str) -> Vec<String> {
        parse_array(self.field(name))
    }

    pub fn description(&self) -> String {
        let normalized = self.raw.replace("\r\n", "\n");
        let Some((_, body)) = normalized.split_once("---\n") else {
            return String::new();
        };
        let Some((_, body)) = body.split_once("---\n") else {
            return String::new();
        };
        let body = body.trim_start();
        let body = body
            .strip_prefix(&format!("# {}", self.title))
            .unwrap_or(body)
            .trim_start_matches(['\r', '\n']);
        let end = body.find("\n## ").unwrap_or(body.len());
        body[..end].trim().to_owned()
    }

    pub fn json(&self) -> Value {
        let mut object = Map::new();
        for (key, value) in &self.fields {
            if value.trim().starts_with('[') && value.trim().ends_with(']') {
                object.insert(
                    key.clone(),
                    Value::Array(parse_array(value).into_iter().map(Value::String).collect()),
                );
            } else {
                object.insert(key.clone(), Value::String(value.clone()));
            }
        }
        Value::Object(object)
    }

    pub fn notes(&self) -> Vec<WebNote> {
        let marker = ["", "## Notes"].join("\n");
        let Some((_, section)) = self.raw.split_once(&marker) else {
            return Vec::new();
        };
        let mut notes = Vec::new();
        let mut timestamp = None;
        let mut body = Vec::new();
        for line in section.lines().skip_while(|line| line.trim().is_empty()) {
            if line.starts_with("## ") {
                break;
            }
            if let Some(value) = line
                .trim()
                .strip_prefix("**")
                .and_then(|value| value.strip_suffix("**"))
                .filter(|value| chrono::DateTime::parse_from_rfc3339(value).is_ok())
            {
                push_note(&mut notes, timestamp.take(), &mut body);
                timestamp = Some(value.to_owned());
            } else {
                body.push(line);
            }
        }
        push_note(&mut notes, timestamp, &mut body);
        notes
    }
}

fn push_note(notes: &mut Vec<WebNote>, timestamp: Option<String>, body: &mut Vec<&str>) {
    let text = body.join("\n").trim().to_owned();
    body.clear();
    if !text.is_empty() {
        notes.push(WebNote { timestamp, text });
    }
}

pub fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn revision(raw: &str) -> String {
    format!("{:x}", Sha256::digest(raw.as_bytes()))
}

#[derive(Debug)]
pub struct RevisionConflict {
    pub expected: String,
    pub actual: String,
}

impl std::fmt::Display for RevisionConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "ticket changed on disk (expected revision {}, found {})",
            self.expected, self.actual
        )
    }
}

impl std::error::Error for RevisionConflict {}

#[derive(Default)]
pub struct TicketChanges<'a> {
    pub status: Option<&'a str>,
    pub title: Option<&'a str>,
    pub description: Option<&'a str>,
    pub priority: Option<u8>,
    pub assignee: Option<&'a str>,
    pub tags: Option<&'a [String]>,
    pub parent: Option<Option<&'a str>>,
}

pub async fn find_tickets_dir(write: bool) -> Result<PathBuf> {
    find_tickets_dir_from(None, write).await
}

/// Resolve an explicit tickets-directory override before consulting the
/// environment or searching from the current directory.
pub async fn find_tickets_dir_with_override(
    tickets_dir: Option<&Path>,
    write: bool,
) -> Result<PathBuf> {
    match tickets_dir {
        Some(path) => resolve_tickets_dir_override(path, write).await,
        None => find_tickets_dir(write).await,
    }
}

/// Resolve ticket storage from an explicit project or `.tickets` path, or from
/// the process environment/current directory when `start` is absent.
pub async fn find_tickets_dir_from(start: Option<&Path>, write: bool) -> Result<PathBuf> {
    if start.is_none()
        && let Some(dir) = std::env::var_os("TICKETS_DIR")
    {
        return resolve_tickets_dir_override(Path::new(&dir), write).await;
    }

    let start = match start {
        Some(path) => {
            let path = if path.is_absolute() {
                path.to_owned()
            } else {
                std::env::current_dir()
                    .context("Error: cannot determine current directory")?
                    .join(path)
            };
            if path.file_name().is_some_and(|name| name == ".tickets") {
                if path.is_dir() {
                    return Ok(path);
                }
                bail!(
                    "Error: tickets directory '{}' does not exist",
                    path.display()
                );
            }
            if !path.exists() {
                bail!("Error: workspace '{}' does not exist", path.display());
            }
            if path.exists() && !path.is_dir() {
                bail!("Error: workspace '{}' is not a directory", path.display());
            }
            path
        }
        None => std::env::current_dir().context("Error: cannot determine current directory")?,
    };
    for ancestor in start.ancestors() {
        let candidate = ancestor.join(".tickets");
        if candidate.is_dir() {
            return Ok(candidate);
        }
    }

    if write {
        Ok(start.join(".tickets"))
    } else {
        bail!(
            "Error: no .tickets directory found (searched parent directories)\nRun 'fer init' to initialize, pass --tickets-dir, or set TICKETS_DIR"
        )
    }
}

async fn resolve_tickets_dir_override(path: &Path, write: bool) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .context("Error: cannot determine current directory")?
            .join(path)
    };
    if !write && !path.is_dir() {
        bail!(
            "Error: tickets directory '{}' does not exist",
            path.display()
        );
    }
    Ok(path)
}

pub async fn load_all(dir: &Path) -> Result<Vec<Ticket>> {
    let mut paths = Vec::new();
    let mut entries = match fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).with_context(|| format!("reading {}", dir.display())),
    };
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "md") {
            paths.push(path);
        }
    }
    paths.sort();

    let results = stream::iter(
        paths
            .into_iter()
            .map(|path| async move { load(path).await }),
    )
    .buffer_unordered(32)
    .collect::<Vec<_>>()
    .await;
    let mut tickets = results.into_iter().collect::<Result<Vec<_>>>()?;
    tickets.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(tickets)
}

pub async fn load(path: PathBuf) -> Result<Ticket> {
    let raw = fs::read_to_string(&path)
        .await
        .with_context(|| format!("reading {}", path.display()))?;
    let metadata = fs::metadata(&path).await?;
    parse(
        path,
        raw,
        metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
    )
}

fn parse(path: PathBuf, raw: String, modified: SystemTime) -> Result<Ticket> {
    let mut fields = BTreeMap::new();
    let mut frontmatter = false;
    let mut delimiters = 0;
    let mut title = String::new();

    for raw_line in raw.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line == "---" {
            delimiters += 1;
            frontmatter = delimiters == 1;
            if delimiters == 2 {
                frontmatter = false;
            }
            continue;
        }
        if frontmatter {
            if let Some((key, value)) = line.split_once(':') {
                fields.insert(key.trim().to_owned(), value.trim().to_owned());
            }
        } else if delimiters >= 2
            && title.is_empty()
            && let Some(value) = line.strip_prefix("# ")
        {
            title = value.to_owned();
        }
    }

    if delimiters < 2 {
        bail!(
            "Error: invalid ticket file '{}': missing YAML frontmatter",
            path.display()
        );
    }
    if !fields.contains_key("id") {
        let fallback = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        fields.insert("id".into(), fallback.into());
    }

    Ok(Ticket {
        path,
        raw,
        fields,
        title,
        modified,
    })
}

pub async fn resolve(dir: &Path, pattern: &str) -> Result<Ticket> {
    let pattern = pattern.trim();
    if pattern.is_empty()
        || Path::new(pattern).components().count() != 1
        || pattern.contains(['/', '\\'])
    {
        bail!("Error: invalid ticket ID '{pattern}'");
    }
    let tickets = load_all(dir).await?;
    let mut exact = tickets
        .iter()
        .filter(|ticket| ticket.id() == pattern)
        .cloned()
        .collect::<Vec<_>>();
    match exact.len() {
        1 => return Ok(exact.remove(0)),
        count if count > 1 => bail!("Error: duplicate ticket ID '{pattern}'"),
        _ => {}
    }
    let mut matches = tickets
        .into_iter()
        .filter(|ticket| ticket.id().contains(pattern))
        .collect::<Vec<_>>();
    match matches.len() {
        0 => bail!("Error: ticket '{pattern}' not found"),
        1 => Ok(matches.remove(0)),
        _ => bail!("Error: ambiguous ID '{pattern}' matches multiple tickets"),
    }
}

pub async fn create(dir: &Path, mut input: CreateTicket) -> Result<Ticket> {
    fs::create_dir_all(dir).await?;
    if input.title.trim().is_empty() {
        input.title = "Untitled".into();
    }
    validate_single_line("ticket title", &input.title)?;
    if input.issue_type.is_empty() {
        input.issue_type = "task".into();
    }
    validate_frontmatter_scalar("ticket type", &input.issue_type)?;
    validate_frontmatter_scalar("assignee", &input.assignee)?;
    validate_frontmatter_scalar("external reference", &input.external_ref)?;
    validate_array_values("tag", &input.tags)?;
    if input.priority > 4 {
        bail!("priority must be between 0 and 4");
    }
    if let Some(parent) = input.parent.take() {
        let parent = resolve(dir, &parent).await?;
        ensure_no_parent_cycle(dir, "", parent.id()).await?;
        input.parent = Some(parent.id().to_owned());
    }

    loop {
        let candidate = generate_id(dir)?;
        let path = dir.join(format!("{candidate}.md"));
        let raw = render_new_ticket(&candidate, &input);
        match atomic_create(&path, raw.as_bytes()).await {
            Ok(()) => return load(path).await,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("creating {}", path.display()));
            }
        }
    }
}

fn render_new_ticket(id: &str, input: &CreateTicket) -> String {
    let mut raw = format!(
        "---\nid: {id}\nstatus: open\ndeps: []\nlinks: []\ncreated: {}\ntype: {}\npriority: {}\n",
        now(),
        input.issue_type,
        input.priority
    );
    if !input.assignee.is_empty() {
        raw.push_str(&format!("assignee: {}\n", input.assignee));
    }
    if !input.external_ref.is_empty() {
        raw.push_str(&format!("external-ref: {}\n", input.external_ref));
    }
    if let Some(parent) = input.parent.as_deref() {
        raw.push_str(&format!("parent: {parent}\n"));
    }
    if !input.tags.is_empty() {
        raw.push_str(&format!("tags: [{}]\n", input.tags.join(", ")));
    }
    raw.push_str(&format!("---\n# {}\n\n", input.title.trim()));
    if !input.description.is_empty() {
        raw.push_str(&input.description);
        raw.push_str("\n\n");
    }
    if !input.design.is_empty() {
        raw.push_str(&format!("## Design\n\n{}\n\n", input.design));
    }
    if !input.acceptance.is_empty() {
        raw.push_str(&format!(
            "## Acceptance Criteria\n\n{}\n\n",
            input.acceptance
        ));
    }
    raw
}

fn generate_id(dir: &Path) -> Result<String> {
    let workspace = dir.parent().unwrap_or(dir);
    let directory = workspace
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("fer")
        .to_lowercase();
    let initials = directory
        .split(['-', '_'])
        .filter_map(|segment| segment.chars().next())
        .collect::<String>();
    let prefix = if initials.chars().count() >= 2 {
        initials
    } else {
        directory.chars().take(3).collect()
    };
    let random = rand::rng()
        .sample_iter(Alphanumeric)
        .filter(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        .take(4)
        .map(char::from)
        .collect::<String>();
    Ok(format!("{prefix}-{random}"))
}

pub async fn update_field(ticket: &Ticket, name: &str, value: &str) -> Result<Ticket> {
    match name {
        "status" if !matches!(value, "open" | "in_progress" | "closed") => {
            bail!("invalid status '{value}'")
        }
        "assignee" | "external-ref" | "parent" | "type" => {
            validate_frontmatter_scalar(name, value)?;
        }
        _ => {}
    }
    let (normalized, line_ending) = normalize_line_endings(&ticket.raw);
    let mut output = Vec::new();
    let mut inserted = false;
    let mut in_frontmatter = false;
    for line in normalized.lines() {
        if line == "---" {
            output.push(line.to_owned());
            if !in_frontmatter {
                in_frontmatter = true;
            } else {
                if !inserted {
                    output.insert(1, format!("{name}: {value}"));
                    inserted = true;
                }
                in_frontmatter = false;
            }
            continue;
        }
        if in_frontmatter && line.starts_with(&format!("{name}:")) {
            output.push(format!("{name}: {value}"));
            inserted = true;
        } else {
            output.push(line.to_owned());
        }
    }
    let mut raw = output.join("\n");
    if normalized.ends_with('\n') {
        raw.push('\n');
    }
    let raw = restore_line_endings(raw, line_ending);
    atomic_write(&ticket.path, &raw).await?;
    load(ticket.path.clone()).await
}

pub async fn apply_changes(
    ticket: &Ticket,
    expected_revision: Option<&str>,
    changes: TicketChanges<'_>,
) -> Result<Ticket> {
    if let Some(status) = changes.status
        && !matches!(status, "open" | "in_progress" | "closed")
    {
        bail!("invalid status '{status}'");
    }
    if let Some(title) = changes.title {
        validate_single_line("ticket title", title)?;
    }
    if let Some(assignee) = changes.assignee {
        validate_frontmatter_scalar("assignee", assignee)?;
    }
    if let Some(tags) = changes.tags {
        validate_array_values("tag", tags)?;
    }
    if changes.priority.is_some_and(|priority| priority > 4) {
        bail!("priority must be between 0 and 4");
    }
    let normalized_parent = match changes.parent {
        Some(Some(parent)) if parent.trim().is_empty() => Some(None),
        Some(Some(parent)) => {
            validate_frontmatter_scalar("parent", parent)?;
            let dir = ticket
                .path
                .parent()
                .context("ticket has no parent directory")?;
            let parent = resolve(dir, parent).await?;
            if parent.id() == ticket.id() {
                bail!("a ticket cannot be its own parent");
            }
            ensure_no_parent_cycle(dir, ticket.id(), parent.id()).await?;
            Some(Some(parent.id().to_owned()))
        }
        Some(None) => Some(None),
        None => None,
    };
    let current = load(ticket.path.clone()).await?;
    if let Some(expected) = expected_revision
        && current.revision() != expected
    {
        return Err(RevisionConflict {
            expected: expected.to_owned(),
            actual: current.revision(),
        }
        .into());
    }

    let (mut raw, line_ending) = normalize_line_endings(&current.raw);
    if let Some(status) = changes.status {
        raw = replace_field(&raw, "status", status);
    }
    if let Some(priority) = changes.priority {
        raw = replace_field(&raw, "priority", &priority.to_string());
    }
    if let Some(assignee) = changes.assignee {
        raw = replace_field(&raw, "assignee", assignee);
    }
    if let Some(tags) = changes.tags {
        raw = replace_field(&raw, "tags", &format!("[{}]", tags.join(", ")));
    }
    if let Some(parent) = normalized_parent {
        raw = replace_field(&raw, "parent", parent.as_deref().unwrap_or_default());
    }
    if let Some(title) = changes.title.filter(|title| !title.trim().is_empty()) {
        raw = replace_title(&raw, title.trim());
    }
    if let Some(description) = changes.description {
        raw = replace_description(&raw, description);
    }
    let raw = restore_line_endings(raw, line_ending);

    atomic_write(&current.path, &raw).await?;
    load(current.path).await
}

fn replace_field(raw: &str, name: &str, value: &str) -> String {
    let mut output = Vec::new();
    let mut inserted = false;
    let mut in_frontmatter = false;
    let mut delimiters = 0;
    for line in raw.lines() {
        if line == "---" {
            if delimiters == 1 && !inserted && !value.is_empty() {
                output.push(format!("{name}: {value}"));
                inserted = true;
            }
            output.push(line.to_owned());
            delimiters += 1;
            in_frontmatter = delimiters == 1;
            continue;
        }
        if in_frontmatter && line.starts_with(&format!("{name}:")) {
            if !value.is_empty() {
                output.push(format!("{name}: {value}"));
            }
            inserted = true;
        } else {
            output.push(line.to_owned());
        }
    }
    preserve_trailing_newline(raw, output.join("\n"))
}

pub async fn append_note_checked(
    ticket: &Ticket,
    expected_revision: Option<&str>,
    note: &str,
) -> Result<Ticket> {
    let current = load(ticket.path.clone()).await?;
    if let Some(expected) = expected_revision
        && current.revision() != expected
    {
        return Err(RevisionConflict {
            expected: expected.to_owned(),
            actual: current.revision(),
        }
        .into());
    }
    let (mut raw, line_ending) = normalize_line_endings(&current.raw);
    if !raw.lines().any(|line| line == "## Notes") {
        raw.push_str("\n## Notes\n");
    }
    raw.push_str(&format!("\n**{}**\n\n{}\n", now(), note));
    let raw = restore_line_endings(raw, line_ending);
    atomic_write(&current.path, &raw).await?;
    load(current.path).await
}

pub async fn edit_note_checked(
    ticket: &Ticket,
    expected_revision: Option<&str>,
    index: usize,
    text: &str,
) -> Result<Ticket> {
    if text.trim().is_empty() {
        bail!("note text cannot be empty");
    }
    mutate_note_checked(ticket, expected_revision, index, Some(text)).await
}

pub async fn delete_note_checked(
    ticket: &Ticket,
    expected_revision: Option<&str>,
    index: usize,
) -> Result<Ticket> {
    mutate_note_checked(ticket, expected_revision, index, None).await
}

async fn mutate_note_checked(
    ticket: &Ticket,
    expected_revision: Option<&str>,
    index: usize,
    replacement: Option<&str>,
) -> Result<Ticket> {
    let current = load(ticket.path.clone()).await?;
    if let Some(expected) = expected_revision
        && current.revision() != expected
    {
        return Err(RevisionConflict {
            expected: expected.to_owned(),
            actual: current.revision(),
        }
        .into());
    }
    let mut notes = current.notes();
    if index >= notes.len() {
        bail!("note {} does not exist", index + 1);
    }
    if let Some(text) = replacement {
        notes[index].text = text.trim().to_owned();
    } else {
        notes.remove(index);
    }
    let raw = replace_notes(&current.raw, &notes)?;
    atomic_write(&current.path, &raw).await?;
    load(current.path).await
}

fn replace_notes(raw: &str, notes: &[WebNote]) -> Result<String> {
    let line_ending = if raw.contains("\r\n") { "\r\n" } else { "\n" };
    let normalized = raw.replace("\r\n", "\n");
    let marker = "\n## Notes";
    let marker_start = normalized
        .find(marker)
        .context("ticket does not contain a Notes section")?;
    let content_start = marker_start + marker.len();
    let rest = &normalized[content_start..];
    let suffix_start = rest
        .find("\n## ")
        .map_or(normalized.len(), |offset| content_start + offset);
    let prefix = normalized[..marker_start].trim_end_matches('\n');
    let suffix = normalized[suffix_start..].trim_start_matches('\n');
    let mut output = prefix.to_owned();
    if !notes.is_empty() {
        output.push_str("\n\n## Notes\n");
        for note in notes {
            output.push('\n');
            if let Some(timestamp) = note.timestamp.as_deref() {
                output.push_str(&format!("**{timestamp}**\n\n"));
            }
            output.push_str(note.text.trim());
            output.push('\n');
        }
    }
    if !suffix.is_empty() {
        output.push_str("\n\n");
        output.push_str(suffix);
    }
    if raw.ends_with('\n') && !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(if line_ending == "\r\n" {
        output.replace('\n', "\r\n")
    } else {
        output
    })
}

fn replace_title(raw: &str, title: &str) -> String {
    let mut output = Vec::new();
    let mut delimiters = 0;
    let mut replaced = false;
    for line in raw.lines() {
        if line == "---" {
            delimiters += 1;
        }
        if delimiters >= 2 && !replaced && line.starts_with("# ") {
            output.push(format!("# {title}"));
            replaced = true;
        } else {
            output.push(line.to_owned());
        }
    }
    preserve_trailing_newline(raw, output.join("\n"))
}

fn replace_description(raw: &str, description: &str) -> String {
    let mut delimiters = 0;
    let mut heading_end = None;
    for (offset, line) in raw.match_indices('\n') {
        let line_start = raw[..offset].rfind('\n').map_or(0, |index| index + 1);
        let value = &raw[line_start..offset];
        if value == "---" {
            delimiters += 1;
        } else if delimiters >= 2 && value.starts_with("# ") {
            heading_end = Some(offset + line.len());
            break;
        }
    }
    let Some(start) = heading_end else {
        return raw.to_owned();
    };
    let body = &raw[start..];
    let reserved = ["## Design", "## Acceptance Criteria", "## Notes"]
        .into_iter()
        .filter_map(|heading| body.find(&format!("\n{heading}")))
        .min()
        .unwrap_or(body.len());
    let suffix = body[reserved..].trim_start_matches('\n');
    let mut output = raw[..start].trim_end_matches('\n').to_owned();
    output.push_str("\n\n");
    output.push_str(description.trim());
    if !suffix.is_empty() {
        output.push_str("\n\n");
        output.push_str(suffix);
    }
    output.push('\n');
    output
}

fn preserve_trailing_newline(original: &str, mut output: String) -> String {
    if original.ends_with('\n') {
        output.push('\n');
    }
    output
}

pub async fn append_note(ticket: &Ticket, note: &str) -> Result<Ticket> {
    let (mut raw, line_ending) = normalize_line_endings(&ticket.raw);
    if !raw.lines().any(|line| line == "## Notes") {
        raw.push_str("\n## Notes\n");
    }
    raw.push_str(&format!("\n**{}**\n\n{}\n", now(), note));
    let raw = restore_line_endings(raw, line_ending);
    atomic_write(&ticket.path, &raw).await?;
    load(ticket.path.clone()).await
}

pub async fn save_attachment(
    tickets_dir: &Path,
    ticket_id: &str,
    file_name: &str,
    bytes: &[u8],
) -> Result<PathBuf> {
    resolve(tickets_dir, ticket_id).await?;
    let ticket_id = safe_path_component(ticket_id, "ticket ID")?;
    let file_name = safe_attachment_name(file_name)?;
    let directory = tickets_dir.join("attachments").join(ticket_id);
    fs::create_dir_all(&directory).await?;
    ensure_attachment_directory(tickets_dir, &directory).await?;
    let mut candidate = file_name.clone();
    let mut index = 2;
    loop {
        let path = directory.join(&candidate);
        match atomic_create(&path, bytes).await {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(error).with_context(|| format!("saving {}", path.display()));
            }
        }
        let path = Path::new(&file_name);
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("file");
        let extension = path.extension().and_then(|value| value.to_str());
        candidate = match extension {
            Some(extension) => format!("{stem}-{index}.{extension}"),
            None => format!("{stem}-{index}"),
        };
        index += 1;
    }
}

pub fn attachment_path(tickets_dir: &Path, ticket_id: &str, file_name: &str) -> Result<PathBuf> {
    let ticket_id = safe_path_component(ticket_id, "ticket ID")?;
    let file_name = safe_attachment_name(file_name)?;
    Ok(tickets_dir
        .join("attachments")
        .join(ticket_id)
        .join(file_name))
}

pub async fn read_attachment(
    tickets_dir: &Path,
    ticket_id: &str,
    file_name: &str,
) -> Result<(PathBuf, Vec<u8>)> {
    let path = attachment_path(tickets_dir, ticket_id, file_name)?;
    let canonical_root = fs::canonicalize(tickets_dir).await?;
    let canonical_path = fs::canonicalize(&path).await?;
    if !canonical_path.starts_with(canonical_root.join("attachments")) {
        bail!("Error: attachment path escapes the ticket directory");
    }
    let metadata = fs::symlink_metadata(&path).await?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        bail!("Error: attachment is not a regular file");
    }
    let bytes = fs::read(&canonical_path).await?;
    Ok((canonical_path, bytes))
}

async fn ensure_attachment_directory(tickets_dir: &Path, directory: &Path) -> Result<()> {
    let canonical_root = fs::canonicalize(tickets_dir).await?;
    let canonical_directory = fs::canonicalize(directory).await?;
    if !canonical_directory.starts_with(canonical_root.join("attachments")) {
        bail!("Error: attachment directory escapes the ticket directory");
    }
    Ok(())
}

fn safe_attachment_name(value: &str) -> Result<String> {
    let value = safe_path_component(value, "attachment name")?;
    let mut sanitized = String::with_capacity(value.len());
    let mut separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
            if character == '-' && separator {
                continue;
            }
            sanitized.push(character);
            separator = character == '-';
        } else if !separator {
            sanitized.push('-');
            separator = true;
        }
    }
    let sanitized = sanitized.trim_matches(['.', '-']).replace("-.", ".");
    if sanitized.is_empty() {
        bail!("Error: invalid attachment name");
    }
    Ok(sanitized)
}

fn safe_path_component<'a>(value: &'a str, label: &str) -> Result<&'a str> {
    let value = value.trim();
    if value.is_empty()
        || Path::new(value).components().count() != 1
        || value.contains(['/', '\\'])
        || value == "."
        || value == ".."
    {
        bail!("Error: invalid {label} '{value}'");
    }
    Ok(value)
}

pub async fn set_array(ticket: &Ticket, name: &str, values: &[String]) -> Result<Ticket> {
    validate_array_values(name, values)?;
    update_field(ticket, name, &format!("[{}]", values.join(", "))).await
}

fn validate_single_line(label: &str, value: &str) -> Result<()> {
    if value.contains(['\r', '\n']) {
        bail!("{label} must be a single line");
    }
    Ok(())
}

fn validate_frontmatter_scalar(label: &str, value: &str) -> Result<()> {
    validate_single_line(label, value)
}

fn validate_array_values(label: &str, values: &[String]) -> Result<()> {
    for value in values {
        validate_single_line(label, value)?;
        if value.contains([',', '[', ']']) {
            bail!("{label} cannot contain commas or brackets");
        }
    }
    Ok(())
}

fn normalize_line_endings(value: &str) -> (String, &'static str) {
    if value.contains("\r\n") {
        (value.replace("\r\n", "\n"), "\r\n")
    } else {
        (value.to_owned(), "\n")
    }
}

fn restore_line_endings(value: String, line_ending: &str) -> String {
    if line_ending == "\r\n" {
        value.replace('\n', "\r\n")
    } else {
        value
    }
}

async fn ensure_no_parent_cycle(dir: &Path, ticket_id: &str, parent_id: &str) -> Result<()> {
    let tickets = ticket_map(&load_all(dir).await?);
    let mut current = parent_id;
    let mut visited = HashSet::new();
    while !current.is_empty() && visited.insert(current.to_owned()) {
        if !ticket_id.is_empty() && current == ticket_id {
            bail!("parent relationship would create a cycle");
        }
        let Some(ticket) = tickets.get(current) else {
            break;
        };
        current = ticket.field("parent");
    }
    if !current.is_empty() {
        bail!("parent relationship contains a cycle");
    }
    Ok(())
}

pub fn parse_array(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub fn ticket_map(tickets: &[Ticket]) -> HashMap<String, Ticket> {
    tickets
        .iter()
        .map(|ticket| (ticket.id().to_owned(), ticket.clone()))
        .collect()
}

pub fn active(ticket: &Ticket) -> bool {
    matches!(ticket.status(), "open" | "in_progress")
}

pub fn unresolved(ticket: &Ticket, tickets: &HashMap<String, Ticket>) -> Vec<String> {
    ticket
        .array("deps")
        .into_iter()
        .filter(|id| {
            tickets
                .get(id)
                .is_none_or(|ticket| ticket.status() != "closed")
        })
        .collect()
}

pub fn to_web(ticket: &Ticket, all: &HashMap<String, Ticket>) -> WebTicket {
    let updated =
        chrono::DateTime::<Utc>::from(ticket.modified).to_rfc3339_opts(SecondsFormat::Secs, true);
    WebTicket {
        id: ticket.id().to_owned(),
        revision: ticket.revision(),
        path: ticket.path.display().to_string(),
        status: non_empty(ticket.status(), "open"),
        deps: ticket.array("deps"),
        links: ticket.array("links"),
        created: non_empty(ticket.field("created"), &updated),
        updated,
        issue_type: non_empty(ticket.field("type"), "task"),
        priority: ticket.priority(),
        assignee: optional(ticket.field("assignee")),
        external_ref: optional(ticket.field("external-ref")),
        parent: optional(ticket.field("parent")),
        tags: ticket.array("tags"),
        title: ticket.title.clone(),
        description: ticket.description(),
        raw: ticket.raw.clone(),
        blocked: !unresolved(ticket, all).is_empty(),
        notes: ticket.notes(),
    }
}

fn non_empty(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_owned()
    } else {
        value.to_owned()
    }
}

fn optional(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

pub fn detect_cycles(tickets: &HashMap<String, Ticket>) -> Vec<Vec<String>> {
    fn visit(
        node: &str,
        tickets: &HashMap<String, Ticket>,
        state: &mut HashMap<String, u8>,
        stack: &mut Vec<String>,
        found: &mut Vec<Vec<String>>,
        seen: &mut HashSet<String>,
    ) {
        if tickets
            .get(node)
            .is_none_or(|ticket| ticket.status() == "closed")
        {
            return;
        }
        if state.get(node) == Some(&1) {
            if let Some(start) = stack.iter().position(|id| id == node) {
                let cycle = stack[start..].to_vec();
                let mut normalized = cycle.clone();
                normalized.sort();
                let key = normalized.join("\0");
                if seen.insert(key) {
                    found.push(cycle);
                }
            }
            return;
        }
        if state.get(node) == Some(&2) {
            return;
        }
        state.insert(node.into(), 1);
        stack.push(node.into());
        if let Some(ticket) = tickets.get(node) {
            for dependency in ticket.array("deps") {
                visit(&dependency, tickets, state, stack, found, seen);
            }
        }
        stack.pop();
        state.insert(node.into(), 2);
    }

    let mut ids = tickets.keys().cloned().collect::<Vec<_>>();
    ids.sort();
    let mut state = HashMap::new();
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for id in ids {
        visit(
            &id,
            tickets,
            &mut state,
            &mut Vec::new(),
            &mut found,
            &mut seen,
        );
    }
    found
}

async fn atomic_write(path: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
    let nonce: u64 = rand::random();
    let temporary = path.with_extension(format!("md.tmp.{}.{nonce}", std::process::id()));
    if let Err(error) = fs::write(&temporary, contents).await {
        let _ = fs::remove_file(&temporary).await;
        return Err(error).with_context(|| format!("writing {}", temporary.display()));
    }
    if cfg!(windows)
        && fs::try_exists(path).await?
        && let Err(error) = fs::remove_file(path).await
    {
        let _ = fs::remove_file(&temporary).await;
        return Err(error).with_context(|| format!("replacing {}", path.display()));
    }
    match fs::rename(&temporary, path).await {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temporary).await;
            Err(error).with_context(|| format!("replacing {}", path.display()))
        }
    }
}

async fn atomic_create(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let nonce: u64 = rand::random();
    let temporary = path.with_extension(format!("tmp.{}.{nonce}", std::process::id()));
    if let Err(error) = fs::write(&temporary, contents).await {
        let _ = fs::remove_file(&temporary).await;
        return Err(error);
    }
    let result = fs::hard_link(&temporary, path).await;
    let _ = fs::remove_file(&temporary).await;
    result
}

/// Watch only direct Markdown children of the resolved `.tickets` directory.
/// Nested directories and non-ticket temporary files never produce updates.
pub async fn watch_ticket_files(
    dir: &Path,
) -> Result<(RecommendedWatcher, mpsc::UnboundedReceiver<()>)> {
    let watched_dir = fs::canonicalize(dir).await.with_context(|| {
        format!(
            "Error: cannot watch tickets directory '{}'; check that it exists and is readable",
            dir.display()
        )
    })?;
    let callback_dir = watched_dir.clone();
    let initial_fingerprint = direct_ticket_fingerprint(&watched_dir).await;
    let (raw_sender, mut raw_receiver) = mpsc::unbounded_channel();
    let (sender, receiver) = mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else {
            return;
        };
        let relevant = event.paths.iter().any(|path| {
            path == callback_dir.as_path()
                || (path.parent() == Some(callback_dir.as_path())
                    && path.extension().is_some_and(|extension| extension == "md"))
        });
        if relevant {
            let _ = raw_sender.send(());
        }
    })?;
    watcher.watch(&watched_dir, RecursiveMode::NonRecursive)?;
    tokio::spawn(async move {
        let mut previous = initial_fingerprint;
        while raw_receiver.recv().await.is_some() {
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            while raw_receiver.try_recv().is_ok() {}
            let current = direct_ticket_fingerprint(&watched_dir).await;
            if current != previous {
                previous = current;
                let _ = sender.send(());
            }
        }
    });
    Ok((watcher, receiver))
}

async fn direct_ticket_fingerprint(dir: &Path) -> String {
    let mut files = Vec::new();
    let Ok(mut entries) = fs::read_dir(dir).await else {
        return String::new();
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "md") {
            files.push(path);
        }
    }
    files.sort();
    let mut hasher = Sha256::new();
    for path in files {
        hasher.update(path.file_name().unwrap_or_default().as_encoded_bytes());
        if let Ok(contents) = fs::read(path).await {
            hasher.update(contents);
        }
    }
    format!("{:x}", hasher.finalize())
}

pub fn required_ticket<'a>(map: &'a HashMap<String, Ticket>, id: &str) -> Result<&'a Ticket> {
    map.get(id)
        .ok_or_else(|| anyhow!("Error: ticket '{id}' not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_use_the_workspace_name_instead_of_the_process_cwd() {
        let id = generate_id(Path::new("/tmp/my-project/.tickets")).unwrap();
        assert!(id.starts_with("mp-"), "unexpected id: {id}");
    }

    #[test]
    fn parses_ticket_compatible_frontmatter_and_body() {
        let raw = "---\nid: demo-1234\nstatus: open\ndeps: [demo-0001, demo-0002]\nlinks: []\ncreated: 2026-01-01T00:00:00Z\ntype: task\npriority: 1\ntags: [rust, cli]\n---\n# Ship it\n\nThe description.\n\n## Design\n\nAsync.\n";
        let ticket = parse(
            PathBuf::from("demo-1234.md"),
            raw.into(),
            SystemTime::UNIX_EPOCH,
        )
        .unwrap();
        assert_eq!(ticket.id(), "demo-1234");
        assert_eq!(ticket.title, "Ship it");
        assert_eq!(ticket.array("deps"), ["demo-0001", "demo-0002"]);
        assert_eq!(ticket.array("tags"), ["rust", "cli"]);
        assert_eq!(ticket.description(), "The description.");
    }

    #[test]
    fn parses_crlf_ticket_title_description_and_notes() {
        let raw = "---\r\nid: demo\r\nstatus: open\r\n---\r\n# Demo\r\n\r\nDescription.\r\n\r\n## Notes\r\n\r\n**2026-07-12T01:02:03Z**\r\n\r\nA note.\r\n";
        let ticket = parse(PathBuf::from("demo.md"), raw.into(), SystemTime::UNIX_EPOCH).unwrap();
        assert_eq!(ticket.title, "Demo");
        assert_eq!(ticket.description(), "Description.");
    }

    #[test]
    fn parses_timestamped_notes_for_ui_projection() {
        let lines = [
            "---",
            "id: demo",
            "status: open",
            "---",
            "# Demo",
            "",
            "Description",
            "",
            "## Notes",
            "",
            "**2026-07-12T01:02:03Z**",
            "",
            "First note.",
            "",
            "**2026-07-12T02:03:04Z**",
            "",
            "Second note.",
            "",
        ];
        let ticket = parse(
            PathBuf::from("demo.md"),
            lines.join("\n"),
            SystemTime::UNIX_EPOCH,
        )
        .unwrap();
        let notes = ticket.notes();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].timestamp.as_deref(), Some("2026-07-12T01:02:03Z"));
        assert_eq!(notes[1].text, "Second note.");
    }

    #[test]
    fn rejects_frontmatter_and_heading_injection() {
        let mut input = CreateTicket {
            title: "Safe\nstatus: closed".into(),
            ..CreateTicket::default()
        };
        assert!(validate_single_line("ticket title", &input.title).is_err());
        input.title = "Safe".into();
        input.assignee = "Ada\nstatus: closed".into();
        assert!(validate_frontmatter_scalar("assignee", &input.assignee).is_err());
        assert!(validate_array_values("tag", &["safe, injected".into()]).is_err());
        assert!(validate_array_values("tag", &["safe\nstatus: closed".into()]).is_err());
    }

    #[test]
    fn renders_compatible_new_ticket_frontmatter() {
        let input = CreateTicket {
            title: "Ship it".into(),
            issue_type: "task".into(),
            priority: 1,
            assignee: "Ada".into(),
            tags: vec!["rust".into(), "cli".into()],
            ..CreateTicket::default()
        };
        let raw = render_new_ticket("fer-1234", &input);
        assert!(raw.contains("\nid: fer-1234\n"));
        assert!(raw.contains("\ntags: [rust, cli]\n"));
        assert!(raw.contains("\n# Ship it\n"));
    }

    #[test]
    fn unresolved_includes_missing_and_active_dependencies() {
        let base = |id: &str, status: &str, deps: &str| {
            let raw = format!(
                "---\nid: {id}\nstatus: {status}\ndeps: [{deps}]\nlinks: []\n---\n# {id}\n"
            );
            parse(
                PathBuf::from(format!("{id}.md")),
                raw,
                SystemTime::UNIX_EPOCH,
            )
            .unwrap()
        };
        let main = base("main", "open", "done, active, missing");
        let tickets = ticket_map(&[
            main.clone(),
            base("done", "closed", ""),
            base("active", "open", ""),
        ]);
        assert_eq!(unresolved(&main, &tickets), ["active", "missing"]);
    }

    #[test]
    fn detects_active_cycles_but_ignores_closed_tickets() {
        let base = |id: &str, status: &str, deps: &str| {
            parse(
                PathBuf::from(format!("{id}.md")),
                format!(
                    "---\nid: {id}\nstatus: {status}\ndeps: [{deps}]\nlinks: []\n---\n# {id}\n"
                ),
                SystemTime::UNIX_EPOCH,
            )
            .unwrap()
        };
        let map = ticket_map(&[
            base("one", "open", "two"),
            base("two", "in_progress", "one"),
            base("closed-a", "closed", "closed-b"),
            base("closed-b", "open", "closed-a"),
        ]);
        let cycles = detect_cycles(&map);
        assert_eq!(cycles.len(), 1);
        assert_eq!(
            cycles[0].iter().cloned().collect::<HashSet<_>>(),
            HashSet::from(["one".into(), "two".into()])
        );
    }

    #[tokio::test]
    async fn resolves_explicit_project_and_tickets_paths() {
        let root = std::env::temp_dir().join(format!("ferricket-path-test-{}", std::process::id()));
        let project = root.join("project");
        let nested = project.join("src");
        fs::create_dir_all(project.join(".tickets")).await.unwrap();
        fs::create_dir_all(&nested).await.unwrap();

        assert_eq!(
            find_tickets_dir_from(Some(&project), false).await.unwrap(),
            project.join(".tickets")
        );
        assert!(
            find_tickets_dir_from(Some(&root.join("missing")), true)
                .await
                .is_err()
        );
        assert!(
            find_tickets_dir_from(Some(&root.join("missing").join(".tickets")), true)
                .await
                .is_err()
        );
        assert_eq!(
            find_tickets_dir_from(Some(&nested), false).await.unwrap(),
            project.join(".tickets")
        );
        assert_eq!(
            find_tickets_dir_from(Some(&project.join(".tickets")), false)
                .await
                .unwrap(),
            project.join(".tickets")
        );

        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn resolves_explicit_tickets_directory_override() {
        let root = std::env::temp_dir().join(format!(
            "ferricket-override-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let tickets = root.join("custom-tickets");
        fs::create_dir_all(&tickets).await.unwrap();

        assert_eq!(
            find_tickets_dir_with_override(Some(&tickets), false)
                .await
                .unwrap(),
            tickets
        );
        let missing = root.join("missing");
        assert!(
            find_tickets_dir_with_override(Some(&missing), false)
                .await
                .is_err()
        );
        assert_eq!(
            find_tickets_dir_with_override(Some(&missing), true)
                .await
                .unwrap(),
            missing
        );

        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn read_only_workspace_resolution_requires_initialization() {
        let root = std::env::temp_dir().join(format!(
            "ferricket-uninitialized-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&root).await.unwrap();

        let error = find_tickets_dir_from(Some(&root), false)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("no .tickets directory found"));
        assert!(error.contains("fer init"));
        assert_eq!(
            find_tickets_dir_from(Some(&root), true).await.unwrap(),
            root.join(".tickets")
        );

        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn checked_changes_preserve_reserved_sections_and_reject_stale_revisions() {
        let root = std::env::temp_dir().join(format!(
            "ferricket-revision-test-{}-{}",
            std::process::id(),
            now().replace([':', '-'], "")
        ));
        fs::create_dir_all(&root).await.unwrap();
        let path = root.join("demo.md");
        let raw = "---\nid: demo\nstatus: open\ndeps: []\nlinks: []\npriority: 2\n---\n# Demo\n\nOld description.\n\n## Design\n\nKeep this.\n\n## Notes\n\n**2026-01-01T00:00:00Z**\n\nKeep note.\n";
        fs::write(&path, raw).await.unwrap();
        let ticket = load(path.clone()).await.unwrap();
        let old_revision = ticket.revision();
        let changed = apply_changes(
            &ticket,
            Some(&old_revision),
            TicketChanges {
                description: Some("New **Markdown** description."),
                status: Some("in_progress"),
                tags: Some(&["rust".into(), "ui".into()]),
                ..TicketChanges::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(changed.description(), "New **Markdown** description.");
        assert_eq!(changed.status(), "in_progress");
        assert_eq!(changed.array("tags"), ["rust", "ui"]);
        assert!(changed.raw.contains("## Design\n\nKeep this."));
        assert!(changed.raw.contains("## Notes"));
        assert!(
            apply_changes(
                &changed,
                Some(&old_revision),
                TicketChanges {
                    status: Some("closed"),
                    ..TicketChanges::default()
                },
            )
            .await
            .unwrap_err()
            .downcast_ref::<RevisionConflict>()
            .is_some()
        );
        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn parent_updates_resolve_partial_ids_and_reject_cycles() {
        let root = std::env::temp_dir().join(format!(
            "ferricket-parent-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&root).await.unwrap();
        for (id, parent) in [("root-1111", ""), ("child-2222", "root-1111")] {
            let parent = if parent.is_empty() {
                String::new()
            } else {
                format!("parent: {parent}\n")
            };
            fs::write(
                root.join(format!("{id}.md")),
                format!("---\nid: {id}\nstatus: open\n{parent}---\n# {id}\n"),
            )
            .await
            .unwrap();
        }

        let root_ticket = resolve(&root, "root").await.unwrap();
        let error = apply_changes(
            &root_ticket,
            Some(&root_ticket.revision()),
            TicketChanges {
                parent: Some(Some("child")),
                ..TicketChanges::default()
            },
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("cycle"));

        let child = resolve(&root, "child").await.unwrap();
        let changed = apply_changes(
            &child,
            Some(&child.revision()),
            TicketChanges {
                parent: Some(Some("root")),
                ..TicketChanges::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(changed.field("parent"), "root-1111");

        let cleared = apply_changes(
            &changed,
            Some(&changed.revision()),
            TicketChanges {
                parent: Some(Some("")),
                ..TicketChanges::default()
            },
        )
        .await
        .unwrap();
        assert!(cleared.field("parent").is_empty());
        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn checked_note_edits_and_deletes_preserve_the_ticket_and_reject_stale_revisions() {
        let root = std::env::temp_dir().join(format!(
            "ferricket-note-mutations-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&root).await.unwrap();
        let path = root.join("demo.md");
        let raw = "---\nid: demo\nstatus: open\ndeps: []\nlinks: []\npriority: 2\n---\n# Demo\n\nDescription.\n\n## Design\n\nKeep this.\n\n## Notes\n\n**2026-01-01T00:00:00Z**\n\nFirst note.\n\n**2026-01-02T00:00:00Z**\n\nSecond note.\n";
        fs::write(&path, raw).await.unwrap();
        let original = load(path.clone()).await.unwrap();
        let edited = edit_note_checked(&original, Some(&original.revision()), 0, "Edited note.")
            .await
            .unwrap();
        assert_eq!(
            edited.notes(),
            vec![
                WebNote {
                    timestamp: Some("2026-01-01T00:00:00Z".into()),
                    text: "Edited note.".into(),
                },
                WebNote {
                    timestamp: Some("2026-01-02T00:00:00Z".into()),
                    text: "Second note.".into(),
                },
            ]
        );
        assert!(edited.raw.contains("## Design\n\nKeep this."));
        let error = delete_note_checked(&edited, Some(&original.revision()), 1)
            .await
            .unwrap_err();
        assert!(error.downcast_ref::<RevisionConflict>().is_some());
        let deleted = delete_note_checked(&edited, Some(&edited.revision()), 1)
            .await
            .unwrap();
        assert_eq!(deleted.notes().len(), 1);
        assert_eq!(deleted.notes()[0].text, "Edited note.");
        let without_notes = delete_note_checked(&deleted, Some(&deleted.revision()), 0)
            .await
            .unwrap();
        assert!(without_notes.notes().is_empty());
        assert!(!without_notes.raw.contains("## Notes"));
        assert!(without_notes.raw.contains("## Design\n\nKeep this."));
        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn checked_note_edits_preserve_crlf_line_endings() {
        let root = std::env::temp_dir().join(format!(
            "ferricket-crlf-note-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&root).await.unwrap();
        let path = root.join("demo.md");
        let raw = "---\r\nid: demo\r\nstatus: open\r\n---\r\n# Demo\r\n\r\nDescription.\r\n\r\n## Notes\r\n\r\n**2026-01-01T00:00:00Z**\r\n\r\nFirst note.\r\n";
        fs::write(&path, raw).await.unwrap();
        let original = load(path.clone()).await.unwrap();
        let edited = edit_note_checked(&original, Some(&original.revision()), 0, "Updated.")
            .await
            .unwrap();
        assert!(edited.raw.contains("\r\n"));
        assert!(!edited.raw.replace("\r\n", "").contains('\n'));
        assert_eq!(edited.notes()[0].text, "Updated.");
        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn checked_ticket_changes_and_note_appends_preserve_crlf_line_endings() {
        let root = std::env::temp_dir().join(format!(
            "ferricket-crlf-change-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&root).await.unwrap();
        let path = root.join("demo.md");
        let raw = "---\r\nid: demo\r\nstatus: open\r\npriority: 2\r\n---\r\n# Demo\r\n\r\nOld.\r\n";
        fs::write(&path, raw).await.unwrap();
        let original = load(path.clone()).await.unwrap();
        let changed = apply_changes(
            &original,
            Some(&original.revision()),
            TicketChanges {
                status: Some("in_progress"),
                title: Some("Changed"),
                description: Some("New."),
                ..TicketChanges::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(changed.status(), "in_progress");
        assert_eq!(changed.title, "Changed");
        assert_eq!(changed.description(), "New.");
        let noted = append_note_checked(&changed, Some(&changed.revision()), "Note.")
            .await
            .unwrap();
        assert!(noted.raw.contains("\r\n## Notes\r\n"));
        assert!(!noted.raw.replace("\r\n", "").contains('\n'));
        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn watcher_reports_direct_markdown_changes() {
        let root =
            std::env::temp_dir().join(format!("ferricket-watch-test-{}", std::process::id()));
        let nested = root.join("nested");
        fs::create_dir_all(&nested).await.unwrap();
        fs::write(root.join("ticket.md"), "first").await.unwrap();
        let (_watcher, mut changes) = watch_ticket_files(&root).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        fs::write(root.join("ticket.md"), "second").await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), changes.recv())
            .await
            .unwrap()
            .unwrap();
        while changes.try_recv().is_ok() {}
        fs::write(nested.join("ignored.md"), "nested")
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(500), changes.recv())
                .await
                .is_err()
        );
        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn resolve_rejects_path_components() {
        let root = std::env::temp_dir().join(format!("ferricket-id-test-{}", std::process::id()));
        fs::create_dir_all(&root).await.unwrap();
        assert!(
            resolve(&root, "../outside")
                .await
                .unwrap_err()
                .to_string()
                .contains("invalid ticket ID")
        );
        assert!(
            resolve(&root, "nested/ticket")
                .await
                .unwrap_err()
                .to_string()
                .contains("invalid ticket ID")
        );
        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn resolve_uses_frontmatter_ids_and_rejects_duplicates() {
        let root = std::env::temp_dir().join(format!(
            "ferricket-resolve-id-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&root).await.unwrap();
        fs::write(
            root.join("renamed.md"),
            "---\nid: exact-id\nstatus: open\n---\n# Exact\n",
        )
        .await
        .unwrap();
        assert_eq!(resolve(&root, "exact-id").await.unwrap().title, "Exact");
        fs::write(
            root.join("duplicate.md"),
            "---\nid: exact-id\nstatus: open\n---\n# Duplicate\n",
        )
        .await
        .unwrap();
        assert!(
            resolve(&root, "exact-id")
                .await
                .unwrap_err()
                .to_string()
                .contains("duplicate ticket ID")
        );
        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn attachments_are_sanitized_and_collision_safe() {
        let root = std::env::temp_dir().join(format!(
            "ferricket-attachment-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&root).await.unwrap();
        fs::write(
            root.join("fer-123.md"),
            "---\nid: fer-123\nstatus: open\ndeps: []\nlinks: []\n---\n# Attachment test\n",
        )
        .await
        .unwrap();

        assert!(
            save_attachment(&root, "fer-123", "../secret", b"no")
                .await
                .is_err()
        );
        assert!(attachment_path(&root, "fer-123", "../secret").is_err());
        assert!(attachment_path(&root, "../outside", "file.txt").is_err());

        let first = save_attachment(&root, "fer-123", "Design notes (final).txt", b"one")
            .await
            .unwrap();
        let second = save_attachment(&root, "fer-123", "Design notes (final).txt", b"two")
            .await
            .unwrap();

        assert_eq!(first.file_name().unwrap(), "Design-notes-final.txt");
        assert_eq!(second.file_name().unwrap(), "Design-notes-final-2.txt");
        assert_eq!(fs::read(first).await.unwrap(), b"one");
        assert_eq!(fs::read(second).await.unwrap(), b"two");
        fs::remove_dir_all(root).await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn attachment_reads_reject_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "ferricket-attachment-link-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let outside = root.with_extension("outside");
        fs::create_dir_all(root.join("attachments/fer-123"))
            .await
            .unwrap();
        fs::write(&outside, b"secret").await.unwrap();
        symlink(&outside, root.join("attachments/fer-123/secret.txt")).unwrap();

        assert!(
            read_attachment(&root, "fer-123", "secret.txt")
                .await
                .is_err()
        );

        fs::remove_dir_all(root).await.unwrap();
        fs::remove_file(outside).await.unwrap();
    }
}
