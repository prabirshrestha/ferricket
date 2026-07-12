use std::{
    collections::{HashMap, HashSet, VecDeque},
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
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Sparkline, Wrap},
};

use crate::{
    preferences, query,
    storage::{self, CreateTicket, Ticket},
};

const ACCENT: Color = Color::Rgb(101, 114, 220);
const MUTED: Color = Color::Rgb(137, 137, 144);
const BORDER: Color = Color::Rgb(55, 55, 63);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TicketLayout {
    Grouped,
    Tree,
    Board,
    Roadmap,
    Dag,
    Insights,
}

impl TicketLayout {
    fn label(self) -> &'static str {
        match self {
            Self::Grouped => "Grouped",
            Self::Tree => "Tree",
            Self::Board => "Board",
            Self::Roadmap => "Roadmap",
            Self::Dag => "DAG",
            Self::Insights => "Insights",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Grouped => "grouped",
            Self::Tree => "hierarchy",
            Self::Board => "board",
            Self::Roadmap => "roadmap",
            Self::Dag => "dag",
            Self::Insights => "insights",
        }
    }

    fn from_key(value: &str) -> Option<Self> {
        match value {
            "grouped" | "list" => Some(Self::Grouped),
            "hierarchy" | "tree" => Some(Self::Tree),
            "board" => Some(Self::Board),
            "roadmap" => Some(Self::Roadmap),
            "dag" => Some(Self::Dag),
            "insights" => Some(Self::Insights),
            _ => None,
        }
    }

    fn toggled(self) -> Self {
        match self {
            Self::Grouped => Self::Tree,
            Self::Tree => Self::Board,
            Self::Board => Self::Roadmap,
            Self::Roadmap => Self::Dag,
            Self::Dag => Self::Insights,
            Self::Insights => Self::Grouped,
        }
    }
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
    Sneak,
    Note,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PaletteAction {
    Create,
    View(TicketView),
    Layout(TicketLayout),
    ToggleDetails,
    SaveView,
    ToggleLive,
    Ticket(String),
}

#[derive(Clone, Debug)]
struct PaletteEntry {
    label: String,
    detail: String,
    keywords: String,
    group: &'static str,
    action: PaletteAction,
}

impl PaletteEntry {
    fn action(label: &str, detail: &str, keywords: &str, action: PaletteAction) -> Self {
        Self {
            label: label.to_owned(),
            detail: detail.to_owned(),
            keywords: keywords.to_owned(),
            group: "Actions",
            action,
        }
    }
}

#[derive(Clone, Debug)]
struct LayoutColumn {
    label: String,
    detail: String,
    indices: Vec<usize>,
}

