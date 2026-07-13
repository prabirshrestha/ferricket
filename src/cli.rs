use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    io::{IsTerminal, Read},
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
};

use anyhow::{Context, Result, anyhow, bail};
use argh::FromArgs;
use tokio::{fs, io::AsyncWriteExt, process::Command};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::{
    storage::{self, CreateTicket, Ticket},
    tui, ui,
};

const STATUSES: &[&str] = &["open", "in_progress", "closed"];

#[derive(Debug)]
pub struct PluginExit(pub ExitStatus);

impl std::fmt::Display for PluginExit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "plugin exited with {}", self.0)
    }
}

impl std::error::Error for PluginExit {}

/// Ferricket: a minimal ticket system with dependency tracking.
#[derive(FromArgs)]
struct Arguments {
    /// print the Ferricket version
    #[argh(switch, short = 'V')]
    version: bool,
    /// command to run
    #[argh(subcommand)]
    command: Option<CommandKind>,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum CommandKind {
    Init(InitArgs),
    Create(CreateArgs),
    Start(IdArgs),
    Close(CloseArgs),
    Reopen(ReopenArgs),
    Status(StatusArgs),
    Dep(DepArgs),
    Undep(UndepArgs),
    Link(LinkArgs),
    Unlink(UnlinkArgs),
    Ls(LsArgs),
    List(ListArgs),
    Ready(ReadyArgs),
    Blocked(BlockedArgs),
    Closed(ClosedArgs),
    Show(ShowArgs),
    AddNote(NoteArgs),
    Edit(EditArgs),
    Query(QueryArgs),
    Tui(TuiArgs),
    Ui(UiArgs),
    Super(SuperArgs),
}

/// initialize a Ferricket workspace
#[derive(FromArgs)]
#[argh(subcommand, name = "init")]
struct InitArgs {
    /// workspace directory; defaults to the current directory
    #[argh(positional)]
    path: Option<PathBuf>,
}

/// create a ticket and print its ID
#[derive(FromArgs)]
#[argh(subcommand, name = "create")]
struct CreateArgs {
    /// ticket title
    #[argh(positional)]
    title: Vec<String>,
    /// description text
    #[argh(option, short = 'd')]
    description: Option<String>,
    /// design notes
    #[argh(option)]
    design: Option<String>,
    /// acceptance criteria
    #[argh(option)]
    acceptance: Option<String>,
    /// type: bug, feature, task, epic, or chore
    #[argh(option, short = 't', default = "String::from(\"task\")")]
    issue_type: String,
    /// priority from 0 (highest) to 4
    #[argh(option, short = 'p', default = "2")]
    priority: u8,
    /// assignee (defaults to git user.name)
    #[argh(option, short = 'a')]
    assignee: Option<String>,
    /// external reference such as gh-123
    #[argh(option)]
    external_ref: Option<String>,
    /// parent ticket ID
    #[argh(option)]
    parent: Option<String>,
    /// comma-separated tags
    #[argh(option)]
    tags: Option<String>,
}

/// set a ticket to in_progress
#[derive(FromArgs)]
#[argh(subcommand, name = "start")]
struct IdArgs {
    /// ticket ID or unique partial ID
    #[argh(positional)]
    id: String,
}

/// close a ticket
#[derive(FromArgs)]
#[argh(subcommand, name = "close")]
struct CloseArgs {
    #[argh(positional)]
    id: String,
}

/// reopen a ticket
#[derive(FromArgs)]
#[argh(subcommand, name = "reopen")]
struct ReopenArgs {
    #[argh(positional)]
    id: String,
}

/// update ticket status
#[derive(FromArgs)]
#[argh(subcommand, name = "status")]
struct StatusArgs {
    /// ticket ID
    #[argh(positional)]
    id: String,
    /// open, in_progress, or closed
    #[argh(positional)]
    status: String,
}

/// manage ticket dependencies
#[derive(FromArgs)]
#[argh(subcommand, name = "dep")]
struct DepArgs {
    /// dependency operation
    #[argh(subcommand)]
    command: DepCommand,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum DepCommand {
    Add(DepAddArgs),
    Tree(DepTreeArgs),
    Cycle(DepCycleArgs),
}

/// add a dependency
#[derive(FromArgs)]
#[argh(subcommand, name = "add")]
struct DepAddArgs {
    /// ticket ID
    #[argh(positional)]
    id: String,
    /// dependency ticket ID
    #[argh(positional)]
    dependency: String,
}

/// show a ticket's dependency tree
#[derive(FromArgs)]
#[argh(subcommand, name = "tree")]
struct DepTreeArgs {
    /// ticket ID
    #[argh(positional)]
    id: String,
    /// show repeated dependencies
    #[argh(switch)]
    full: bool,
}

/// find cycles among active tickets
#[derive(FromArgs)]
#[argh(subcommand, name = "cycle")]
struct DepCycleArgs {}

/// remove a dependency
#[derive(FromArgs)]
#[argh(subcommand, name = "undep")]
struct UndepArgs {
    #[argh(positional)]
    id: String,
    #[argh(positional)]
    dependency: String,
}

/// link two or more tickets symmetrically
#[derive(FromArgs)]
#[argh(subcommand, name = "link")]
struct LinkArgs {
    /// ticket IDs
    #[argh(positional)]
    ids: Vec<String>,
}

/// remove a symmetric ticket link
#[derive(FromArgs)]
#[argh(subcommand, name = "unlink")]
struct UnlinkArgs {
    #[argh(positional)]
    id: String,
    #[argh(positional)]
    target: String,
}

#[derive(FromArgs, Default)]
#[argh(
    subcommand,
    name = "ls",
    description = "list tickets with optional filters"
)]
struct LsArgs {
    /// status to include
    #[argh(option)]
    status: Option<String>,
    /// exact assignee
    #[argh(option, short = 'a')]
    assignee: Option<String>,
    /// exact tag
    #[argh(option, short = 'T')]
    tag: Option<String>,
}

