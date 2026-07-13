use std::{
    collections::{HashMap, HashSet},
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

use anyhow::{Result, bail};
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    preferences, query,
    storage::{self, CreateTicket, Ticket},
};

const ACCENT: Color = Color::Rgb(101, 114, 220);
const MUTED: Color = Color::Rgb(137, 137, 144);
const BORDER: Color = Color::Rgb(55, 55, 63);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    List,
    Detail,
    Help,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneFocus {
    List,
    Details,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TicketView {
    Active,
    All,
    Blocked,
    Closed,
}

impl TicketView {
    fn label(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::All => "All",
            Self::Blocked => "Blocked",
            Self::Closed => "Closed",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Active => Self::All,
            Self::All => Self::Blocked,
            Self::Blocked => Self::Closed,
            Self::Closed => Self::Active,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::All => "all",
            Self::Blocked => "blocked",
            Self::Closed => "closed",
        }
    }

    fn from_key(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "all" => Some(Self::All),
            "blocked" => Some(Self::Blocked),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Prompt {
    Filter,
    Jump,
    Sneak,
    Note,
}

#[derive(Default)]
struct CreateForm {
    fields: [String; 8],
    focus: usize,
}

#[derive(Clone, Debug)]
struct Row {
    ticket: Ticket,
    depth: usize,
    last: bool,
}

struct App {
    dir: PathBuf,
    tickets: Vec<Ticket>,
    rows: Vec<Row>,
    selected: usize,
    pane_focus: PaneFocus,
    split_panes: bool,
    view: TicketView,
    screen: Screen,
    previous_screen: Screen,
    prompt: Option<Prompt>,
    create_form: Option<CreateForm>,
    input: String,
    filter: String,
    filter_before_prompt: String,
    current_user: Option<String>,
    preferences_key: String,
    detail_scroll: u16,
    detail_relation: usize,
    back_history: Vec<String>,
    forward_history: Vec<String>,
    message: String,
    live_updates: bool,
    watch_available: bool,
    selected_ids: HashSet<String>,
}

impl App {
    async fn load(dir: PathBuf, watch_enabled: bool) -> Result<Self> {
        let tickets = storage::load_all(&dir).await?;
        let current_user = query::current_user().await;
        let preferences_key = preferences::workspace_key(&dir).await;
        let saved = preferences::load(&preferences_key, "tui")
            .await
            .ok()
            .flatten();
        let view = saved
            .as_ref()
            .and_then(|value| value.get("view"))
            .and_then(serde_json::Value::as_str)
            .and_then(TicketView::from_key)
            .unwrap_or(TicketView::Active);
        let filter = saved
            .as_ref()
            .and_then(|value| value.get("query"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let rows = hierarchy_rows(&tickets, &filter, view, current_user.as_deref());
        Ok(Self {
            dir,
            tickets,
            rows,
            selected: 0,
            pane_focus: PaneFocus::List,
            split_panes: false,
            view,
            screen: Screen::List,
            previous_screen: Screen::List,
            prompt: None,
            create_form: None,
            input: String::new(),
            filter,
            filter_before_prompt: String::new(),
            current_user,
            preferences_key,
            detail_scroll: 0,
            detail_relation: 0,
            back_history: Vec::new(),
            forward_history: Vec::new(),
            message: String::new(),
            live_updates: watch_enabled,
            watch_available: watch_enabled,
            selected_ids: HashSet::new(),
        })
    }

    async fn refresh(&mut self) -> Result<()> {
        let selected_id = self.selected_ticket().map(|ticket| ticket.id().to_owned());
        self.tickets = storage::load_all(&self.dir).await?;
        self.rebuild_rows();
        if let Some(id) = selected_id
            && let Some(index) = self.rows.iter().position(|row| row.ticket.id() == id)
        {
            self.selected = index;
        }
        self.clamp_selection();
        Ok(())
    }

    fn rebuild_rows(&mut self) {
        self.rows = hierarchy_rows(
            &self.tickets,
            &self.filter,
            self.view,
            self.current_user.as_deref(),
        );
        self.clamp_selection();
    }

    fn set_view(&mut self, view: TicketView) {
        self.view = view;
        self.selected = 0;
        self.rebuild_rows();
        self.message = format!("{} view · {} tickets", view.label(), self.rows.len());
    }

    fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
    }

    fn selected_ticket(&self) -> Option<&Ticket> {
        self.rows.get(self.selected).map(|row| &row.ticket)
    }

    fn move_selection(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let index = self.selected as isize + delta;
        self.selected = index.clamp(0, self.rows.len() as isize - 1) as usize;
        self.detail_relation = 0;
        self.detail_scroll = 0;
    }

    fn move_relation(&mut self, delta: isize) {
        let count = self.detail_relations().len();
        if count == 0 {
            return;
        }
        let index = self.detail_relation as isize + delta;
        self.detail_relation = index.clamp(0, count as isize - 1) as usize;
    }

    async fn mutate_status(&mut self) -> Result<()> {
        let tickets = self.mutation_targets();
        let mut changed = 0;
        let mut conflicts = 0;
        for ticket in tickets {
            let status = match ticket.status() {
                "open" => "in_progress",
                "in_progress" => "closed",
                _ => "open",
            };
            match storage::apply_changes(
                &ticket,
                Some(&ticket.revision()),
                storage::TicketChanges {
                    status: Some(status),
                    ..storage::TicketChanges::default()
                },
            )
            .await
            {
                Ok(_) => changed += 1,
                Err(error) if error.downcast_ref::<storage::RevisionConflict>().is_some() => {
                    conflicts += 1
                }
                Err(error) => return Err(error),
            }
        }
        self.selected_ids.clear();
        self.message = mutation_message(changed, conflicts);
        self.refresh().await
    }

    async fn mutate_priority(&mut self) -> Result<()> {
        let tickets = self.mutation_targets();
        let mut changed = 0;
        let mut conflicts = 0;
        for ticket in tickets {
            let priority = (ticket.priority() + 1) % 5;
            match storage::apply_changes(
                &ticket,
                Some(&ticket.revision()),
                storage::TicketChanges {
                    priority: Some(priority),
                    ..storage::TicketChanges::default()
                },
            )
            .await
            {
                Ok(_) => changed += 1,
                Err(error) if error.downcast_ref::<storage::RevisionConflict>().is_some() => {
                    conflicts += 1
                }
                Err(error) => return Err(error),
            }
        }
        self.selected_ids.clear();
        self.message = mutation_message(changed, conflicts);
        self.refresh().await
    }

    fn mutation_targets(&self) -> Vec<Ticket> {
        if self.selected_ids.is_empty() {
            self.selected_ticket().cloned().into_iter().collect()
        } else {
            self.tickets
                .iter()
                .filter(|ticket| self.selected_ids.contains(ticket.id()))
                .cloned()
                .collect()
        }
    }

    fn open_prompt(&mut self, prompt: Prompt) {
        if prompt == Prompt::Filter {
            self.filter_before_prompt.clone_from(&self.filter);
        }
        self.input = if prompt == Prompt::Filter {
            self.filter.clone()
        } else {
            String::new()
        };
        self.prompt = Some(prompt);
    }

    fn select_id(&mut self, id: &str, record: bool) -> bool {
        let index = self
            .rows
            .iter()
            .position(|row| row.ticket.id() == id)
            .or_else(|| {
                self.view = TicketView::All;
                self.filter.clear();
                self.rebuild_rows();
                self.rows.iter().position(|row| row.ticket.id() == id)
            });
        let Some(index) = index else { return false };
        if record
            && let Some(current) = self.selected_ticket().map(|ticket| ticket.id().to_owned())
            && current != id
        {
            self.back_history.push(current);
            self.forward_history.clear();
        }
        self.selected = index;
        self.detail_scroll = 0;
        self.detail_relation = 0;
        true
    }

    fn navigate_history(&mut self, forward: bool) {
        let current = self.selected_ticket().map(|ticket| ticket.id().to_owned());
        let target = if forward {
            self.forward_history.pop()
        } else {
            self.back_history.pop()
        };
        let Some(target) = target else { return };
        if let Some(current) = current {
            if forward {
                self.back_history.push(current);
            } else {
                self.forward_history.push(current);
            }
        }
        self.select_id(&target, false);
    }

    fn detail_relations(&self) -> Vec<String> {
        let Some(ticket) = self.selected_ticket() else {
            return Vec::new();
        };
        let map = storage::ticket_map(&self.tickets);
        let mut ids = Vec::new();
        if map.contains_key(ticket.field("parent")) {
            ids.push(ticket.field("parent").to_owned());
        }
        ids.extend(
            self.tickets
                .iter()
                .filter(|candidate| candidate.field("parent") == ticket.id())
                .map(|candidate| candidate.id().to_owned()),
        );
        ids.extend(
            ticket
                .array("deps")
                .into_iter()
                .filter(|id| map.contains_key(id)),
        );
        ids.extend(
            self.tickets
                .iter()
                .filter(|candidate| candidate.array("deps").iter().any(|id| id == ticket.id()))
                .map(|candidate| candidate.id().to_owned()),
        );
        ids.extend(
            ticket
                .array("links")
                .into_iter()
                .filter(|id| map.contains_key(id)),
        );
        let mut seen = HashSet::new();
        ids.into_iter()
            .filter(|id| seen.insert(id.clone()))
            .collect()
    }

    fn detail_bottom(&self) -> u16 {
        let Some(ticket) = self.selected_ticket() else {
            return 0;
        };
        let content = ticket.description().lines().count()
            + self.detail_relations().len()
            + ticket
                .notes()
                .iter()
                .map(|note| note.text.lines().count() + 2)
                .sum::<usize>()
            + 16;
        u16::try_from(content).unwrap_or(10_000).min(10_000)
    }

    async fn submit_prompt(&mut self) -> Result<()> {
        let Some(prompt) = self.prompt.take() else {
            return Ok(());
        };
        let value = self.input.trim().to_owned();
        match prompt {
            Prompt::Filter => {
                self.filter = value;
                self.rebuild_rows();
                self.message = if self.filter.is_empty() {
                    "Filter cleared".into()
                } else {
                    format!("Filtered by ‘{}’", self.filter)
                };
            }
            Prompt::Jump | Prompt::Sneak if !value.is_empty() => {
                let query = value.to_lowercase();
                if let Some(id) = self
                    .rows
                    .iter()
                    .find(|row| {
                        let target = if prompt == Prompt::Sneak {
                            row.ticket.title.to_lowercase()
                        } else {
                            format!("{} {}", row.ticket.id(), row.ticket.title).to_lowercase()
                        };
                        target.contains(&query)
                    })
                    .map(|row| row.ticket.id().to_owned())
                {
                    self.select_id(&id, self.screen == Screen::Detail);
                    self.message = format!("Jumped to {id}");
                } else {
                    self.message = format!("No ticket matches ‘{value}’");
                }
            }
            Prompt::Note if !value.is_empty() => {
                if let Some(ticket) = self.selected_ticket().cloned() {
                    match storage::append_note_checked(&ticket, Some(&ticket.revision()), &value)
                        .await
                    {
                        Ok(_) => self.message = format!("Note added to {}", ticket.id()),
                        Err(error)
                            if error.downcast_ref::<storage::RevisionConflict>().is_some() =>
                        {
                            self.message = "Ticket changed on disk; note was not added".into()
                        }
                        Err(error) => return Err(error),
                    }
                    self.refresh().await?;
                }
            }
            _ => {}
        }
        self.input.clear();
        Ok(())
    }

    async fn submit_create_form(&mut self) -> Result<()> {
        let Some(form) = self.create_form.take() else {
            return Ok(());
        };
        if form.fields[0].trim().is_empty() {
            self.message = "A title is required".into();
            return Ok(());
        }
        let ticket = storage::create(
            &self.dir,
            CreateTicket {
                title: form.fields[0].trim().into(),
                description: form.fields[1].trim().into(),
                issue_type: if form.fields[3].trim().is_empty() {
                    "task".into()
                } else {
                    form.fields[3].trim().into()
                },
                priority: form.fields[4].trim().parse::<u8>().unwrap_or(2).min(4),
                assignee: form.fields[5].trim().into(),
                tags: form.fields[6]
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .collect(),
                parent: (!form.fields[7].trim().is_empty())
                    .then(|| form.fields[7].trim().to_owned()),
                ..CreateTicket::default()
            },
        )
        .await?;
        if matches!(form.fields[2].trim(), "in_progress" | "closed") {
            storage::update_field(&ticket, "status", form.fields[2].trim()).await?;
        }
        self.message = format!("Created {}", ticket.id());
        self.refresh().await?;
        self.select_id(ticket.id(), false);
        Ok(())
    }

    async fn save_preferences(&self) {
        let value = serde_json::json!({ "view": self.view.key(), "query": self.filter });
        let _ = preferences::save(&self.preferences_key, "tui", &value).await;
    }
}

fn mutation_message(changed: usize, conflicts: usize) -> String {
    if conflicts == 0 {
        format!(
            "Updated {changed} ticket{}",
            if changed == 1 { "" } else { "s" }
        )
    } else {
        format!("Updated {changed}; skipped {conflicts} changed on disk")
    }
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
        let _ = io::stdout().flush();
    }
}

pub async fn run(dir: PathBuf, watch_enabled: bool) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("Error: fer tui requires an interactive terminal");
    }
    let _guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    let mut events = EventStream::new();
    let mut app = App::load(dir.clone(), watch_enabled).await?;
    let (watcher, mut changes) = if watch_enabled {
        let (watcher, receiver) = storage::watch_ticket_files(&dir).await?;
        (Some(watcher), Some(receiver))
    } else {
        (None, None)
    };
    let _watcher = watcher;

    loop {
        app.split_panes = terminal.size()?.width >= 110;
        if !app.split_panes {
            app.pane_focus = PaneFocus::List;
        }
        terminal.draw(|frame| draw(frame, &app))?;
        let event = if let Some(receiver) = changes.as_mut() {
            tokio::select! {
                event = events.next() => event.transpose()?,
                change = receiver.recv() => {
                    if change.is_some() && app.live_updates {
                        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                        while receiver.try_recv().is_ok() {}
                        app.refresh().await?;
                        app.message = "Updated from disk".into();
                    }
                    continue;
                }
            }
        } else {
            events.next().await.transpose()?
        };
        let Some(event) = event else { break };
        let Event::Key(key) = event else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if handle_key(&mut app, key).await? {
            break;
        }
    }
    app.save_preferences().await;
    Ok(())
}

