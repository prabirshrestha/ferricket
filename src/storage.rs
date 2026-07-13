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

#[derive(Clone, Debug, Serialize)]
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

/// Resolve ticket storage from an explicit project or `.tickets` path, or from
/// the process environment/current directory when `start` is absent.
pub async fn find_tickets_dir_from(start: Option<&Path>, write: bool) -> Result<PathBuf> {
    if start.is_none()
        && let Some(dir) = std::env::var_os("TICKETS_DIR")
    {
        let path = PathBuf::from(dir);
        let path = if path.is_absolute() {
            path
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
        return Ok(path);
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
            "Error: no .tickets directory found (searched parent directories)\nRun 'fer init' to initialize, or set TICKETS_DIR env var"
        )
    }
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
    let exact = dir.join(format!("{pattern}.md"));
    if exact.is_file() {
        return load(exact).await;
    }

    let mut matches = load_all(dir)
        .await?
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
    if input.title.is_empty() {
        input.title = "Untitled".into();
    }
    if input.issue_type.is_empty() {
        input.issue_type = "task".into();
    }
    if let Some(parent) = input.parent.take() {
        input.parent = Some(resolve(dir, &parent).await?.id().to_owned());
    }

    let id = loop {
        let candidate = generate_id(dir)?;
        if !dir.join(format!("{candidate}.md")).exists() {
            break candidate;
        }
    };
    let path = dir.join(format!("{id}.md"));
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
    if let Some(parent) = input.parent {
        raw.push_str(&format!("parent: {parent}\n"));
    }
    if !input.tags.is_empty() {
        raw.push_str(&format!("tags: [{}]\n", input.tags.join(", ")));
    }
    raw.push_str(&format!("---\n# {}\n\n", input.title));
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
    atomic_write(&path, &raw).await?;
    load(path).await
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
    let mut output = Vec::new();
    let mut inserted = false;
    let mut in_frontmatter = false;
    for line in ticket.raw.lines() {
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
    if ticket.raw.ends_with('\n') {
        raw.push('\n');
    }
    atomic_write(&ticket.path, &raw).await?;
    load(ticket.path.clone()).await
}

pub async fn apply_changes(
    ticket: &Ticket,
    expected_revision: Option<&str>,
    changes: TicketChanges<'_>,
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

    let mut raw = current.raw.clone();
    if let Some(status) = changes.status {
        raw = replace_field(&raw, "status", status);
    }
    if let Some(priority) = changes.priority {
        raw = replace_field(&raw, "priority", &priority.min(4).to_string());
    }
    if let Some(assignee) = changes.assignee {
        raw = replace_field(&raw, "assignee", assignee);
    }
    if let Some(tags) = changes.tags {
        raw = replace_field(&raw, "tags", &format!("[{}]", tags.join(", ")));
    }
    if let Some(parent) = changes.parent {
        raw = replace_field(&raw, "parent", parent.unwrap_or_default());
    }
    if let Some(title) = changes.title.filter(|title| !title.trim().is_empty()) {
        raw = replace_title(&raw, title.trim());
    }
    if let Some(description) = changes.description {
        raw = replace_description(&raw, description);
    }

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
    let mut raw = current.raw.clone();
    if !raw.lines().any(|line| line == "## Notes") {
        raw.push_str("\n## Notes\n");
    }
    raw.push_str(&format!("\n**{}**\n\n{}\n", now(), note));
    atomic_write(&current.path, &raw).await?;
    load(current.path).await
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
    let mut raw = ticket.raw.clone();
    if !raw.lines().any(|line| line == "## Notes") {
        raw.push_str("\n## Notes\n");
    }
    raw.push_str(&format!("\n**{}**\n\n{}\n", now(), note));
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
    let file_name = safe_attachment_name(file_name)?;
    let directory = tickets_dir.join("attachments").join(ticket_id);
    fs::create_dir_all(&directory).await?;
    ensure_attachment_directory(tickets_dir, &directory).await?;
    let mut candidate = file_name.clone();
    let mut index = 2;
    while fs::try_exists(directory.join(&candidate)).await? {
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
    let path = directory.join(candidate);
    atomic_write(&path, bytes).await?;
    Ok(path)
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
    update_field(ticket, name, &format!("[{}]", values.join(", "))).await
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
    fs::write(&temporary, contents)
        .await
        .with_context(|| format!("writing {}", temporary.display()))?;
    fs::rename(&temporary, path)
        .await
        .with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
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