/// alias for ls
#[derive(FromArgs, Default)]
#[argh(subcommand, name = "list")]
struct ListArgs {
    /// status to include
    #[argh(option)]
    status: Option<String>,
    /// exact assignee
    #[argh(option, short = 'a')]
    assignee: Option<String>,
    /// exact tag
    #[argh(option, short = 'T')]
    tag: Option<String>,
}

/// list active tickets whose dependencies are resolved
#[derive(FromArgs, Default)]
#[argh(subcommand, name = "ready")]
struct ReadyArgs {
    /// exact assignee
    #[argh(option, short = 'a')]
    assignee: Option<String>,
    /// exact tag
    #[argh(option, short = 'T')]
    tag: Option<String>,
}

/// list active tickets with unresolved dependencies
#[derive(FromArgs, Default)]
#[argh(subcommand, name = "blocked")]
struct BlockedArgs {
    /// exact assignee
    #[argh(option, short = 'a')]
    assignee: Option<String>,
    /// exact tag
    #[argh(option, short = 'T')]
    tag: Option<String>,
}

/// list recently modified closed tickets
#[derive(FromArgs)]
#[argh(subcommand, name = "closed")]
struct ClosedArgs {
    /// maximum number of tickets
    #[argh(option, default = "20")]
    limit: usize,
    /// exact assignee
    #[argh(option, short = 'a')]
    assignee: Option<String>,
    /// exact tag
    #[argh(option, short = 'T')]
    tag: Option<String>,
}

/// display a ticket with relationship details
#[derive(FromArgs)]
#[argh(subcommand, name = "show")]
struct ShowArgs {
    #[argh(positional)]
    id: String,
}

/// append a timestamped note
#[derive(FromArgs)]
#[argh(subcommand, name = "add-note")]
struct NoteArgs {
    #[argh(positional)]
    id: String,
    /// note text; stdin is used when omitted
    #[argh(positional)]
    text: Vec<String>,
}

/// open a ticket in EDITOR
#[derive(FromArgs)]
#[argh(subcommand, name = "edit")]
struct EditArgs {
    #[argh(positional)]
    id: String,
}

/// output tickets as JSONL, optionally filtered with jq
#[derive(FromArgs)]
#[argh(subcommand, name = "query")]
struct QueryArgs {
    /// jq select expression
    #[argh(positional)]
    filter: Option<String>,
}

/// launch the bundled Ferricket web UI
#[derive(FromArgs)]
#[argh(subcommand, name = "ui")]
struct UiArgs {
    /// project directory or .tickets directory; defaults to current workspace
    #[argh(positional)]
    path: Option<PathBuf>,
    /// address to bind
    #[argh(option, default = "String::from(\"127.0.0.1\")")]
    host: String,
    /// port; 0 selects an available port
    #[argh(option, default = "0")]
    port: u16,
    /// do not open a browser
    #[argh(switch)]
    no_open: bool,
    /// disable live updates from the .tickets directory
    #[argh(switch)]
    no_watch: bool,
}

/// launch the interactive terminal UI
#[derive(FromArgs)]
#[argh(subcommand, name = "tui")]
struct TuiArgs {
    /// project directory or .tickets directory; defaults to current workspace
    #[argh(positional)]
    path: Option<PathBuf>,
    /// disable live updates from the .tickets directory
    #[argh(switch)]
    no_watch: bool,
}