async fn handle_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Ok(true);
    }
    if let Some(form) = app.create_form.as_mut() {
        match key.code {
            KeyCode::Esc => app.create_form = None,
            KeyCode::Tab | KeyCode::Down => form.focus = (form.focus + 1) % form.fields.len(),
            KeyCode::BackTab | KeyCode::Up => {
                form.focus = (form.focus + form.fields.len() - 1) % form.fields.len();
            }
            KeyCode::Enter if form.focus + 1 == form.fields.len() => {
                app.submit_create_form().await?
            }
            KeyCode::Enter => form.focus += 1,
            KeyCode::Backspace => {
                form.fields[form.focus].pop();
            }
            KeyCode::Char(character) => form.fields[form.focus].push(character),
            _ => {}
        }
        return Ok(false);
    }
    if app.prompt.is_some() {
        match key.code {
            KeyCode::Esc => {
                if app.prompt == Some(Prompt::Filter) {
                    app.filter.clone_from(&app.filter_before_prompt);
                    app.rebuild_rows();
                }
                app.prompt = None;
                app.input.clear();
            }
            KeyCode::Enter => app.submit_prompt().await?,
            KeyCode::Backspace => {
                app.input.pop();
                if app.prompt == Some(Prompt::Filter) {
                    app.filter = app.input.clone();
                    app.rebuild_rows();
                }
            }
            KeyCode::Char(character) => {
                app.input.push(character);
                if app.prompt == Some(Prompt::Filter) {
                    app.filter = app.input.clone();
                    app.rebuild_rows();
                }
            }
            _ => {}
        }
        return Ok(false);
    }
    if app.screen == Screen::Help {
        if matches!(
            key.code,
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
        ) {
            app.screen = app.previous_screen;
        }
        return Ok(false);
    }
    match key.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('?') => {
            app.previous_screen = app.screen;
            app.screen = Screen::Help;
        }
        KeyCode::Esc | KeyCode::Backspace if app.screen == Screen::Detail => {
            app.screen = Screen::List;
            app.detail_scroll = 0;
        }
        KeyCode::Esc if !app.filter.is_empty() => {
            app.filter.clear();
            app.rebuild_rows();
        }
        KeyCode::Char('1') if app.screen == Screen::List => app.set_view(TicketView::Active),
        KeyCode::Char('2') if app.screen == Screen::List => app.set_view(TicketView::All),
        KeyCode::Char('3') if app.screen == Screen::List => app.set_view(TicketView::Blocked),
        KeyCode::Char('4') if app.screen == Screen::List => app.set_view(TicketView::Closed),
        KeyCode::Char('v') if app.screen == Screen::List => app.set_view(app.view.next()),
        KeyCode::Left | KeyCode::Char('h')
            if app.screen == Screen::List
                && app.split_panes
                && app.pane_focus == PaneFocus::Details =>
        {
            app.pane_focus = PaneFocus::List;
            app.message = "Ticket list focused".into();
        }
        KeyCode::Right | KeyCode::Char('l')
            if app.screen == Screen::List
                && app.split_panes
                && app.pane_focus == PaneFocus::List =>
        {
            app.pane_focus = PaneFocus::Details;
            app.message = "Details focused · j/k relationships · Enter follow".into();
        }
        KeyCode::Tab if app.screen == Screen::List && app.split_panes => {
            app.pane_focus = if app.pane_focus == PaneFocus::List {
                PaneFocus::Details
            } else {
                PaneFocus::List
            };
        }
        KeyCode::BackTab if app.screen == Screen::List && app.split_panes => {
            app.pane_focus = if app.pane_focus == PaneFocus::List {
                PaneFocus::Details
            } else {
                PaneFocus::List
            };
        }
        KeyCode::Down | KeyCode::Char('j')
            if app.screen == Screen::List && app.pane_focus == PaneFocus::List =>
        {
            app.move_selection(1)
        }
        KeyCode::Up | KeyCode::Char('k')
            if app.screen == Screen::List && app.pane_focus == PaneFocus::List =>
        {
            app.move_selection(-1)
        }
        KeyCode::Down | KeyCode::Char('j')
            if app.screen == Screen::List && app.pane_focus == PaneFocus::Details =>
        {
            app.move_relation(1)
        }
        KeyCode::Up | KeyCode::Char('k')
            if app.screen == Screen::List && app.pane_focus == PaneFocus::Details =>
        {
            app.move_relation(-1)
        }
        KeyCode::Enter if app.screen == Screen::List && app.pane_focus == PaneFocus::Details => {
            if let Some(id) = app.detail_relations().get(app.detail_relation).cloned() {
                app.select_id(&id, true);
            } else {
                app.screen = Screen::Detail;
                app.detail_scroll = 0;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.detail_scroll = app.detail_scroll.saturating_add(1)
        }
        KeyCode::Up | KeyCode::Char('k') => app.detail_scroll = app.detail_scroll.saturating_sub(1),
        KeyCode::Char('d')
            if key.modifiers.contains(KeyModifiers::CONTROL) && app.screen == Screen::Detail =>
        {
            app.detail_scroll = app.detail_scroll.saturating_add(10)
        }
        KeyCode::Char('u')
            if key.modifiers.contains(KeyModifiers::CONTROL) && app.screen == Screen::Detail =>
        {
            app.detail_scroll = app.detail_scroll.saturating_sub(10)
        }
        KeyCode::Char('g') if app.screen == Screen::Detail => app.detail_scroll = 0,
        KeyCode::Char('G') if app.screen == Screen::Detail => {
            app.detail_scroll = app.detail_bottom()
        }
        KeyCode::Left | KeyCode::Char('h') if app.screen == Screen::Detail => {
            app.navigate_history(false)
        }
        KeyCode::Right | KeyCode::Char('l') if app.screen == Screen::Detail => {
            app.navigate_history(true)
        }
        KeyCode::Tab if app.screen == Screen::Detail => {
            let count = app.detail_relations().len();
            if count > 0 {
                app.detail_relation = (app.detail_relation + 1) % count;
            }
        }
        KeyCode::BackTab if app.screen == Screen::Detail => {
            let count = app.detail_relations().len();
            if count > 0 {
                app.detail_relation = (app.detail_relation + count - 1) % count;
            }
        }
        KeyCode::Enter if app.screen == Screen::Detail => {
            if let Some(id) = app.detail_relations().get(app.detail_relation).cloned() {
                app.select_id(&id, true);
            }
        }
        KeyCode::Enter if app.selected_ticket().is_some() => {
            app.screen = Screen::Detail;
            app.detail_scroll = 0;
        }
        KeyCode::Char('/') => app.open_prompt(Prompt::Filter),
        KeyCode::Char('s') => app.open_prompt(Prompt::Sneak),
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.open_prompt(Prompt::Jump)
        }
        KeyCode::Char('c') if app.screen == Screen::List => {
            app.create_form = Some(CreateForm::default())
        }
        KeyCode::Char(' ') if app.screen == Screen::List => {
            if let Some(id) = app.selected_ticket().map(|ticket| ticket.id().to_owned()) {
                if !app.selected_ids.insert(id.clone()) {
                    app.selected_ids.remove(&id);
                }
                app.move_selection(1);
            }
        }
        KeyCode::Char('n') if app.screen == Screen::Detail => app.open_prompt(Prompt::Note),
        KeyCode::Char('S') => app.mutate_status().await?,
        KeyCode::Char('p') => app.mutate_priority().await?,
        KeyCode::Char('r') => {
            app.refresh().await?;
            app.message = "Reloaded from disk".into();
        }
        KeyCode::Char('L') if app.watch_available => {
            app.live_updates = !app.live_updates;
            if app.live_updates {
                app.refresh().await?;
                app.message = "Live updates resumed".into();
            } else {
                app.message = "Live updates paused".into();
            }
        }
        _ => {}
    }
    Ok(false)
}