#[derive(Default)]
struct PaletteState {
    input: String,
    selected: usize,
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
    preview_visible: bool,
    layout: TicketLayout,
    view: TicketView,
    screen: Screen,
    previous_screen: Screen,
    prompt: Option<Prompt>,
    palette: Option<PaletteState>,
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
    list_offset: usize,
    column_offsets: Vec<usize>,
    viewport_rows: usize,
    pending_g: bool,
    pending_z: bool,
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
        let layout = saved
            .as_ref()
            .and_then(|value| value.get("layout"))
            .and_then(serde_json::Value::as_str)
            .and_then(TicketLayout::from_key)
            .unwrap_or(TicketLayout::Tree);
        let preview_visible = saved
            .as_ref()
            .and_then(|value| value.get("preview"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        let mut app = Self {
            dir,
            tickets,
            rows: Vec::new(),
            selected: 0,
            pane_focus: PaneFocus::List,
            split_panes: false,
            preview_visible,
            layout,
            view,
            screen: Screen::List,
            previous_screen: Screen::List,
            prompt: None,
            palette: None,
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
            list_offset: 0,
            column_offsets: Vec::new(),
            viewport_rows: 1,
            pending_g: false,
            pending_z: false,
        };
        app.rebuild_rows();
        Ok(app)
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
        self.rows = match self.layout {
            TicketLayout::Tree => hierarchy_rows(
                &self.tickets,
                &self.filter,
                self.view,
                self.current_user.as_deref(),
            ),
            TicketLayout::Roadmap | TicketLayout::Dag => dependency_rows(
                &self.tickets,
                &self.filter,
                self.view,
                self.current_user.as_deref(),
            ),
            TicketLayout::Grouped => grouped_rows(
                &self.tickets,
                &self.filter,
                self.view,
                self.current_user.as_deref(),
            ),
            TicketLayout::Board | TicketLayout::Insights => flat_rows(
                &self.tickets,
                &self.filter,
                self.view,
                self.current_user.as_deref(),
            ),
        };
        self.clamp_selection();
    }

    fn set_view(&mut self, view: TicketView) {
        self.view = view;
        self.selected = 0;
        self.reset_offsets();
        self.rebuild_rows();
        self.message = format!("{} view · {} tickets", view.label(), self.rows.len());
    }

    fn set_layout(&mut self, layout: TicketLayout) {
        self.layout = layout;
        self.pane_focus = PaneFocus::List;
        self.selected = 0;
        self.reset_offsets();
        self.rebuild_rows();
        self.message = format!("{} layout", layout.label());
    }

    fn reset_offsets(&mut self) {
        self.list_offset = 0;
        self.column_offsets.clear();
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
        self.ensure_selection_visible();
    }

    fn select_top(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        if self.layout_uses_columns() {
            let Some(column) = self.layout_column() else {
                return;
            };
            if let Some(index) = self.layout_column_indices(column).first().copied() {
                self.selected = index;
                self.set_column_offset(column, 0);
            }
        } else {
            self.selected = 0;
            self.list_offset = 0;
        }
        self.reset_detail_position();
    }

    fn select_bottom(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        if self.layout_uses_columns() {
            let Some(column) = self.layout_column() else {
                return;
            };
            let indices = self.layout_column_indices(column);
            if let Some(index) = indices.last().copied() {
                self.selected = index;
                self.set_column_offset(column, indices.len().saturating_sub(self.viewport_rows));
            }
        } else {
            self.selected = self.rows.len() - 1;
            self.list_offset = self.rows.len().saturating_sub(self.viewport_rows);
        }
        self.reset_detail_position();
    }

    fn center_selection(&mut self) {
        let half = self.viewport_rows / 2;
        if self.layout_uses_columns() {
            let Some(column) = self.layout_column() else {
                return;
            };
            let indices = self.layout_column_indices(column);
            if let Some(position) = indices.iter().position(|index| *index == self.selected) {
                self.set_column_offset(column, position.saturating_sub(half));
            }
        } else {
            self.list_offset = self.selected.saturating_sub(half);
        }
        self.message = "Selection centered".into();
    }

    fn page_selection(&mut self, direction: isize) {
        let amount = (self.viewport_rows / 2).max(1) as isize * direction;
        if self.layout_uses_columns() {
            self.move_column_vertical(amount);
        } else {
            self.move_selection(amount);
        }
    }

    fn move_column_vertical(&mut self, delta: isize) {
        let Some(column) = self.layout_column() else {
            return;
        };
        let indices = self.layout_column_indices(column);
        let Some(position) = indices.iter().position(|index| *index == self.selected) else {
            return;
        };
        let next = (position as isize + delta).clamp(0, indices.len() as isize - 1) as usize;
        self.selected = indices[next];
        self.reset_detail_position();
        self.ensure_selection_visible();
    }

    fn move_column_horizontal(&mut self, delta: isize) {
        let Some(column) = self.layout_column() else {
            return;
        };
        let current = self.layout_column_indices(column);
        let row = current
            .iter()
            .position(|index| *index == self.selected)
            .unwrap_or(0);
        let mut next = column as isize + delta;
        let count = self.layout_column_count() as isize;
        while (0..count).contains(&next) {
            let indices = self.layout_column_indices(next as usize);
            if !indices.is_empty() {
                self.selected = indices[row.min(indices.len() - 1)];
                self.reset_detail_position();
                self.ensure_selection_visible();
                return;
            }
            next += delta;
        }
    }

    fn layout_uses_columns(&self) -> bool {
        matches!(
            self.layout,
            TicketLayout::Board | TicketLayout::Roadmap | TicketLayout::Dag
        )
    }

    fn layout_supports_preview(&self) -> bool {
        matches!(self.layout, TicketLayout::Grouped | TicketLayout::Tree)
    }

    fn layout_columns(&self) -> Vec<LayoutColumn> {
        layout_columns(self.layout, &self.rows)
    }

    fn layout_column_count(&self) -> usize {
        self.layout_columns().len()
    }

    fn layout_column(&self) -> Option<usize> {
        self.layout_columns()
            .iter()
            .position(|column| column.indices.contains(&self.selected))
    }

    fn layout_column_indices(&self, column: usize) -> Vec<usize> {
        self.layout_columns()
            .get(column)
            .map(|column| column.indices.clone())
            .unwrap_or_default()
    }

    fn column_offset(&self, column: usize) -> usize {
        self.column_offsets.get(column).copied().unwrap_or(0)
    }

    fn set_column_offset(&mut self, column: usize, value: usize) {
        if self.column_offsets.len() <= column {
            self.column_offsets.resize(column + 1, 0);
        }
        self.column_offsets[column] = value;
    }

    fn ensure_selection_visible(&mut self) {
        let height = self.viewport_rows.max(1);
        if self.layout_uses_columns() {
            let Some(column) = self.layout_column() else {
                return;
            };
            let indices = self.layout_column_indices(column);
            if let Some(position) = indices.iter().position(|index| *index == self.selected) {
                let mut offset = self.column_offset(column);
                if position < offset {
                    offset = position;
                } else if position >= offset + height {
                    offset = position + 1 - height;
                }
                self.set_column_offset(column, offset);
            }
        } else if self.selected < self.list_offset {
            self.list_offset = self.selected;
        } else if self.selected >= self.list_offset + height {
            self.list_offset = self.selected + 1 - height;
        }
    }

    fn reset_detail_position(&mut self) {
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

    fn open_palette(&mut self) {
        self.palette = Some(PaletteState::default());
    }

    fn palette_entries(&self) -> Vec<PaletteEntry> {
        let actions = vec![
            PaletteEntry::action(
                "Create ticket",
                "C",
                "new add create",
                PaletteAction::Create,
            ),
            PaletteEntry::action(
                "Go to active tickets",
                "View",
                "active inbox",
                PaletteAction::View(TicketView::Active),
            ),
            PaletteEntry::action(
                "Go to all tickets",
                "View",
                "all tickets",
                PaletteAction::View(TicketView::All),
            ),
            PaletteEntry::action(
                "Go to blocked tickets",
                "View",
                "blocked dependency",
                PaletteAction::View(TicketView::Blocked),
            ),
            PaletteEntry::action(
                "Go to closed tickets",
                "View",
                "closed done archive",
                PaletteAction::View(TicketView::Closed),
            ),
            PaletteEntry::action(
                "Use grouped layout",
                "Layout",
                "list grouped rows",
                PaletteAction::Layout(TicketLayout::Grouped),
            ),
            PaletteEntry::action(
                "Use tree layout",
                "Layout",
                "tree hierarchy parent children",
                PaletteAction::Layout(TicketLayout::Tree),
            ),
            PaletteEntry::action(
                "Use board layout",
                "Layout",
                "board kanban columns",
                PaletteAction::Layout(TicketLayout::Board),
            ),
            PaletteEntry::action(
                "Use roadmap layout",
                "Layout",
                "roadmap stages dependencies",
                PaletteAction::Layout(TicketLayout::Roadmap),
            ),
            PaletteEntry::action(
                "Use dependency DAG layout",
                "Layout",
                "dag graph dependencies",
                PaletteAction::Layout(TicketLayout::Dag),
            ),
            PaletteEntry::action(
                "Open Insights",
                "Layout",
                "insights charts metrics dashboard",
                PaletteAction::Layout(TicketLayout::Insights),
            ),
            PaletteEntry::action(
                "Toggle details pane",
                "Action",
                "details preview sidebar pane",
                PaletteAction::ToggleDetails,
            ),
            PaletteEntry::action(
                "Save current view",
                "Action",
                "save view filter layout",
                PaletteAction::SaveView,
            ),
            PaletteEntry::action(
                "Pause or resume live updates",
                "Action",
                "live pause resume filesystem watch",
                PaletteAction::ToggleLive,
            ),
        ];
        actions
            .into_iter()
            .chain(self.tickets.iter().map(|ticket| {
                PaletteEntry {
                    label: ticket.title.clone(),
                    detail: ticket.id().to_owned(),
                    keywords: [
                        ticket.id().to_owned(),
                        ticket.title.clone(),
                        ticket.description(),
                        ticket.field("assignee").to_owned(),
                        ticket.field("type").to_owned(),
                        ticket.array("tags").join(" "),
                    ]
                    .join(" "),
                    group: "Tickets",
                    action: PaletteAction::Ticket(ticket.id().to_owned()),
                }
            }))
            .collect()
    }

    fn filtered_palette_entries(&self) -> Vec<PaletteEntry> {
        let Some(palette) = &self.palette else {
            return Vec::new();
        };
        filter_palette_entries(self.palette_entries(), &palette.input)
    }

    async fn execute_palette_action(&mut self, action: PaletteAction) -> Result<()> {
        self.palette = None;
        match action {
            PaletteAction::Create => self.create_form = Some(CreateForm::default()),
            PaletteAction::View(view) => {
                self.screen = Screen::List;
                self.set_view(view);
            }
            PaletteAction::Layout(layout) => {
                self.screen = Screen::List;
                self.set_layout(layout);
            }
            PaletteAction::ToggleDetails => {
                self.preview_visible = !self.preview_visible;
                self.pane_focus = PaneFocus::List;
                self.message = if self.preview_visible {
                    "Details pane enabled"
                } else {
                    "Details pane hidden"
                }
                .into();
            }
            PaletteAction::SaveView => {
                self.save_preferences().await;
                self.message = "Current view saved".into();
            }
            PaletteAction::ToggleLive => self.toggle_live_updates().await?,
            PaletteAction::Ticket(id) => {
                self.select_id(&id, self.screen == Screen::Detail);
                self.screen = Screen::Detail;
                self.message = format!("Opened {id}");
            }
        }
        Ok(())
    }

    async fn toggle_live_updates(&mut self) -> Result<()> {
        if !self.watch_available {
            self.message = "Filesystem watching is disabled".into();
            return Ok(());
        }
        self.live_updates = !self.live_updates;
        if self.live_updates {
            self.refresh().await?;
            self.message = "Live updates resumed".into();
        } else {
            self.message = "Live updates paused".into();
        }
        Ok(())
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
        self.detail_relation_sections()
            .into_iter()
            .flat_map(|(_, ids)| ids)
            .collect()
    }

    fn detail_relation_sections(&self) -> Vec<(&'static str, Vec<String>)> {
        let Some(ticket) = self.selected_ticket() else {
            return Vec::new();
        };
        let map = storage::ticket_map(&self.tickets);
        let mut seen = HashSet::new();
        let mut section = |heading: &'static str, ids: Vec<String>| {
            let ids = ids
                .into_iter()
                .filter(|id| map.contains_key(id) && seen.insert(id.clone()))
                .collect::<Vec<_>>();
            (!ids.is_empty()).then_some((heading, ids))
        };
        let mut sections = Vec::new();
        if map.contains_key(ticket.field("parent")) {
            sections.extend(section("Parent", vec![ticket.field("parent").to_owned()]));
        }
        sections.extend(section(
            "Sub-tickets",
            self.tickets
                .iter()
                .filter(|candidate| candidate.field("parent") == ticket.id())
                .map(|candidate| candidate.id().to_owned())
                .collect(),
        ));
        sections.extend(section("Depends on", ticket.array("deps")));
        sections.extend(section(
            "Blocks",
            self.tickets
                .iter()
                .filter(|candidate| candidate.array("deps").iter().any(|id| id == ticket.id()))
                .map(|candidate| candidate.id().to_owned())
                .collect(),
        ));
        sections.extend(section("Related", ticket.array("links")));
        sections
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
            Prompt::Sneak if !value.is_empty() => {
                let query = value.to_lowercase();
                if let Some(id) = self
                    .rows
                    .iter()
                    .find(|row| {
                        let target = row.ticket.title.to_lowercase();
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
        let Some(form) = self.create_form.as_ref() else {
            return Ok(());
        };
        if form.fields[0].trim().is_empty() {
            self.message = "A title is required".into();
            if let Some(form) = self.create_form.as_mut() {
                form.focus = 0;
            }
            return Ok(());
        }
        let status = match form.fields[2].trim() {
            "" => "open".to_owned(),
            status => status.to_owned(),
        };
        if !matches!(status.as_str(), "open" | "in_progress" | "closed") {
            self.message = "Status must be open, in_progress, or closed".into();
            if let Some(form) = self.create_form.as_mut() {
                form.focus = 2;
            }
            return Ok(());
        }
        let priority = match form.fields[4].trim() {
            "" => 2,
            value => match value.parse::<u8>() {
                Ok(priority @ 0..=4) => priority,
                _ => {
                    self.message = "Priority must be between 0 and 4".into();
                    if let Some(form) = self.create_form.as_mut() {
                        form.focus = 4;
                    }
                    return Ok(());
                }
            },
        };
        let form = self.create_form.take().expect("create form was validated");
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
                priority,
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
        if status != "open" {
            storage::update_field(&ticket, "status", &status).await?;
        }
        self.message = format!("Created {}", ticket.id());
        self.refresh().await?;
        self.select_id(ticket.id(), false);
        Ok(())
    }

    async fn save_preferences(&self) {
        let value = serde_json::json!({
            "view": self.view.key(),
            "query": self.filter,
            "layout": self.layout.key(),
            "preview": self.preview_visible,
        });
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
        let terminal_size = terminal.size()?;
        app.split_panes =
            terminal_size.width >= 110 && app.preview_visible && app.layout_supports_preview();
        app.viewport_rows = if app.layout_uses_columns() {
            usize::from(terminal_size.height.saturating_sub(7) / 4).max(1)
        } else {
            usize::from(terminal_size.height.saturating_sub(6)).max(1)
        };
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
    if app.palette.is_some() {
        handle_palette_key(app, key).await?;
        return Ok(false);
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
    if app.pending_g {
        app.pending_g = false;
        if key.code == KeyCode::Char('g') {
            if app.screen == Screen::Detail {
                app.detail_scroll = 0;
            } else {
                app.select_top();
            }
            return Ok(false);
        }
    }
    if app.pending_z {
        app.pending_z = false;
        if key.code == KeyCode::Char('z') {
            if app.screen == Screen::Detail {
                app.detail_scroll = app
                    .detail_scroll
                    .saturating_sub(app.viewport_rows as u16 / 2);
            } else {
                app.center_selection();
            }
            return Ok(false);
        }
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
        KeyCode::Char('V') if app.screen == Screen::List => app.set_layout(app.layout.toggled()),
        KeyCode::Char('g') => app.pending_g = true,
        KeyCode::Char('z') => app.pending_z = true,
        KeyCode::Home if app.screen == Screen::List => app.select_top(),
        KeyCode::End | KeyCode::Char('G') if app.screen == Screen::List => app.select_bottom(),
        KeyCode::Char('d')
            if key.modifiers.contains(KeyModifiers::CONTROL) && app.screen == Screen::List =>
        {
            app.page_selection(1)
        }
        KeyCode::Char('u')
            if key.modifiers.contains(KeyModifiers::CONTROL) && app.screen == Screen::List =>
        {
            app.page_selection(-1)
        }
        KeyCode::Left | KeyCode::Char('h')
            if app.screen == Screen::List && app.layout_uses_columns() =>
        {
            app.move_column_horizontal(-1)
        }
        KeyCode::Right | KeyCode::Char('l')
            if app.screen == Screen::List && app.layout_uses_columns() =>
        {
            app.move_column_horizontal(1)
        }
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
            if app.screen == Screen::List
                && app.pane_focus == PaneFocus::List
                && app.layout_uses_columns() =>
        {
            app.move_column_vertical(1)
        }
        KeyCode::Up | KeyCode::Char('k')
            if app.screen == Screen::List
                && app.pane_focus == PaneFocus::List
                && app.layout_uses_columns() =>
        {
            app.move_column_vertical(-1)
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
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => app.open_palette(),
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

async fn handle_palette_key(app: &mut App, key: KeyEvent) -> Result<()> {
    let entries = app.filtered_palette_entries();
    match key.code {
        KeyCode::Esc => app.palette = None,
        KeyCode::Enter => {
            if let Some(palette) = app.palette.as_ref()
                && let Some(entry) = entries.get(palette.selected)
            {
                app.execute_palette_action(entry.action.clone()).await?;
            }
        }
        KeyCode::Down => {
            if let Some(palette) = app.palette.as_mut() {
                palette.selected = (palette.selected + 1).min(entries.len().saturating_sub(1));
            }
        }
        KeyCode::Up => {
            if let Some(palette) = app.palette.as_mut() {
                palette.selected = palette.selected.saturating_sub(1);
            }
        }
        KeyCode::Home => {
            if let Some(palette) = app.palette.as_mut() {
                palette.selected = 0;
            }
        }
        KeyCode::End => {
            if let Some(palette) = app.palette.as_mut() {
                palette.selected = entries.len().saturating_sub(1);
            }
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(palette) = app.palette.as_mut() {
                palette.selected = (palette.selected + 6).min(entries.len().saturating_sub(1));
            }
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(palette) = app.palette.as_mut() {
                palette.selected = palette.selected.saturating_sub(6);
            }
        }
        KeyCode::Backspace => {
            if let Some(palette) = app.palette.as_mut() {
                palette.input.pop();
                palette.selected = 0;
            }
        }
        KeyCode::Char(character) => {
            if let Some(palette) = app.palette.as_mut() {
                palette.input.push(character);
                palette.selected = 0;
            }
        }
        _ => {}
    }
    Ok(())
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
        if app.palette.is_some() {
            draw_palette(frame, app);
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
    if app.split_panes && app.screen == Screen::List {
        let panes = Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(chunks[1]);
        draw_layout(frame, panes[0], app);
        draw_preview(frame, panes[1], app, true);
    } else {
        draw_layout(frame, chunks[1], app);
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
    if app.palette.is_some() {
        draw_palette(frame, app);
    }
}

fn draw_layout(frame: &mut Frame<'_>, area: Rect, app: &App) {
    match app.layout {
        TicketLayout::Grouped => draw_grouped(frame, area, app),
        TicketLayout::Tree => draw_list(frame, area, app),
        TicketLayout::Board | TicketLayout::Roadmap | TicketLayout::Dag => {
            draw_columns(frame, area, app)
        }
        TicketLayout::Insights => draw_insights(frame, area, app),
    }
}

fn draw_grouped(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let statuses = ["in_progress", "open", "closed"]
        .into_iter()
        .filter_map(|status| {
            let indices = app
                .rows
                .iter()
                .enumerate()
                .filter_map(|(index, row)| (row.ticket.status() == status).then_some(index))
                .collect::<Vec<_>>();
            (!indices.is_empty()).then_some((status, indices))
        })
        .collect::<Vec<_>>();
    if statuses.is_empty() {
        draw_empty_layout(frame, area, app);
        return;
    }
    let constraints = statuses
        .iter()
        .map(|(_, indices)| Constraint::Length((indices.len() as u16 + 2).min(10)))
        .chain(std::iter::once(Constraint::Min(0)))
        .collect::<Vec<_>>();
    let chunks = Layout::vertical(constraints).split(area);
    for (group_index, (status, indices)) in statuses.iter().enumerate() {
        let items = indices
            .iter()
            .map(|index| ListItem::new(ticket_line(app, &app.rows[*index], false)))
            .collect::<Vec<_>>();
        let selected = indices.iter().position(|index| *index == app.selected);
        let mut state = ListState::default().with_selected(selected);
        let title = format!(
            " {} {} · {} ",
            status_glyph(status),
            status_label(status),
            indices.len()
        );
        frame.render_stateful_widget(
            List::new(items)
                .block(
                    Block::default()
                        .title(title)
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(if selected.is_some() {
                            ACCENT
                        } else {
                            BORDER
                        })),
                )
                .highlight_style(selected_style())
                .highlight_symbol("› "),
            chunks[group_index],
            &mut state,
        );
    }
}

fn draw_columns(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let columns = app.layout_columns();
    if columns.is_empty() {
        draw_empty_layout(frame, area, app);
        return;
    }
    let focused = app.layout_column().unwrap_or_default();
    let visible_count = (usize::from(area.width) / 28).clamp(1, columns.len());
    let start = focused
        .saturating_sub(visible_count / 2)
        .min(columns.len().saturating_sub(visible_count));
    let end = (start + visible_count).min(columns.len());
    let constraints = (start..end)
        .map(|_| Constraint::Ratio(1, (end - start) as u32))
        .collect::<Vec<_>>();
    let chunks = Layout::horizontal(constraints).spacing(1).split(area);
    for (slot, column_index) in (start..end).enumerate() {
        let column = &columns[column_index];
        let selected = column
            .indices
            .iter()
            .position(|index| *index == app.selected);
        let items = column
            .indices
            .iter()
            .map(|index| {
                let row = &app.rows[*index];
                let mut lines = vec![
                    ticket_line(app, row, true),
                    Line::styled(
                        format!("  {}", column_ticket_detail(app.layout, &row.ticket)),
                        Style::default().fg(MUTED),
                    ),
                ];
                if app.layout == TicketLayout::Dag {
                    let dependencies = row.ticket.array("deps");
                    lines.push(Line::styled(
                        if dependencies.is_empty() {
                            "  root node".to_owned()
                        } else {
                            format!("  ← {}", dependencies.join(", "))
                        },
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                ListItem::new(Text::from(lines))
            })
            .collect::<Vec<_>>();
        let mut state = ListState::default()
            .with_offset(app.column_offset(column_index))
            .with_selected(selected);
        let title = format!(" {} · {} ", column.label, column.indices.len());
        frame.render_stateful_widget(
            List::new(items)
                .block(
                    Block::default()
                        .title(title)
                        .title_bottom(Line::styled(
                            format!(" {} ", column.detail),
                            Style::default().fg(MUTED),
                        ))
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(if column_index == focused {
                            ACCENT
                        } else {
                            BORDER
                        })),
                )
                .highlight_style(selected_style())
                .highlight_symbol("› "),
            chunks[slot],
            &mut state,
        );
    }
}

fn column_ticket_detail(layout: TicketLayout, ticket: &Ticket) -> String {
    match layout {
        TicketLayout::Board => format!("P{} · {}", ticket.priority(), ticket.field("type")),
        TicketLayout::Roadmap => {
            let deps = ticket.array("deps").len();
            format!(
                "{} · {deps} prerequisite{}",
                status_label(ticket.status()),
                if deps == 1 { "" } else { "s" }
            )
        }
        TicketLayout::Dag => format!("{} · P{}", status_label(ticket.status()), ticket.priority()),
        _ => String::new(),
    }
}

fn draw_insights(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(7),
        Constraint::Min(6),
    ])
    .spacing(1)
    .split(area);
    let statuses = ["in_progress", "open", "closed"]
        .map(|status| {
            let count = app
                .rows
                .iter()
                .filter(|row| row.ticket.status() == status)
                .count();
            format!(
                "{} {}  {:>3}",
                status_glyph(status),
                status_label(status),
                count
            )
        })
        .join("     ");
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                format!("{} tickets in {}", app.rows.len(), app.view.label()),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::styled(statuses, Style::default().fg(MUTED)),
        ])
        .block(panel(" Overview ")),
        chunks[0],
    );
    let series = activity_series(&app.rows, 24);
    frame.render_widget(
        Sparkline::default()
            .block(panel(" Ticket activity · oldest → newest "))
            .data(&series)
            .style(Style::default().fg(ACCENT)),
        chunks[1],
    );
    let slices = [
        (
            "Priority",
            insight_counts(&app.rows, |ticket| format!("P{}", ticket.priority())),
        ),
        (
            "Type",
            insight_counts(&app.rows, |ticket| {
                let value = ticket.field("type");
                if value.is_empty() {
                    "task".to_owned()
                } else {
                    value.to_owned()
                }
            }),
        ),
        (
            "Assignee",
            insight_counts(&app.rows, |ticket| {
                let value = ticket.field("assignee");
                if value.is_empty() {
                    "Unassigned".to_owned()
                } else {
                    value.to_owned()
                }
            }),
        ),
        ("Labels", insight_label_counts(&app.rows)),
    ];
    let widths = slices
        .iter()
        .map(|_| Constraint::Ratio(1, 4))
        .collect::<Vec<_>>();
    let panels = Layout::horizontal(widths).spacing(1).split(chunks[2]);
    for ((title, rows), panel_area) in slices.into_iter().zip(panels.iter()) {
        let lines = rows
            .into_iter()
            .take(usize::from(panel_area.height.saturating_sub(2)))
            .map(|(label, count)| Line::raw(format!("{count:>3}  {label}")))
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines).block(panel_owned(title)), *panel_area);
    }
}

fn draw_empty_layout(frame: &mut Frame<'_>, area: Rect, app: &App) {
    frame.render_widget(
        Paragraph::new("No matching tickets. Press c to create one or / to change the filter.")
            .alignment(Alignment::Center)
            .block(panel_owned(app.layout.label())),
        area,
    );
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
        .map(|row| ListItem::new(ticket_line(app, row, app.layout == TicketLayout::Tree)))
        .collect::<Vec<_>>();
    let focus = if app.pane_focus == PaneFocus::List {
        "●"
    } else {
        "○"
    };
    let title = if app.filter.is_empty() {
        format!(
            " {focus} {} · {} · {} ",
            app.view.label(),
            app.layout.label(),
            app.rows.len()
        )
    } else {
        format!(
            " {focus} {} · {} · {} · /{} ",
            app.view.label(),
            app.layout.label(),
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
        .highlight_style(selected_style())
        .highlight_symbol("› ");
    let mut state = ListState::default()
        .with_offset(app.list_offset)
        .with_selected((!app.rows.is_empty()).then_some(app.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn ticket_line<'a>(app: &App, row: &'a Row, tree_visible: bool) -> Line<'a> {
    let tree = if !tree_visible || row.depth == 0 {
        String::new()
    } else {
        format!(
            "{}{} ",
            "  ".repeat(row.depth.saturating_sub(1)),
            if row.last { "└─" } else { "├─" }
        )
    };
    let blocked = !storage::unresolved(&row.ticket, &storage::ticket_map(&app.tickets)).is_empty();
    Line::from(vec![
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
    ])
}

fn selected_style() -> Style {
    Style::default()
        .bg(Color::Rgb(37, 38, 45))
        .add_modifier(Modifier::BOLD)
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
    for (heading, ids) in app.detail_relation_sections() {
        relationship_lines(
            &mut lines,
            heading,
            &ids,
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
        " h/j/k/l navigate  gg/G top/bottom  zz center  V layout  ^p commands  / query  ? help"
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

fn draw_palette(frame: &mut Frame<'_>, app: &App) {
    let Some(palette) = &app.palette else { return };
    let area = centered(frame.area(), 72, 22);
    frame.render_widget(Clear, area);
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(2),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(palette.input.as_str())
            .block(panel(" Search tickets, views, layouts, and actions… ")),
        chunks[0],
    );
    let entries = app.filtered_palette_entries();
    let items = if entries.is_empty() {
        vec![ListItem::new(Line::styled(
            "No matching tickets or actions.",
            Style::default().fg(MUTED),
        ))]
    } else {
        let mut previous_group = "";
        entries
            .iter()
            .map(|entry| {
                let prefix = if previous_group == entry.group {
                    "  ".to_owned()
                } else {
                    previous_group = entry.group;
                    format!("{}  ", entry.group)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{prefix:<11}"), Style::default().fg(MUTED)),
                    Span::styled(entry.label.clone(), Style::default().fg(Color::White)),
                    Span::styled(format!("  {}", entry.detail), Style::default().fg(MUTED)),
                ]))
            })
            .collect()
    };
    let selected = (!entries.is_empty()).then_some(palette.selected.min(entries.len() - 1));
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::LEFT | Borders::RIGHT))
            .highlight_style(selected_style())
            .highlight_symbol("› "),
        chunks[1],
        &mut state,
    );
    frame.render_widget(
        Paragraph::new(" ↑↓ navigate   ↵ open   Esc close ")
            .style(Style::default().fg(MUTED))
            .block(Block::default().borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)),
        chunks[2],
    );
    let cursor = area.x + u16::try_from(palette.input.chars().count()).unwrap_or_default() + 1;
    frame.set_cursor_position((cursor.min(area.right().saturating_sub(2)), area.y + 1));
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
    let area = centered(frame.area(), 76, 25);
    frame.render_widget(Clear, area);
    let help = "Scope and layout\n  1 / 2 / 3 / 4   Active / All / Blocked / Closed\n  v / V           Cycle ticket scope / display layout\n  ctrl+p          Search tickets and run commands\n\nNavigate tickets\n  j / k, ↑ / ↓   Move vertically\n  h / l, ← / →   Move columns or focus list/details\n  gg / G          First / last ticket in the current column\n  zz              Center the selected ticket\n  ctrl+d / ctrl+u Half-page down / up\n  enter           Open ticket or follow selected relationship\n  space           Add/remove ticket from bulk selection\n\nFull details\n  j / k           Scroll content\n  h / l, ← / →   Back / forward ticket history\n  gg / G          Top / bottom; tab selects relationships\n  esc             Back to the current layout\n\nFind and actions\n  / / s           Gmail-style filter / title sneak\n  c / S / p / n   Create / status / priority / add note\n  r / L           Reload / pause or resume live updates\n  ? / q / ctrl+c  Help / quit";
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

fn panel_owned(title: &str) -> Block<'static> {
    Block::default()
        .title(format!(" {title} "))
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

fn flat_rows(
    tickets: &[Ticket],
    filter: &str,
    view: TicketView,
    current_user: Option<&str>,
) -> Vec<Row> {
    let ticket_map = storage::ticket_map(tickets);
    let mut visible = tickets
        .iter()
        .filter(|ticket| {
            ticket_matches_view(ticket, &ticket_map, view)
                && query::matches(ticket, tickets, filter, current_user)
        })
        .cloned()
        .collect::<Vec<_>>();
    visible
        .sort_by(|left, right| (left.priority(), left.id()).cmp(&(right.priority(), right.id())));
    visible
        .into_iter()
        .map(|ticket| Row {
            ticket,
            depth: 0,
            last: true,
        })
        .collect()
}

fn grouped_rows(
    tickets: &[Ticket],
    filter: &str,
    view: TicketView,
    current_user: Option<&str>,
) -> Vec<Row> {
    let rows = flat_rows(tickets, filter, view, current_user);
    ["in_progress", "open", "closed"]
        .into_iter()
        .flat_map(|status| {
            rows.iter()
                .filter(move |row| row.ticket.status() == status)
                .cloned()
        })
        .collect()
}

fn dependency_rows(
    tickets: &[Ticket],
    filter: &str,
    view: TicketView,
    current_user: Option<&str>,
) -> Vec<Row> {
    let matched = flat_rows(tickets, filter, view, current_user);
    let by_id = tickets
        .iter()
        .map(|ticket| (ticket.id().to_owned(), ticket))
        .collect::<HashMap<_, _>>();
    let mut included = matched
        .iter()
        .map(|row| row.ticket.id().to_owned())
        .collect::<HashSet<_>>();
    let mut pending = VecDeque::from_iter(included.iter().cloned());
    while let Some(id) = pending.pop_front() {
        let Some(ticket) = by_id.get(&id) else {
            continue;
        };
        for dependency in ticket.array("deps") {
            if by_id.contains_key(&dependency) && included.insert(dependency.clone()) {
                pending.push_back(dependency);
            }
        }
    }
    let mut rows = tickets
        .iter()
        .filter(|ticket| included.contains(ticket.id()))
        .cloned()
        .map(|ticket| Row {
            ticket,
            depth: 0,
            last: true,
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        (left.ticket.priority(), left.ticket.id())
            .cmp(&(right.ticket.priority(), right.ticket.id()))
    });
    rows
}

fn insight_counts<F>(rows: &[Row], key: F) -> Vec<(String, usize)>
where
    F: Fn(&Ticket) -> String,
{
    let mut counts = HashMap::new();
    for row in rows {
        *counts.entry(key(&row.ticket)).or_insert(0) += 1;
    }
    let mut counts = counts.into_iter().collect::<Vec<_>>();
    counts.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    counts
}

fn insight_label_counts(rows: &[Row]) -> Vec<(String, usize)> {
    let mut counts = HashMap::new();
    for row in rows {
        let tags = row.ticket.array("tags");
        if tags.is_empty() {
            *counts.entry("No label".to_owned()).or_insert(0) += 1;
        } else {
            for tag in tags {
                *counts.entry(tag).or_insert(0) += 1;
            }
        }
    }
    let mut counts = counts.into_iter().collect::<Vec<_>>();
    counts.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    counts
}

fn activity_series(rows: &[Row], buckets: usize) -> Vec<u64> {
    let mut timestamps = rows
        .iter()
        .filter_map(|row| {
            chrono::DateTime::parse_from_rfc3339(row.ticket.field("created"))
                .ok()
                .map(|value| value.timestamp())
        })
        .collect::<Vec<_>>();
    if timestamps.is_empty() {
        return vec![0; buckets];
    }
    timestamps.sort_unstable();
    let start = *timestamps.first().unwrap_or(&0);
    let end = *timestamps.last().unwrap_or(&start);
    let span = (end - start).max(1);
    let mut output = vec![0_u64; buckets];
    for timestamp in timestamps {
        let index = (((timestamp - start) * (buckets.saturating_sub(1) as i64)) / span) as usize;
        output[index.min(buckets.saturating_sub(1))] += 1;
    }
    output
}

fn layout_columns(layout: TicketLayout, rows: &[Row]) -> Vec<LayoutColumn> {
    match layout {
        TicketLayout::Board => [
            ("Todo", "Open tickets", "open"),
            ("In progress", "Active work", "in_progress"),
            ("Done", "Completed tickets", "closed"),
        ]
        .into_iter()
        .map(|(label, detail, status)| LayoutColumn {
            label: label.to_owned(),
            detail: detail.to_owned(),
            indices: rows
                .iter()
                .enumerate()
                .filter_map(|(index, row)| (row.ticket.status() == status).then_some(index))
                .collect(),
        })
        .collect(),
        TicketLayout::Roadmap | TicketLayout::Dag => dependency_columns(layout, rows),
        _ => vec![LayoutColumn {
            label: layout.label().to_owned(),
            detail: format!("{} tickets", rows.len()),
            indices: (0..rows.len()).collect(),
        }],
    }
}

fn dependency_columns(layout: TicketLayout, rows: &[Row]) -> Vec<LayoutColumn> {
    let by_id = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.ticket.id().to_owned(), index))
        .collect::<HashMap<_, _>>();
    let mut indegree = rows
        .iter()
        .map(|row| (row.ticket.id().to_owned(), 0_usize))
        .collect::<HashMap<_, _>>();
    let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
    let mut missing = HashSet::new();
    for row in rows {
        let mut seen = HashSet::new();
        for dependency in row.ticket.array("deps") {
            if !seen.insert(dependency.clone()) {
                continue;
            }
            if by_id.contains_key(&dependency) {
                *indegree.entry(row.ticket.id().to_owned()).or_default() += 1;
                dependents
                    .entry(dependency)
                    .or_default()
                    .push(row.ticket.id().to_owned());
            } else {
                missing.insert(row.ticket.id().to_owned());
            }
        }
    }
    let mut queue = indegree
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(id.clone()))
        .collect::<Vec<_>>();
    queue.sort();
    let mut queue = VecDeque::from(queue);
    let mut stage = queue
        .iter()
        .map(|id| (id.clone(), 0_usize))
        .collect::<HashMap<_, _>>();
    while let Some(id) = queue.pop_front() {
        let next_stage = stage.get(&id).copied().unwrap_or_default() + 1;
        let mut next = dependents.get(&id).cloned().unwrap_or_default();
        next.sort();
        for dependent in next {
            stage
                .entry(dependent.clone())
                .and_modify(|value| *value = (*value).max(next_stage))
                .or_insert(next_stage);
            let remaining = indegree.entry(dependent.clone()).or_default();
            *remaining = remaining.saturating_sub(1);
            if *remaining == 0 {
                queue.push_back(dependent);
            }
        }
    }
    let mut unresolved = missing;
    unresolved.extend(
        rows.iter()
            .filter(|row| !stage.contains_key(row.ticket.id()))
            .map(|row| row.ticket.id().to_owned()),
    );
    let mut pending = VecDeque::from_iter(unresolved.iter().cloned());
    while let Some(id) = pending.pop_front() {
        for dependent in dependents.get(&id).into_iter().flatten() {
            if unresolved.insert(dependent.clone()) {
                pending.push_back(dependent.clone());
            }
        }
    }
    let max_stage = stage.values().copied().max().unwrap_or_default();
    let mut columns = (0..=max_stage)
        .filter_map(|column| {
            let indices = rows
                .iter()
                .enumerate()
                .filter_map(|(index, row)| {
                    (stage.get(row.ticket.id()) == Some(&column)
                        && !unresolved.contains(row.ticket.id()))
                    .then_some(index)
                })
                .collect::<Vec<_>>();
            (!indices.is_empty()).then(|| LayoutColumn {
                label: if layout == TicketLayout::Roadmap {
                    format!("Stage {}", column + 1)
                } else {
                    format!("Layer {}", column + 1)
                },
                detail: if column == 0 {
                    "Ready prerequisites".to_owned()
                } else {
                    format!("After layer {column}")
                },
                indices,
            })
        })
        .collect::<Vec<_>>();
    if !unresolved.is_empty() {
        columns.push(LayoutColumn {
            label: "Needs attention".to_owned(),
            detail: "Missing or cyclic dependencies".to_owned(),
            indices: rows
                .iter()
                .enumerate()
                .filter_map(|(index, row)| unresolved.contains(row.ticket.id()).then_some(index))
                .collect(),
        });
    }
    columns
}

fn filter_palette_entries(mut entries: Vec<PaletteEntry>, query: &str) -> Vec<PaletteEntry> {
    let normalized = query.trim().to_lowercase();
    if normalized.is_empty() {
        entries.sort_by_key(|entry| if entry.group == "Actions" { 0 } else { 1 });
        return entries;
    }
    entries.retain(|entry| {
        format!("{} {} {}", entry.label, entry.keywords, entry.detail)
            .to_lowercase()
            .contains(&normalized)
    });
    entries.sort_by_key(|entry| if entry.group == "Tickets" { 0 } else { 1 });
    entries
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

    async fn app_with_tickets(name: &str, tickets: Vec<Ticket>) -> (App, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "ferricket-tui-{name}-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let mut app = App::load(root.clone(), false).await.unwrap();
        app.tickets = tickets;
        app.view = TicketView::All;
        app.rebuild_rows();
        (app, root)
    }

    #[tokio::test]
    async fn command_palette_searches_from_one_character_and_executes_every_action_kind() {
        let (mut app, root) = app_with_tickets(
            "palette",
            vec![
                ticket("M1", "", 1),
                ticket("M2", "", 2),
                ticket("jira", "", 3),
            ],
        )
        .await;
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
        )
        .await
        .unwrap();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('M'), KeyModifiers::NONE),
        )
        .await
        .unwrap();
        let matches = app.filtered_palette_entries();
        assert_eq!(
            matches
                .iter()
                .take(2)
                .map(|entry| entry.detail.as_str())
                .collect::<Vec<_>>(),
            ["M1", "M2"]
        );
        assert!(matches.iter().take(2).all(|entry| entry.group == "Tickets"));
        handle_key(&mut app, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
            .await
            .unwrap();
        assert_eq!(app.palette.as_ref().unwrap().selected, 1);
        handle_key(&mut app, KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
            .await
            .unwrap();
        assert_eq!(app.palette.as_ref().unwrap().selected, 0);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE),
        )
        .await
        .unwrap();
        assert_eq!(app.palette.as_ref().unwrap().input, "M1");
        handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await
            .unwrap();
        assert_eq!(app.screen, Screen::Detail);
        assert_eq!(app.selected_ticket().unwrap().id(), "M1");