/// bypass plugins and run a built-in command
#[derive(FromArgs)]
#[argh(subcommand, name = "super")]
struct SuperArgs {
    /// built-in command name
    #[argh(positional)]
    command: String,
    /// arguments passed to the built-in command
    #[argh(positional, greedy)]
    args: Vec<String>,
}

pub async fn run() -> Result<()> {
    let mut args = std::env::args_os().collect::<Vec<_>>();
    let command_name = args.get(1).and_then(|arg| arg.to_str()).unwrap_or_default();
    if args.len() == 1 || matches!(command_name, "help" | "--help" | "-h") {
        print_top_level_help(&args[0]).await?;
        return Ok(());
    }
    let plugin_candidate = !command_name.is_empty()
        && command_name != "super"
        && command_name != "help"
        && !command_name.starts_with('-');
    if plugin_candidate && run_plugin(command_name, &args[2..]).await? {
        return Ok(());
    }
    normalize_equals(&mut args);
    let strings = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let Some(parsed) = parse_arguments(&strings)? else {
        return Ok(());
    };
    if parsed.version {
        println!("fer {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let Some(command) = parsed.command else {
        print_top_level_help(&args[0]).await?;
        return Ok(());
    };
    dispatch(command).await
}

fn parse_arguments(strings: &[String]) -> Result<Option<Arguments>> {
    let mut strings = strings.to_vec();
    normalize_legacy_syntax(&mut strings);
    let command = "fer";
    match Arguments::from_args(
        &[command],
        &strings[1..].iter().map(String::as_str).collect::<Vec<_>>(),
    ) {
        Ok(arguments) => Ok(Some(arguments)),
        Err(exit) if exit.status.is_ok() => {
            print!("{}", exit.output);
            Ok(None)
        }
        Err(exit) => Err(anyhow!(exit.output.trim().to_owned())),
    }
}

fn normalize_legacy_syntax(args: &mut Vec<String>) {
    if args.get(1).map(String::as_str) != Some("dep") {
        return;
    }
    let Some(operation) = args.get(2).map(String::as_str) else {
        return;
    };
    if !matches!(operation, "add" | "tree" | "cycle" | "help" | "--help") {
        args.insert(2, "add".into());
    }
}

fn print_generated_help(command: &str) -> Result<()> {
    let exit = match Arguments::from_args(&[command], &["--help"]) {
        Ok(_) => bail!("Error: argh did not generate help"),
        Err(exit) => exit,
    };
    print!("{}", exit.output);
    Ok(())
}

async fn print_top_level_help(command: &std::ffi::OsStr) -> Result<()> {
    print_generated_help(&command.to_string_lossy())?;
    let plugins = installed_plugins().await;
    if !plugins.is_empty() {
        println!("\nPlugins (fer-<cmd>, tk-<cmd>, or ticket-<cmd> in PATH):");
        for (name, description) in plugins {
            println!("  {name:<22} {description}");
        }
    }
    println!(
        "\nPlugins receive TICKETS_DIR, FER_SCRIPT, and TK_SCRIPT. Use 'super' to bypass plugins."
    );
    println!("Plugin descriptions: '# fer-plugin: text', '# tk-plugin: text', or --tk-describe.");
    Ok(())
}

async fn installed_plugins() -> Vec<(String, String)> {
    let Some(path) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    let mut plugins = Vec::new();
    let mut seen = HashSet::new();
    for prefix in ["fer", "tk", "ticket"] {
        let mut candidates = std::env::split_paths(&path)
            .filter_map(|directory| std::fs::read_dir(directory).ok())
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let command = name.strip_prefix(&format!("{prefix}-"))?.to_owned();
                (!command.is_empty() && is_executable(&entry.path()))
                    .then(|| (command, entry.path()))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.0.cmp(&right.0));
        for (command, path) in candidates {
            if !seen.insert(command.clone()) {
                continue;
            }
            plugins.push((command, plugin_description(&path).await));
        }
    }
    plugins
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    return metadata.permissions().mode() & 0o111 != 0;
    #[cfg(not(unix))]
    true
}

async fn plugin_description(path: &Path) -> String {
    if let Ok(contents) = fs::read_to_string(path).await {
        for line in contents.lines().take(10) {
            if let Some(description) = line
                .strip_prefix("# fer-plugin:")
                .or_else(|| line.strip_prefix("# tk-plugin:"))
                .map(str::trim)
                .filter(|description| !description.is_empty())
            {
                return description.to_owned();
            }
        }
    }
    if let Ok(Ok(output)) = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        Command::new(path).arg("--tk-describe").output(),
    )
    .await
    {
        let first = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned();
        if output.status.success()
            && let Some(description) = first.strip_prefix("tk-plugin:").map(str::trim)
            && !description.is_empty()
        {
            return description.to_owned();
        }
    }
    "(no description)".to_owned()
}