fn draw(frame: &mut Frame<'_>, app: &App) {
    if app.screen == Screen::Detail {
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(4),
            Constraint::Length(1),
        ])
        .split(frame.area());
        draw_header(frame, chunks[0], app);
        draw_detail_identity(frame, chunks[1], app);
        draw_preview(frame, chunks[2], app, false);
        draw_footer(frame, chunks[3], app);
        if let Some(prompt) = app.prompt {
            draw_prompt(frame, app, prompt);
        }
        return;
    }
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .split(frame.area());
    draw_header(frame, chunks[0], app);
    if frame.area().width >= 110 && app.screen == Screen::List {
        let panes = Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(chunks[1]);
        draw_list(frame, panes[0], app);
        draw_preview(frame, panes[1], app, true);
    } else {
        draw_list(frame, chunks[1], app);
    }
    draw_footer(frame, chunks[2], app);
    if app.screen == Screen::Help {
        draw_help(frame);
    }
    if let Some(prompt) = app.prompt {
        draw_prompt(frame, app, prompt);
    }
    if app.create_form.is_some() {
        draw_create_form(frame, app);
    }
}

fn draw_detail_identity(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(ticket) = app.selected_ticket() else {
        return;
    };
    let lines = vec![
        Line::from(vec![
            Span::styled(
                ticket.id(),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "  {}  P{}  {}",
                    status_label(ticket.status()),
                    ticket.priority(),
                    ticket.field("type")
                ),
                Style::default().fg(MUTED),
            ),
        ]),
        Line::from(Span::styled(
            ticket.title.clone(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(BORDER)),
        ),
        area,
    );
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let workspace = app.dir.parent().unwrap_or(&app.dir).display();
    let live = if !app.watch_available {
        "  watch off"
    } else if app.live_updates {
        "  ● live"
    } else {
        "  ○ paused"
    };
    let title = Line::from(vec![
        Span::styled(
            " FERRICKET ",
            Style::default()
                .fg(Color::White)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(workspace.to_string(), Style::default().fg(MUTED)),
        Span::styled(
            live,
            Style::default().fg(if app.live_updates {
                Color::Green
            } else {
                MUTED
            }),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(title)
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(BORDER)),
            )
            .alignment(Alignment::Left),
        area,
    );
}

fn draw_list(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let items = app
        .rows
        .iter()
        .map(|row| {
            let tree = if row.depth == 0 {
                String::new()
            } else {
                format!(
                    "{}{} ",
                    "  ".repeat(row.depth.saturating_sub(1)),
                    if row.last { "└─" } else { "├─" }
                )
            };
            let blocked =
                !storage::unresolved(&row.ticket, &storage::ticket_map(&app.tickets)).is_empty();
            ListItem::new(Line::from(vec![
                Span::styled(
                    if app.selected_ids.contains(row.ticket.id()) {
                        "✓ "
                    } else {
                        "  "
                    },
                    Style::default().fg(ACCENT),
                ),
                Span::styled(
                    format!("{:<3}", status_glyph(row.ticket.status())),
                    Style::default().fg(status_color(row.ticket.status())),
                ),
                Span::styled(
                    format!("P{} ", row.ticket.priority()),
                    priority_style(row.ticket.priority()),
                ),
                Span::styled(
                    format!("{}{:<10}", tree, row.ticket.id()),
                    Style::default().fg(MUTED),
                ),
                Span::raw(" "),
                Span::styled(row.ticket.title.clone(), Style::default().fg(Color::White)),
                if blocked {
                    Span::styled("  blocked", Style::default().fg(Color::Red))
                } else {
                    Span::raw("")
                },
            ]))
        })
        .collect::<Vec<_>>();
    let focus = if app.pane_focus == PaneFocus::List {
        "●"
    } else {
        "○"
    };
    let title = if app.filter.is_empty() {
        format!(" {focus} {} · {} ", app.view.label(), app.rows.len())
    } else {
        format!(
            " {focus} {} · {} · /{} ",
            app.view.label(),
            app.rows.len(),
            app.filter
        )
    };
    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if app.pane_focus == PaneFocus::List {
                    ACCENT
                } else {
                    BORDER
                })),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(37, 38, 45))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    let mut state =
        ListState::default().with_selected((!app.rows.is_empty()).then_some(app.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_preview(frame: &mut Frame<'_>, area: Rect, app: &App, include_identity: bool) {
    let Some(ticket) = app.selected_ticket() else {
        frame.render_widget(
            Paragraph::new("No tickets yet. Press c to create one.").block(panel(" Preview ")),
            area,
        );
        return;
    };
    let map = storage::ticket_map(&app.tickets);
    let dependencies = ticket.array("deps");
    let blocking = app
        .tickets
        .iter()
        .filter(|candidate| candidate.array("deps").iter().any(|id| id == ticket.id()))
        .map(|candidate| candidate.id().to_owned())
        .collect::<Vec<_>>();
    let children = app
        .tickets
        .iter()
        .filter(|candidate| candidate.field("parent") == ticket.id())
        .map(|candidate| candidate.id())
        .collect::<Vec<_>>();
    let mut lines = Vec::new();
    if include_identity {
        lines.extend([
            Line::from(Span::styled(
                ticket.title.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
            Line::from(vec![
                Span::styled(ticket.id(), Style::default().fg(ACCENT)),
                Span::styled(
                    format!(
                        "  {}  P{}  {}",
                        status_label(ticket.status()),
                        ticket.priority(),
                        ticket.field("type")
                    ),
                    Style::default().fg(MUTED),
                ),
            ]),
            Line::raw(""),
        ]);
    }
    let description = ticket.description();
    lines.extend(description.lines().map(|line| Line::raw(line.to_owned())));
    if description.is_empty() {
        lines.push(Line::styled("No description.", Style::default().fg(MUTED)));
    }
    let selected_relation = (app.screen == Screen::Detail
        || (include_identity && app.pane_focus == PaneFocus::Details))
        .then_some(app.detail_relation);
    let mut relation_index = 0;
    let parent = ticket.field("parent").to_owned();
    if map.contains_key(parent.as_str()) {
        relationship_lines(
            &mut lines,
            "Parent",
            &[parent],
            &map,
            selected_relation,
            &mut relation_index,
        );
    }
    relationship_lines(
        &mut lines,
        "Sub-tickets",
        &children
            .into_iter()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>(),
        &map,
        selected_relation,
        &mut relation_index,
    );
    relationship_lines(
        &mut lines,
        "Depends on",
        &dependencies,
        &map,
        selected_relation,
        &mut relation_index,
    );
    relationship_lines(
        &mut lines,
        "Blocks",
        &blocking,
        &map,
        selected_relation,
        &mut relation_index,
    );
    if !ticket.array("links").is_empty() {
        relationship_lines(
            &mut lines,
            "Related",
            &ticket.array("links"),
            &map,
            selected_relation,
            &mut relation_index,
        );
    }
    if !ticket.notes().is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "Notes",
            Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
        ));
        for note in ticket.notes() {
            if let Some(timestamp) = note.timestamp {
                lines.push(Line::styled(timestamp, Style::default().fg(MUTED)));
            }
            lines.extend(note.text.lines().map(|line| Line::raw(format!("  {line}"))));
        }
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0))
            .block(
                Block::default()
                    .title(if include_identity {
                        if app.pane_focus == PaneFocus::Details {
                            " ● Details · j/k select · Enter follow "
                        } else {
                            " ○ Details "
                        }
                    } else {
                        " Details "
                    })
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(
                        if include_identity && app.pane_focus == PaneFocus::Details {
                            ACCENT
                        } else {
                            BORDER
                        },
                    )),
            ),
        area,
    );
}