        app.open_palette();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
        )
        .await
        .unwrap();
        assert_eq!(app.palette.as_ref().unwrap().input, "j");
        assert_eq!(app.filtered_palette_entries()[0].detail, "jira");

        app.open_palette();
        app.palette.as_mut().unwrap().input = "roadmap".into();
        handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await
            .unwrap();
        assert_eq!(app.layout, TicketLayout::Roadmap);
        assert_eq!(app.screen, Screen::List);

        app.open_palette();
        app.palette.as_mut().unwrap().input = "closed tickets".into();
        handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await
            .unwrap();
        assert_eq!(app.view, TicketView::Closed);

        app.open_palette();
        app.palette.as_mut().unwrap().input = "create ticket".into();
        handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await
            .unwrap();
        assert!(app.create_form.is_some());
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn board_columns_and_vim_navigation_follow_visual_positions() {
        let mut todo = ticket("todo", "", 1);
        let mut doing_one = ticket("doing-1", "", 1);
        doing_one
            .fields
            .insert("status".into(), "in_progress".into());
        let mut doing_two = ticket("doing-2", "", 2);
        doing_two
            .fields
            .insert("status".into(), "in_progress".into());
        let mut done = ticket("done", "", 1);
        done.fields.insert("status".into(), "closed".into());
        todo.title = "Todo".into();
        let (mut app, root) =
            app_with_tickets("board-navigation", vec![todo, doing_one, doing_two, done]).await;
        app.set_layout(TicketLayout::Board);
        app.select_id("todo", false);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        )
        .await
        .unwrap();
        assert_eq!(app.selected_ticket().unwrap().id(), "doing-1");
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
        )
        .await
        .unwrap();
        assert_eq!(app.selected_ticket().unwrap().id(), "doing-2");
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        )
        .await
        .unwrap();
        assert_eq!(app.selected_ticket().unwrap().id(), "done");
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
        )
        .await
        .unwrap();
        assert_eq!(app.selected_ticket().unwrap().id(), "doing-1");
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn every_layout_renders_at_narrow_and_wide_terminal_sizes() {
        let mut child = ticket("child", "parent", 0);
        child.fields.insert("deps".into(), "[parent]".into());
        child.fields.insert("tags".into(), "[frontend]".into());
        let mut active = ticket("active", "", 1);
        active.fields.insert("status".into(), "in_progress".into());
        active.fields.insert("deps".into(), "[missing]".into());
        let mut closed = ticket("closed", "", 2);
        closed.fields.insert("status".into(), "closed".into());
        let (mut app, root) = app_with_tickets(
            "render-layouts",
            vec![ticket("parent", "", 1), child, active, closed],
        )
        .await;
        for layout in [
            TicketLayout::Grouped,
            TicketLayout::Tree,
            TicketLayout::Board,
            TicketLayout::Roadmap,
            TicketLayout::Dag,
            TicketLayout::Insights,
        ] {
            app.set_layout(layout);
            for (width, height) in [(48_u16, 16_u16), (132, 36)] {
                app.split_panes = width >= 110 && app.layout_supports_preview();
                app.viewport_rows = usize::from(height.saturating_sub(6));
                let backend = ratatui::backend::TestBackend::new(width, height);
                let mut terminal = Terminal::new(backend).unwrap();
                terminal.draw(|frame| draw(frame, &app)).unwrap();
            }
        }
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn vim_top_bottom_center_and_half_pages_update_selection_and_offsets() {
        let tickets = (0..20)
            .map(|index| ticket(&format!("t{index:02}"), "", index as u8 % 5))
            .collect();
        let (mut app, root) = app_with_tickets("vim-movement", tickets).await;
        app.set_layout(TicketLayout::Tree);
        app.viewport_rows = 6;
        app.selected = 10;
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE),
        )
        .await
        .unwrap();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE),
        )
        .await
        .unwrap();
        assert_eq!(app.selected, 0);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE),
        )
        .await
        .unwrap();
        assert_eq!(app.selected, app.rows.len() - 1);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE),
        )
        .await
        .unwrap();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE),
        )
        .await
        .unwrap();
        assert_eq!(app.list_offset, app.selected.saturating_sub(3));
        let before = app.selected;
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        )
        .await
        .unwrap();
        assert_eq!(app.selected, before - 3);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
        )
        .await
        .unwrap();
        assert_eq!(app.selected, before);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn layout_and_preview_preferences_reload_for_the_workspace() {
        let root = std::env::temp_dir().join(format!(
            "ferricket-tui-preferences-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let mut app = App::load(root.clone(), false).await.unwrap();
        app.layout = TicketLayout::Dag;
        app.preview_visible = false;
        app.view = TicketView::Blocked;
        app.filter = "label:backend".into();
        app.save_preferences().await;
        let restored = App::load(root.clone(), false).await.unwrap();
        assert_eq!(restored.layout, TicketLayout::Dag);
        assert!(!restored.preview_visible);
        assert_eq!(restored.view, TicketView::Blocked);
        assert_eq!(restored.filter, "label:backend");
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn create_validation_keeps_the_form_and_focuses_the_title() {
        let (mut app, root) = app_with_tickets("create-validation", Vec::new()).await;
        let mut form = CreateForm::default();
        form.fields[1] = "Keep this description".into();
        form.focus = form.fields.len() - 1;
        app.create_form = Some(form);
        handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await
            .unwrap();
        let form = app.create_form.as_ref().expect("form should remain open");
        assert_eq!(form.focus, 0);
        assert_eq!(form.fields[1], "Keep this description");
        assert_eq!(app.message, "A title is required");
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn rendered_relationships_and_enter_share_one_filtered_deduplicated_order() {
        let mut main = ticket("main", "", 0);
        main.fields
            .insert("deps".into(), "[missing, shared]".into());
        main.fields.insert("links".into(), "[shared]".into());
        let (mut app, root) =
            app_with_tickets("relationship-order", vec![main, ticket("shared", "", 1)]).await;
        app.select_id("main", false);
        assert_eq!(app.detail_relations(), ["shared"]);
        assert_eq!(
            app.detail_relation_sections(),
            vec![("Depends on", vec!["shared".into()])]
        );
        app.screen = Screen::Detail;
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await
            .unwrap();
        assert_eq!(app.selected_ticket().unwrap().id(), "shared");
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
}