fn normalize_equals(args: &mut Vec<OsString>) {
    let mut normalized = Vec::with_capacity(args.len());
    for arg in args.drain(..) {
        let text = arg.to_string_lossy();
        if text.starts_with("--")
            && let Some((name, value)) = text.split_once('=')
        {
            normalized.push(OsString::from(name));
            normalized.push(OsString::from(value));
            continue;
        }
        normalized.push(arg);
    }
    *args = normalized;
}

async fn dispatch(command: CommandKind) -> Result<()> {
    if let CommandKind::Super(args) = command {
        return dispatch_super(args).await;
    }
    if let CommandKind::Init(args) = command {
        return init(args.path.as_deref()).await;
    }
    if let CommandKind::Ui(args) = command {
        let dir = storage::find_tickets_dir_from(args.path.as_deref(), false).await?;
        return ui::serve(dir, &args.host, args.port, !args.no_open, !args.no_watch).await;
    }
    if let CommandKind::Tui(args) = command {
        let dir = storage::find_tickets_dir_from(args.path.as_deref(), false).await?;
        return tui::run(dir, !args.no_watch).await;
    }
    let write = matches!(command, CommandKind::Create(_));
    let dir = storage::find_tickets_dir(write).await?;
    match command {
        CommandKind::Create(args) => create(&dir, args).await,
        CommandKind::Start(args) => status(&dir, &args.id, "in_progress").await,
        CommandKind::Close(args) => status(&dir, &args.id, "closed").await,
        CommandKind::Reopen(args) => status(&dir, &args.id, "open").await,
        CommandKind::Status(args) => status(&dir, &args.id, &args.status).await,
        CommandKind::Dep(args) => dep(&dir, args).await,
        CommandKind::Undep(args) => undep(&dir, &args.id, &args.dependency).await,
        CommandKind::Link(args) => link(&dir, args.ids).await,
        CommandKind::Unlink(args) => unlink(&dir, &args.id, &args.target).await,
        CommandKind::Ls(args) => list(&dir, args.status, args.assignee, args.tag).await,
        CommandKind::List(args) => list(&dir, args.status, args.assignee, args.tag).await,
        CommandKind::Ready(args) => ready(&dir, args.assignee, args.tag, false).await,
        CommandKind::Blocked(args) => ready(&dir, args.assignee, args.tag, true).await,
        CommandKind::Closed(args) => closed(&dir, args).await,
        CommandKind::Show(args) => show(&dir, &args.id).await,
        CommandKind::AddNote(args) => add_note(&dir, args).await,
        CommandKind::Edit(args) => edit(&dir, &args.id).await,
        CommandKind::Query(args) => query(&dir, args.filter).await,
        CommandKind::Init(_) | CommandKind::Ui(_) | CommandKind::Tui(_) | CommandKind::Super(_) => {
            unreachable!()
        }
    }
}

async fn dispatch_super(args: SuperArgs) -> Result<()> {
    if args.command == "super" {
        bail!("Error: nested 'fer super' is not supported");
    }
    let mut values = vec!["fer".to_owned(), args.command];
    values.extend(args.args);
    let Some(parsed) = parse_arguments(&values)? else {
        return Ok(());
    };
    let command = parsed
        .command
        .ok_or_else(|| anyhow!("Error: a built-in command is required"))?;
    Box::pin(dispatch(command)).await
}

const AGENTS_SECTION_MARKER: &str = "<!-- ferricket:instructions -->";
const AGENTS_SECTION: &str = r#"<!-- ferricket:instructions -->
## Ferricket tickets

This project tracks work as Markdown files in `.tickets/`. Use `fer` rather than editing ticket frontmatter by hand.

- Run `fer ready` to find actionable work and `fer show <id>` for full context.
- Create work with `fer create "Title"` and connect dependencies with `fer dep <id> <dependency-id>`.
- Start and finish work with `fer start <id>` and `fer close <id>`.
- Keep ticket IDs, dependency relationships, and `.tickets/*.md` files in commits that implement the work.
"#;