fn relationship_lines(
    lines: &mut Vec<Line<'_>>,
    heading: &str,
    ids: &[String],
    map: &HashMap<String, Ticket>,
    selected: Option<usize>,
    relation_index: &mut usize,
) {
    if ids.is_empty() {
        return;
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        heading.to_owned(),
        Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
    ));
    for id in ids {
        let title = map
            .get(id)
            .map(|ticket| ticket.title.as_str())
            .unwrap_or("missing ticket");
        let active = selected == Some(*relation_index);
        lines.push(Line::styled(
            format!("{} {}  {}", if active { "›" } else { " " }, id, title),
            if active {
                Style::default().fg(Color::White).bg(Color::Rgb(37, 38, 45))
            } else {
                Style::default().fg(Color::Gray)
            },
        ));
        *relation_index += 1;
    }
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let help = if app.screen == Screen::Detail {
        " esc back  h/l history  j/k scroll  ^d/^u page  tab relation  enter follow  s sneak  L live"
    } else {
        " tab/h/l pane  j/k move/select  enter open/follow  space select  / query  ^p jump  ? help"
    };
    let line = if app.message.is_empty() {
        Line::styled(help, Style::default().fg(MUTED))
    } else {
        Line::from(vec![
            Span::styled(
                format!(" {}", app.message),
                Style::default().fg(Color::Green),
            ),
            Span::styled("  │  ", Style::default().fg(BORDER)),
            Span::styled(help.trim(), Style::default().fg(MUTED)),
        ])
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_prompt(frame: &mut Frame<'_>, app: &App, prompt: Prompt) {
    let area = centered(frame.area(), 64, 5);
    frame.render_widget(Clear, area);
    let title = match prompt {
        Prompt::Filter => " Filter query · status:open label:backend -is:blocked OR … ",
        Prompt::Jump => " Jump to ticket · Ctrl+P ",
        Prompt::Sneak => " Sneak by title ",
        Prompt::Note => " Add note ",
    };
    // Leave one interior cell after the visible tail so the cursor never
    // overwrites the dialog's right border.
    let available = usize::from(area.width.saturating_sub(3));
    let visible_input = app
        .input
        .chars()
        .rev()
        .take(available)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    frame.render_widget(
        Paragraph::new(visible_input.as_str())
            .block(panel(title))
            .style(Style::default().fg(Color::White)),
        area,
    );
    frame.set_cursor_position((
        area.x + u16::try_from(visible_input.chars().count()).unwrap_or(available as u16) + 1,
        area.y + 1,
    ));
}

fn draw_create_form(frame: &mut Frame<'_>, app: &App) {
    let Some(form) = &app.create_form else { return };
    let area = centered(frame.area(), 78, 23);
    frame.render_widget(Clear, area);
    let labels = [
        "Title *",
        "Description",
        "Status (open/in_progress/closed)",
        "Type",
        "Priority (0-4)",
        "Assignee",
        "Labels (comma separated)",
        "Parent ticket ID",
    ];
    let mut lines = Vec::new();
    for (index, label) in labels.iter().enumerate() {
        lines.push(Line::styled(
            format!("{label}:"),
            Style::default()
                .fg(if form.focus == index { ACCENT } else { MUTED })
                .add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::styled(
            format!("  {}", form.fields[index]),
            Style::default()
                .fg(Color::White)
                .bg(if form.focus == index {
                    Color::Rgb(37, 38, 45)
                } else {
                    Color::Reset
                }),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "Tab/Shift-Tab fields · Enter advances/creates · Esc cancel",
        Style::default().fg(MUTED),
    ));
    frame.render_widget(Paragraph::new(lines).block(panel(" Create ticket ")), area);
}

fn draw_help(frame: &mut Frame<'_>) {
    let area = centered(frame.area(), 72, 20);
    frame.render_widget(Clear, area);
    let help = "Views\n  1 / 2 / 3 / 4    Active / All / Blocked / Closed\n  v                Cycle views\n\nSplit panes\n  tab / shift-tab  Switch ticket list / details focus\n  h / l, ← / →    Focus left/right pane in split view\n  j / k, ↑ / ↓    Move tickets or select relationships\n  enter            Open details or follow selected relationship\n\nFull details\n  h / l, ← / →    Back/forward ticket history\n  ctrl+d / ctrl+u Page down/up\n  g / G            Top/bottom\n  tab / shift-tab  Select a relationship\n  esc              Back or clear filter\n\nFind and actions\n  /                Gmail-style query filter\n  ctrl+p           Jump to a ticket\n  s                Sneak-search ticket titles\n  c                Create a ticket\n  S / p            Cycle status / priority\n  n                Add note (detail view)\n  r / L            Reload / pause live updates\n  ?                Close this help\n  q / ctrl+c       Quit";
    frame.render_widget(
        Paragraph::new(help).block(panel(" Keyboard shortcuts ")),
        area,
    );
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Length(width.min(area.width.saturating_sub(2))),
            Constraint::Percentage(50),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Length(height.min(area.height.saturating_sub(2))),
            Constraint::Percentage(50),
        ])
        .split(horizontal[1])[1]
}

fn panel(title: &'static str) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER))
}
fn status_label(status: &str) -> &str {
    match status {
        "open" => "Todo",
        "in_progress" => "In progress",
        "closed" => "Done",
        other => other,
    }
}
fn status_glyph(status: &str) -> &str {
    match status {
        "open" => "○",
        "in_progress" => "◐",
        "closed" => "●",
        _ => "?",
    }
}
fn status_color(status: &str) -> Color {
    match status {
        "in_progress" => Color::Yellow,
        "closed" => MUTED,
        _ => Color::Gray,
    }
}
fn priority_style(priority: u8) -> Style {
    Style::default().fg(match priority {
        0 => Color::Red,
        1 => Color::LightRed,
        2 => Color::Yellow,
        3 => Color::Blue,
        _ => MUTED,
    })
}

fn hierarchy_rows(
    tickets: &[Ticket],
    filter: &str,
    view: TicketView,
    current_user: Option<&str>,
) -> Vec<Row> {
    let by_id = tickets
        .iter()
        .map(|ticket| (ticket.id(), ticket))
        .collect::<HashMap<_, _>>();
    let ticket_map = storage::ticket_map(tickets);
    let mut visible = tickets
        .iter()
        .filter(|ticket| {
            ticket_matches_view(ticket, &ticket_map, view)
                && query::matches(ticket, tickets, filter, current_user)
        })
        .map(|ticket| ticket.id().to_owned())
        .collect::<HashSet<_>>();
    if !filter.trim().is_empty() {
        let matched = tickets
            .iter()
            .filter(|ticket| visible.contains(ticket.id()))
            .cloned()
            .collect::<Vec<_>>();
        for ticket in &matched {
            let mut parent = ticket.field("parent");
            let mut path = HashSet::new();
            while let Some(ticket) = by_id
                .get(parent)
                .copied()
                .filter(|candidate| path.insert(candidate.id()))
            {
                visible.insert(ticket.id().to_owned());
                parent = ticket.field("parent");
            }
        }
    }
    let mut children: HashMap<&str, Vec<&Ticket>> = HashMap::new();
    let mut roots = Vec::new();
    for ticket in tickets
        .iter()
        .filter(|ticket| visible.contains(ticket.id()))
    {
        if !ticket.field("parent").is_empty()
            && visible.contains(ticket.field("parent"))
            && ticket.field("parent") != ticket.id()
        {
            children
                .entry(ticket.field("parent"))
                .or_default()
                .push(ticket);
        } else {
            roots.push(ticket);
        }
    }
    let sort = |items: &mut Vec<&Ticket>| {
        items.sort_by(|a, b| (a.priority(), a.id()).cmp(&(b.priority(), b.id())))
    };
    sort(&mut roots);
    for items in children.values_mut() {
        sort(items);
    }
    let mut rows = Vec::new();
    let mut visited = HashSet::new();
    fn visit(
        ticket: &Ticket,
        depth: usize,
        last: bool,
        children: &HashMap<&str, Vec<&Ticket>>,
        visited: &mut HashSet<String>,
        rows: &mut Vec<Row>,
    ) {
        if !visited.insert(ticket.id().to_owned()) {
            return;
        }
        rows.push(Row {
            ticket: ticket.clone(),
            depth,
            last,
        });
        if let Some(items) = children.get(ticket.id()) {
            let count = items.len();
            for (index, child) in items.iter().enumerate() {
                visit(
                    child,
                    depth + 1,
                    index + 1 == count,
                    children,
                    visited,
                    rows,
                );
            }
        }
    }
    let count = roots.len();
    for (index, root) in roots.into_iter().enumerate() {
        visit(
            root,
            0,
            index + 1 == count,
            &children,
            &mut visited,
            &mut rows,
        );
    }
    for ticket in tickets {
        if visible.contains(ticket.id()) && !visited.contains(ticket.id()) {
            visit(ticket, 0, true, &children, &mut visited, &mut rows);
        }
    }
    rows
}

fn ticket_matches_view(
    ticket: &Ticket,
    tickets: &HashMap<String, Ticket>,
    view: TicketView,
) -> bool {
    match view {
        TicketView::Active => ticket.status() != "closed",
        TicketView::All => true,
        TicketView::Blocked => {
            ticket.status() != "closed" && !storage::unresolved(ticket, tickets).is_empty()
        }
        TicketView::Closed => ticket.status() == "closed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::Backend;
    use std::{collections::BTreeMap, time::SystemTime};

    fn ticket(id: &str, parent: &str, priority: u8) -> Ticket {
        let mut fields = BTreeMap::from([
            ("id".into(), id.into()),
            ("status".into(), "open".into()),
            ("priority".into(), priority.to_string()),
            ("tags".into(), "[]".into()),
            ("deps".into(), "[]".into()),
            ("links".into(), "[]".into()),
            ("type".into(), "task".into()),
            ("created".into(), "2026-07-10T00:00:00Z".into()),
        ]);
        if !parent.is_empty() {
            fields.insert("parent".into(), parent.into());
        }
        Ticket {
            path: PathBuf::from(format!("{id}.md")),
            raw: format!("---\nid: {id}\n---\n# {id}\n"),
            fields,
            title: id.into(),
            modified: SystemTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn hierarchy_keeps_parent_before_children_and_filter_context() {
        let tickets = vec![
            ticket("child", "parent", 0),
            ticket("other", "", 1),
            ticket("parent", "", 3),
        ];
        let rows = hierarchy_rows(&tickets, "child", TicketView::Active, None);
        assert_eq!(
            rows.iter().map(|row| row.ticket.id()).collect::<Vec<_>>(),
            ["parent", "child"]
        );
        assert_eq!(rows[1].depth, 1);
    }

    #[test]
    fn views_select_active_all_blocked_and_closed_tickets() {
        let mut active = ticket("active", "", 0);
        active.fields.insert("deps".into(), "[blocker]".into());
        let mut closed = ticket("closed", "", 1);
        closed.fields.insert("status".into(), "closed".into());
        let mut blocker = ticket("blocker", "", 2);
        blocker.fields.insert("status".into(), "in_progress".into());
        let tickets = vec![active, closed, blocker];

        let ids = |view| {
            hierarchy_rows(&tickets, "", view, None)
                .iter()
                .map(|row| row.ticket.id().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(TicketView::Active), ["active", "blocker"]);
        assert_eq!(ids(TicketView::All), ["active", "closed", "blocker"]);
        assert_eq!(ids(TicketView::Blocked), ["active"]);
        assert_eq!(ids(TicketView::Closed), ["closed"]);
    }

    #[test]
    fn query_uses_advanced_syntax_and_keeps_ancestor_context() {
        let mut child = ticket("child", "parent", 0);
        child.fields.insert("tags".into(), "[frontend]".into());
        child.fields.insert("assignee".into(), "Ada".into());
        let tickets = vec![child, ticket("other", "", 1), ticket("parent", "", 3)];

        let rows = hierarchy_rows(
            &tickets,
            "status:todo label:frontend -assignee:unassigned",
            TicketView::Active,
            None,
        );
        assert_eq!(
            rows.iter().map(|row| row.ticket.id()).collect::<Vec<_>>(),
            ["parent", "child"]
        );
    }

    #[test]
    fn query_context_may_include_an_ancestor_outside_the_selected_view() {
        let child = ticket("child", "parent", 0);
        let mut parent = ticket("parent", "", 3);
        parent.fields.insert("status".into(), "closed".into());
        let tickets = vec![child, parent];

        let rows = hierarchy_rows(&tickets, "id:child", TicketView::Active, None);
        assert_eq!(
            rows.iter().map(|row| row.ticket.id()).collect::<Vec<_>>(),
            ["parent", "child"]
        );
    }

    #[tokio::test]
    async fn split_panes_focus_and_follow_all_dependency_relationships() {
        let root = std::env::temp_dir().join(format!("ferricket-tui-panes-{}", std::process::id()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let mut main = ticket("main", "", 0);
        main.fields.insert("deps".into(), "[closed-dep]".into());
        let mut dependency = ticket("closed-dep", "", 1);
        dependency.fields.insert("status".into(), "closed".into());
        let mut blocked = ticket("blocked", "", 2);
        blocked.fields.insert("deps".into(), "[main]".into());
        let mut app = App::load(root.clone(), false).await.unwrap();
        app.tickets = vec![main, dependency, blocked];
        app.view = TicketView::All;
        app.split_panes = true;
        app.rebuild_rows();
        app.select_id("main", false);

        assert_eq!(app.detail_relations(), ["closed-dep", "blocked"]);
        handle_key(&mut app, KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
            .await
            .unwrap();
        assert_eq!(app.pane_focus, PaneFocus::Details);
        handle_key(&mut app, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
            .await
            .unwrap();
        assert_eq!(app.detail_relation, 1);
        handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await
            .unwrap();
        assert_eq!(app.selected_ticket().unwrap().id(), "blocked");
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn narrow_list_cannot_focus_an_invisible_detail_pane() {
        let root =
            std::env::temp_dir().join(format!("ferricket-tui-narrow-panes-{}", std::process::id()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let mut app = App::load(root.clone(), false).await.unwrap();
        app.tickets = vec![ticket("one", "", 0), ticket("two", "", 1)];
        app.view = TicketView::All;
        app.rebuild_rows();

        handle_key(&mut app, KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
            .await
            .unwrap();
        handle_key(&mut app, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .await
            .unwrap();
        assert_eq!(app.pane_focus, PaneFocus::List);
        handle_key(&mut app, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
            .await
            .unwrap();
        assert_eq!(app.selected, 1);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn go_to_bottom_uses_a_safe_scroll_offset() {
        let root =
            std::env::temp_dir().join(format!("ferricket-tui-scroll-{}", std::process::id()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let mut app = App::load(root.clone(), false).await.unwrap();
        app.tickets = vec![ticket("demo", "", 2)];
        app.rebuild_rows();
        app.screen = Screen::Detail;
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE),
        )
        .await
        .unwrap();
        assert!(app.detail_scroll < u16::MAX);
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn long_prompt_keeps_the_cursor_inside_the_dialog() {
        let root =
            std::env::temp_dir().join(format!("ferricket-tui-prompt-{}", std::process::id()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let mut app = App::load(root.clone(), false).await.unwrap();
        app.prompt = Some(Prompt::Filter);
        app.input = "status:open label:backend ".repeat(20);
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let position = terminal.backend_mut().get_cursor_position().unwrap();
        let prompt_area = centered(Rect::new(0, 0, 80, 24), 64, 5);
        assert!(position.x < prompt_area.right().saturating_sub(1));
        assert!(position.y < 24);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn escape_cancels_a_live_filter_edit() {
        let root = std::env::temp_dir().join(format!(
            "ferricket-tui-filter-cancel-{}",
            std::process::id()
        ));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let mut app = App::load(root.clone(), false).await.unwrap();
        app.tickets = vec![ticket("one", "", 1), ticket("two", "", 2)];
        app.rebuild_rows();
        app.filter = "id:one".into();
        app.rebuild_rows();
        app.open_prompt(Prompt::Filter);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        )
        .await
        .unwrap();
        assert_eq!(app.filter, "id:onex");
        handle_key(&mut app, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .await
            .unwrap();
        assert_eq!(app.filter, "id:one");
        assert_eq!(app.rows.len(), 1);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
}