async fn init(path: Option<&Path>) -> Result<()> {
    let workspace = match path {
        Some(path) if path.is_absolute() => path.to_owned(),
        Some(path) => std::env::current_dir()
            .context("Error: cannot determine current directory")?
            .join(path),
        None => std::env::current_dir().context("Error: cannot determine current directory")?,
    };
    fs::create_dir_all(workspace.join(".tickets"))
        .await
        .with_context(|| format!("Error: cannot initialize {}", workspace.display()))?;

    let agents = workspace.join("AGENTS.md");
    let existing = match fs::read_to_string(&agents).await {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error).with_context(|| format!("reading {}", agents.display())),
    };
    if !existing.contains(AGENTS_SECTION_MARKER) {
        let mut updated = existing;
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        if !updated.is_empty() {
            updated.push('\n');
        }
        updated.push_str(AGENTS_SECTION);
        if !updated.ends_with('\n') {
            updated.push('\n');
        }
        fs::write(&agents, updated)
            .await
            .with_context(|| format!("writing {}", agents.display()))?;
    }

    println!("Initialized Ferricket in {}", workspace.display());
    println!("  tickets: {}", workspace.join(".tickets").display());
    println!("  guidance: {}", agents.display());
    Ok(())
}

async fn create(dir: &Path, args: CreateArgs) -> Result<()> {
    if args.priority > 4 {
        bail!("Error: priority must be between 0 and 4");
    }
    let assignee = match args.assignee {
        Some(value) => value,
        None => git_user().await.unwrap_or_default(),
    };
    let ticket = storage::create(
        dir,
        CreateTicket {
            title: args.title.join(" "),
            description: args.description.unwrap_or_default(),
            design: args.design.unwrap_or_default(),
            acceptance: args.acceptance.unwrap_or_default(),
            issue_type: args.issue_type,
            priority: args.priority,
            assignee,
            external_ref: args.external_ref.unwrap_or_default(),
            parent: args.parent,
            tags: args
                .tags
                .map(|tags| {
                    tags.split(',')
                        .map(|tag| tag.trim().to_owned())
                        .filter(|tag| !tag.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
        },
    )
    .await?;
    println!("{}", ticket.id());
    Ok(())
}

async fn git_user() -> Option<String> {
    let output = Command::new("git")
        .args(["config", "user.name"])
        .output()
        .await
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

async fn status(dir: &Path, id: &str, value: &str) -> Result<()> {
    if !STATUSES.contains(&value) {
        bail!("Error: invalid status '{value}'. Must be one of: open in_progress closed");
    }
    let ticket = storage::resolve(dir, id).await?;
    let ticket = storage::update_field(&ticket, "status", value).await?;
    println!("Updated {} -> {value}", ticket.id());
    Ok(())
}

async fn dep(dir: &Path, args: DepArgs) -> Result<()> {
    let (id, dependency) = match args.command {
        DepCommand::Add(args) => (args.id, args.dependency),
        DepCommand::Tree(args) => return dep_tree(dir, &args.id, args.full).await,
        DepCommand::Cycle(_) => return dep_cycles(dir).await,
    };
    let ticket = storage::resolve(dir, &id).await?;
    let dependency = storage::resolve(dir, &dependency).await?;
    let mut deps = ticket.array("deps");
    if deps.iter().any(|id| id == dependency.id()) {
        println!("Dependency already exists");
        return Ok(());
    }
    deps.push(dependency.id().to_owned());
    storage::set_array(&ticket, "deps", &deps).await?;
    println!("Added dependency: {} -> {}", ticket.id(), dependency.id());
    Ok(())
}

async fn undep(dir: &Path, id: &str, dependency: &str) -> Result<()> {
    let ticket = storage::resolve(dir, id).await?;
    let dependency = storage::resolve(dir, dependency).await?;
    let mut deps = ticket.array("deps");
    let old_len = deps.len();
    deps.retain(|id| id != dependency.id());
    if deps.len() == old_len {
        println!("Dependency not found");
        bail!("");
    }
    storage::set_array(&ticket, "deps", &deps).await?;
    println!(
        "Removed dependency: {} -/-> {}",
        ticket.id(),
        dependency.id()
    );
    Ok(())
}

async fn link(dir: &Path, patterns: Vec<String>) -> Result<()> {
    if patterns.len() < 2 {
        bail!("Usage: fer link <id> <id> [id...]");
    }
    let mut tickets = Vec::new();
    for pattern in patterns {
        tickets.push(storage::resolve(dir, &pattern).await?);
    }
    let mut count = 0;
    for ticket in &tickets {
        let mut links = ticket.array("links");
        for other in &tickets {
            if other.id() != ticket.id() && !links.iter().any(|id| id == other.id()) {
                links.push(other.id().to_owned());
                count += 1;
            }
        }
        storage::set_array(ticket, "links", &links).await?;
    }
    if count == 0 {
        println!("All links already exist");
    } else {
        println!("Added {count} link(s) between {} tickets", tickets.len());
    }
    Ok(())
}

async fn unlink(dir: &Path, id: &str, target: &str) -> Result<()> {
    let ticket = storage::resolve(dir, id).await?;
    let target = storage::resolve(dir, target).await?;
    let mut links = ticket.array("links");
    let old_len = links.len();
    links.retain(|id| id != target.id());
    if links.len() == old_len {
        println!("Link not found");
        bail!("");
    }
    storage::set_array(&ticket, "links", &links).await?;
    let mut reverse = target.array("links");
    reverse.retain(|id| id != ticket.id());
    storage::set_array(&target, "links", &reverse).await?;
    println!("Removed link: {} <-> {}", ticket.id(), target.id());
    Ok(())
}

fn matches_filter(ticket: &Ticket, assignee: &Option<String>, tag: &Option<String>) -> bool {
    assignee
        .as_ref()
        .is_none_or(|value| ticket.field("assignee") == value)
        && tag
            .as_ref()
            .is_none_or(|value| ticket.array("tags").contains(value))
}

async fn list(
    dir: &Path,
    status: Option<String>,
    assignee: Option<String>,
    tag: Option<String>,
) -> Result<()> {
    for ticket in storage::load_all(dir).await? {
        if status
            .as_ref()
            .is_some_and(|status| status != ticket.status())
            || !matches_filter(&ticket, &assignee, &tag)
        {
            continue;
        }
        let deps = ticket.array("deps");
        let suffix = if deps.is_empty() {
            String::new()
        } else {
            format!(" <- [{}]", deps.join(", "))
        };
        println!(
            "{:<8} [{}] - {}{}",
            ticket.id(),
            ticket.status(),
            ticket.title,
            suffix
        );
    }
    Ok(())
}

async fn ready(
    dir: &Path,
    assignee: Option<String>,
    tag: Option<String>,
    blocked: bool,
) -> Result<()> {
    let all = storage::load_all(dir).await?;
    let map = storage::ticket_map(&all);
    let mut selected = all
        .into_iter()
        .filter(|ticket| {
            storage::active(ticket)
                && matches_filter(ticket, &assignee, &tag)
                && (storage::unresolved(ticket, &map).is_empty() != blocked)
        })
        .collect::<Vec<_>>();
    selected.sort_by(|a, b| (a.priority(), a.id()).cmp(&(b.priority(), b.id())));
    for ticket in selected {
        let unresolved = storage::unresolved(&ticket, &map);
        let suffix = if blocked {
            format!(" <- [{}]", unresolved.join(", "))
        } else {
            String::new()
        };
        println!(
            "{:<8} [P{}][{}] - {}{}",
            ticket.id(),
            ticket.priority(),
            ticket.status(),
            ticket.title,
            suffix
        );
    }
    Ok(())
}

async fn closed(dir: &Path, args: ClosedArgs) -> Result<()> {
    let mut tickets = storage::load_all(dir)
        .await?
        .into_iter()
        .filter(|ticket| {
            matches!(ticket.status(), "closed" | "done")
                && matches_filter(ticket, &args.assignee, &args.tag)
        })
        .collect::<Vec<_>>();
    tickets.sort_by_key(|ticket| std::cmp::Reverse(ticket.modified));
    for ticket in tickets.into_iter().take(args.limit) {
        println!(
            "{:<8} [{}] - {}",
            ticket.id(),
            ticket.status(),
            ticket.title
        );
    }
    Ok(())
}

async fn show(dir: &Path, id: &str) -> Result<()> {
    let target = storage::resolve(dir, id).await?;
    let all = storage::load_all(dir).await?;
    let map = storage::ticket_map(&all);
    let mut raw = target.raw.clone();
    if let Some(parent) = map.get(target.field("parent")) {
        raw = raw
            .lines()
            .map(|line| {
                if line.starts_with("parent:") {
                    format!("{line}  # {}", parent.title)
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        if target.raw.ends_with('\n') {
            raw.push('\n');
        }
    }
    if !raw.ends_with('\n') {
        raw.push('\n');
    }
    let mut output = raw;
    let blockers = storage::unresolved(&target, &map);
    append_relationship(
        &mut output,
        "Blockers",
        blockers.iter().filter_map(|id| map.get(id)),
    );
    append_relationship(
        &mut output,
        "Blocking",
        all.iter().filter(|ticket| {
            ticket.status() != "closed" && ticket.array("deps").iter().any(|id| id == target.id())
        }),
    );
    append_relationship(
        &mut output,
        "Children",
        all.iter()
            .filter(|ticket| ticket.field("parent") == target.id()),
    );
    append_relationship(
        &mut output,
        "Linked",
        target.array("links").iter().filter_map(|id| map.get(id)),
    );
    if std::io::stdout().is_terminal()
        && let Some(pager) = std::env::var_os("TICKET_PAGER").or_else(|| std::env::var_os("PAGER"))
        && !pager.is_empty()
    {
        let pager = pager.to_string_lossy();
        let mut child = Command::new("sh")
            .args(["-c", pager.as_ref()])
            .stdin(Stdio::piped())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(output.as_bytes()).await?;
        }
        if !child.wait().await?.success() {
            bail!("Pager exited unsuccessfully");
        }
    } else {
        print!("{output}");
    }
    Ok(())
}

fn append_relationship<'a>(
    output: &mut String,
    heading: &str,
    tickets: impl Iterator<Item = &'a Ticket>,
) {
    let tickets = tickets.collect::<Vec<_>>();
    if tickets.is_empty() {
        return;
    }
    output.push_str(&format!("\n## {heading}\n\n"));
    for ticket in tickets {
        output.push_str(&format!(
            "- {} [{}] {}\n",
            ticket.id(),
            ticket.status(),
            ticket.title
        ));
    }
}

async fn add_note(dir: &Path, args: NoteArgs) -> Result<()> {
    let ticket = storage::resolve(dir, &args.id).await?;
    let note = if !args.text.is_empty() {
        args.text.join(" ")
    } else if !std::io::stdin().is_terminal() {
        tokio::task::spawn_blocking(|| {
            let mut value = String::new();
            std::io::stdin().read_to_string(&mut value).map(|_| value)
        })
        .await??
    } else {
        bail!("Error: no note provided");
    };
    storage::append_note(&ticket, note.trim_end()).await?;
    println!("Note added to {}", ticket.id());
    Ok(())
}

async fn edit(dir: &Path, id: &str) -> Result<()> {
    let ticket = storage::resolve(dir, id).await?;
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
        let status = Command::new(editor).arg(&ticket.path).status().await?;
        if !status.success() {
            bail!("Editor exited with {status}");
        }
    } else {
        println!("Edit ticket file: {}", ticket.path.display());
    }
    Ok(())
}

async fn query(dir: &Path, filter: Option<String>) -> Result<()> {
    let lines = storage::load_all(dir)
        .await?
        .into_iter()
        .map(|ticket| serde_json::to_string(&ticket.json()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if let Some(filter) = filter {
        let mut child = Command::new("jq")
            .args(["-c", &format!("select({filter})")])
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .context("Error: jq is required for filtered queries")?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(lines.join("\n").as_bytes()).await?;
        }
        let status = child.wait().await?;
        if !status.success() {
            bail!("jq query failed");
        }
    } else if !lines.is_empty() {
        println!("{}", lines.join("\n"));
    }
    Ok(())
}

async fn dep_cycles(dir: &Path) -> Result<()> {
    let map = storage::ticket_map(&storage::load_all(dir).await?);
    let cycles = storage::detect_cycles(&map);
    if cycles.is_empty() {
        println!("No dependency cycles found");
        return Ok(());
    }
    for (index, cycle) in cycles.iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!(
            "Cycle {}: {} -> {}",
            index + 1,
            cycle.join(" -> "),
            cycle[0]
        );
        for id in cycle {
            let ticket = storage::required_ticket(&map, id)?;
            println!("  {:<8} [{}] {}", id, ticket.status(), ticket.title);
        }
    }
    Ok(())
}

async fn dep_tree(dir: &Path, pattern: &str, full: bool) -> Result<()> {
    let root = storage::resolve(dir, pattern).await?;
    let map = storage::ticket_map(&storage::load_all(dir).await?);
    println!("{} [{}] {}", root.id(), root.status(), root.title);
    let mut printed = HashSet::from([root.id().to_owned()]);
    let mut path = HashSet::from([root.id().to_owned()]);
    print_tree_children(root.id(), "", &map, full, &mut printed, &mut path);
    Ok(())
}

fn print_tree_children(
    id: &str,
    prefix: &str,
    map: &HashMap<String, Ticket>,
    full: bool,
    printed: &mut HashSet<String>,
    path: &mut HashSet<String>,
) {
    let Some(ticket) = map.get(id) else {
        return;
    };
    let mut children = ticket
        .array("deps")
        .into_iter()
        .filter(|child| {
            map.contains_key(child) && !path.contains(child) && (full || !printed.contains(child))
        })
        .collect::<Vec<_>>();
    children.sort_by(|a, b| {
        (subtree_depth(a, map, &mut HashSet::new()), a)
            .cmp(&(subtree_depth(b, map, &mut HashSet::new()), b))
    });
    let count = children.len();
    for (index, child) in children.into_iter().enumerate() {
        let last = index + 1 == count;
        let connector = if last { "└── " } else { "├── " };
        if let Some(ticket) = map.get(&child) {
            println!(
                "{prefix}{connector}{} [{}] {}",
                ticket.id(),
                ticket.status(),
                ticket.title
            );
            printed.insert(child.clone());
            path.insert(child.clone());
            let next_prefix = format!("{prefix}{}", if last { "    " } else { "│   " });
            print_tree_children(&child, &next_prefix, map, full, printed, path);
            path.remove(&child);
        }
    }
}

fn subtree_depth(id: &str, map: &HashMap<String, Ticket>, path: &mut HashSet<String>) -> usize {
    if !path.insert(id.to_owned()) {
        return 0;
    }
    let depth = map
        .get(id)
        .map(|ticket| {
            ticket
                .array("deps")
                .into_iter()
                .map(|child| 1 + subtree_depth(&child, map, path))
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    path.remove(id);
    depth
}

async fn run_plugin(command: &str, args: &[OsString]) -> Result<bool> {
    if command.is_empty() {
        return Ok(false);
    }
    for prefix in ["fer", "tk", "ticket"] {
        let name = format!("{prefix}-{command}");
        if let Some(path) = find_executable(&name) {
            let dir = storage::find_tickets_dir(false).await.ok();
            let current = std::env::current_exe()?;
            let mut child = Command::new(path);
            child
                .args(args)
                .env("FER_SCRIPT", &current)
                .env("TK_SCRIPT", &current);
            if let Some(dir) = dir {
                child.env("TICKETS_DIR", dir);
            }
            let status = child.status().await?;
            if !status.success() {
                return Err(PluginExit(status).into());
            }
            return Ok(true);
        }
    }
    Ok(false)
}

fn find_executable(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn init_preserves_existing_agents_content_and_is_idempotent() {
        let root = std::env::temp_dir().join(format!(
            "ferricket-init-test-{}-{}",
            std::process::id(),
            storage::now().replace([':', '-'], "")
        ));
        fs::create_dir_all(&root).await.unwrap();
        fs::write(root.join("AGENTS.md"), "# Existing guide\nKeep this.\n")
            .await
            .unwrap();

        init(Some(&root)).await.unwrap();
        let first = fs::read_to_string(root.join("AGENTS.md")).await.unwrap();
        init(Some(&root)).await.unwrap();
        let second = fs::read_to_string(root.join("AGENTS.md")).await.unwrap();

        assert!(root.join(".tickets").is_dir());
        assert!(first.starts_with("# Existing guide\nKeep this."));
        assert!(first.contains("fer create \"Title\""));
        assert_eq!(first.matches(AGENTS_SECTION_MARKER).count(), 1);
        assert_eq!(first, second);
        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn create_joins_unquoted_title_words() {
        let root = std::env::temp_dir().join(format!(
            "ferricket-create-title-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&root).await.unwrap();
        create(
            &root,
            CreateArgs {
                title: vec!["Fix".into(), "login".into(), "bug".into()],
                description: None,
                design: None,
                acceptance: None,
                issue_type: "task".into(),
                priority: 2,
                assignee: Some(String::new()),
                external_ref: None,
                parent: None,
                tags: None,
            },
        )
        .await
        .unwrap();
        let tickets = storage::load_all(&root).await.unwrap();
        assert_eq!(tickets.len(), 1);
        assert_eq!(tickets[0].title, "Fix login bug");
        fs::remove_dir_all(root).await.unwrap();
    }

    #[test]
    fn argh_parses_nested_and_compatible_dependency_commands() {
        let nested = Arguments::from_args(&["fer"], &["dep", "tree", "task-1", "--full"]).unwrap();
        assert!(matches!(
            nested.command,
            Some(CommandKind::Dep(DepArgs {
                command: DepCommand::Tree(DepTreeArgs { full: true, .. }),
                ..
            }))
        ));

        let compatible =
            parse_arguments(&["fer".into(), "dep".into(), "task-1".into(), "task-2".into()])
                .unwrap()
                .unwrap();
        assert!(matches!(
            compatible.command,
            Some(CommandKind::Dep(DepArgs {
                command: DepCommand::Add(DepAddArgs { id, dependency })
            })) if id == "task-1" && dependency == "task-2"
        ));
    }

    #[test]
    fn argh_owns_version_and_generated_help() {
        let version = Arguments::from_args(&["fer"], &["--version"]).unwrap();
        assert!(version.version);
        let help = match Arguments::from_args(&["fer"], &["dep", "--help"]) {
            Ok(_) => panic!("help should stop argument parsing"),
            Err(help) => help,
        };
        assert!(help.status.is_ok());
        assert!(help.output.contains("Commands:"));
        assert!(help.output.contains("tree"));
    }
}
