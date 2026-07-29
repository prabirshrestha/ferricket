use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::{self, IsTerminal, Write},
    path::PathBuf,
    sync::Arc,
};

use anyhow::{Result, bail};
use crossterm::{
    event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
#[cfg(test)]
use ratatui::Terminal;
use ratatui::{layout::Rect, style::Color, text::Line};

use crate::{
    preferences, query,
    storage::{self, CreateTicket, Ticket},
    tui_input,
};

mod model;
mod runtime;
use model::{dependency_rows, flat_rows, grouped_rows, hierarchy_rows, layout_columns};
pub(crate) use runtime::run;
#[cfg(test)]
use runtime::update_viewport;

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

#[derive(Clone, Debug, PartialEq, Eq)]
struct DetailCacheKey {
    revision: String,
    width: u16,
    include_identity: bool,
    selected_relation: Option<usize>,
}

struct PreparedDetail {
    key: DetailCacheKey,
    lines: Vec<Line<'static>>,
    height: u16,
}

struct TicketStore {
    tickets: Vec<Ticket>,
    ticket_by_id: HashMap<String, usize>,
    blocked_ids: HashSet<String>,
}

impl TicketStore {
    fn new(tickets: Vec<Ticket>) -> Self {
        let ticket_by_id = tickets
            .iter()
            .enumerate()
            .map(|(index, ticket)| (ticket.id().to_owned(), index))
            .collect::<HashMap<_, _>>();
        let blocked_ids = tickets
            .iter()
            .filter(|ticket| {
                ticket.array("deps").iter().any(|id| {
                    ticket_by_id
                        .get(id)
                        .is_none_or(|index| tickets[*index].status() != "closed")
                })
            })
            .map(|ticket| ticket.id().to_owned())
            .collect();
        Self {
            tickets,
            ticket_by_id,
            blocked_ids,
        }
    }

    fn ticket(&self, id: &str) -> Option<&Ticket> {
        self.ticket_by_id
            .get(id)
            .and_then(|index| self.tickets.get(*index))
    }
}

struct LayoutModel {
    store: Arc<TicketStore>,
    rows: Vec<Row>,
    grouped_sections: Vec<LayoutColumn>,
    layout_columns: Vec<LayoutColumn>,
    column_position_by_row: Vec<Option<(usize, usize)>>,
}

impl LayoutModel {
    fn build(
        tickets: Vec<Ticket>,
        layout: TicketLayout,
        filter: &str,
        view: TicketView,
        current_user: Option<&str>,
    ) -> Self {
        Self::project(
            Arc::new(TicketStore::new(tickets)),
            layout,
            filter,
            view,
            current_user,
        )
    }

    fn project(
        store: Arc<TicketStore>,
        layout: TicketLayout,
        filter: &str,
        view: TicketView,
        current_user: Option<&str>,
    ) -> Self {
        let rows = match layout {
            TicketLayout::Tree => hierarchy_rows(&store, filter, view, current_user),
            TicketLayout::Roadmap | TicketLayout::Dag => {
                dependency_rows(&store, filter, view, current_user)
            }
            TicketLayout::Grouped => grouped_rows(&store, filter, view, current_user),
            TicketLayout::Board | TicketLayout::Insights => {
                flat_rows(&store, filter, view, current_user)
            }
        };
        let layout_columns = layout_columns(layout, &store, &rows);
        let grouped_sections = ["in_progress", "open", "closed"]
            .into_iter()
            .filter_map(|status| {
                let indices = rows
                    .iter()
                    .enumerate()
                    .filter_map(|(index, row)| {
                        (store.tickets[row.ticket].status() == status).then_some(index)
                    })
                    .collect::<Vec<_>>();
                (!indices.is_empty()).then(|| LayoutColumn {
                    label: status.to_owned(),
                    detail: String::new(),
                    indices,
                })
            })
            .collect();
        let mut column_position_by_row = vec![None; rows.len()];
        for (column_index, column) in layout_columns.iter().enumerate() {
            for (position, row_index) in column.indices.iter().enumerate() {
                if let Some(slot) = column_position_by_row.get_mut(*row_index) {
                    *slot = Some((column_index, position));
                }
            }
        }
        Self {
            store,
            rows,
            grouped_sections,
            layout_columns,
            column_position_by_row,
        }
    }
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
    ticket: usize,
    depth: usize,
    last: bool,
}

struct App {
    dir: PathBuf,
    model: LayoutModel,
    data_epoch: u64,
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
    detail_viewport_width: u16,
    detail_viewport_rows: u16,
    prepared_detail: Option<PreparedDetail>,
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
        let model = LayoutModel::build(tickets, layout, &filter, view, current_user.as_deref());
        let app = Self {
            dir,
            model,
            data_epoch: 0,
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
            detail_viewport_width: 78,
            detail_viewport_rows: 14,
            prepared_detail: None,
            pending_g: false,
            pending_z: false,
        };
        Ok(app)
    }

    async fn refresh(&mut self) -> Result<()> {
        let tickets = storage::load_all(&self.dir).await?;
        self.replace_tickets(tickets);
        Ok(())
    }

    fn replace_tickets(&mut self, tickets: Vec<Ticket>) {
        let model = LayoutModel::build(
            tickets,
            self.layout,
            &self.filter,
            self.view,
            self.current_user.as_deref(),
        );
        self.replace_model(model);
    }

    fn replace_model(&mut self, model: LayoutModel) {
        let selected_id = self.selected_ticket().map(|ticket| ticket.id().to_owned());
        self.model = model;
        self.invalidate_data_epoch();
        self.prepared_detail = None;
        if let Some(id) = selected_id
            && let Some(index) = self
                .model
                .rows
                .iter()
                .position(|row| self.model.store.tickets[row.ticket].id() == id)
        {
            self.selected = index;
        }
        self.clamp_selection();
    }

    fn rebuild_rows(&mut self) {
        let model = LayoutModel::project(
            Arc::clone(&self.model.store),
            self.layout,
            &self.filter,
            self.view,
            self.current_user.as_deref(),
        );
        self.model = model;
        self.invalidate_data_epoch();
        self.prepared_detail = None;
        self.clamp_selection();
    }

    fn set_view(&mut self, view: TicketView) {
        self.view = view;
        self.selected = 0;
        self.reset_offsets();
        self.rebuild_rows();
        self.message = format!("{} view · {} tickets", view.label(), self.model.rows.len());
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

    fn invalidate_data_epoch(&mut self) {
        self.data_epoch = self.data_epoch.wrapping_add(1);
    }

    fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.model.rows.len().saturating_sub(1));
    }

    fn selected_ticket(&self) -> Option<&Ticket> {
        self.model
            .rows
            .get(self.selected)
            .and_then(|row| self.model.store.tickets.get(row.ticket))
    }

    fn ticket(&self, id: &str) -> Option<&Ticket> {
        self.model.store.ticket(id)
    }

    fn move_selection(&mut self, delta: isize) {
        if self.model.rows.is_empty() {
            return;
        }
        let index = self.selected as isize + delta;
        self.selected = index.clamp(0, self.model.rows.len() as isize - 1) as usize;
        self.detail_relation = 0;
        self.detail_scroll = 0;
        self.ensure_selection_visible();
    }

    fn select_top(&mut self) {
        if self.model.rows.is_empty() {
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
        if self.model.rows.is_empty() {
            return;
        }
        if self.layout_uses_columns() {
            let Some(column) = self.layout_column() else {
                return;
            };
            let indices = self.layout_column_indices(column);
            let bottom = indices.last().copied();
            let offset = indices.len().saturating_sub(self.viewport_rows);
            if let Some(index) = bottom {
                self.selected = index;
                self.set_column_offset(column, offset);
            }
        } else {
            self.selected = self.model.rows.len() - 1;
            self.list_offset = self.model.rows.len().saturating_sub(self.viewport_rows);
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
        let Some((column, position)) = self.layout_column_position() else {
            return;
        };
        let indices = self.layout_column_indices(column);
        let next = (position as isize + delta).clamp(0, indices.len() as isize - 1) as usize;
        self.selected = indices[next];
        self.reset_detail_position();
        self.ensure_selection_visible();
    }

    fn move_column_horizontal(&mut self, delta: isize) {
        let Some((column, row)) = self.layout_column_position() else {
            return;
        };
        let mut next = column as isize + delta;
        let count = self.model.layout_columns.len() as isize;
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

    fn layout_column(&self) -> Option<usize> {
        self.layout_column_position().map(|(column, _)| column)
    }

    fn layout_column_position(&self) -> Option<(usize, usize)> {
        self.model
            .column_position_by_row
            .get(self.selected)
            .copied()
            .flatten()
    }

    fn layout_column_indices(&self, column: usize) -> &[usize] {
        self.model
            .layout_columns
            .get(column)
            .map(|column| column.indices.as_slice())
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
            let Some((column, position)) = self.layout_column_position() else {
                return;
            };
            let mut offset = self.column_offset(column);
            if position < offset {
                offset = position;
            } else if position >= offset + height {
                offset = position + 1 - height;
            }
            self.set_column_offset(column, offset);
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
            self.model
                .store
                .tickets
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
            .chain(self.model.store.tickets.iter().map(|ticket| {
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
            .model
            .rows
            .iter()
            .position(|row| self.model.store.tickets[row.ticket].id() == id)
            .or_else(|| {
                self.view = TicketView::All;
                self.filter.clear();
                self.rebuild_rows();
                self.model
                    .rows
                    .iter()
                    .position(|row| self.model.store.tickets[row.ticket].id() == id)
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
        let mut seen = HashSet::new();
        let mut section = |heading: &'static str, ids: Vec<String>| {
            let ids = ids
                .into_iter()
                .filter(|id| {
                    self.model.store.ticket_by_id.contains_key(id) && seen.insert(id.clone())
                })
                .collect::<Vec<_>>();
            (!ids.is_empty()).then_some((heading, ids))
        };
        let mut sections = Vec::new();
        if self
            .model
            .store
            .ticket_by_id
            .contains_key(ticket.field("parent"))
        {
            sections.extend(section("Parent", vec![ticket.field("parent").to_owned()]));
        }
        sections.extend(section(
            "Sub-tickets",
            self.model
                .store
                .tickets
                .iter()
                .filter(|candidate| candidate.field("parent") == ticket.id())
                .map(|candidate| candidate.id().to_owned())
                .collect(),
        ));
        sections.extend(section("Depends on", ticket.array("deps")));
        sections.extend(section(
            "Blocks",
            self.model
                .store
                .tickets
                .iter()
                .filter(|candidate| candidate.array("deps").iter().any(|id| id == ticket.id()))
                .map(|candidate| candidate.id().to_owned())
                .collect(),
        ));
        sections.extend(section("Related", ticket.array("links")));
        sections
    }

    fn detail_bottom(&mut self) -> u16 {
        self.prepare_detail(false);
        let content_height = self
            .prepared_detail
            .as_ref()
            .map_or(0, |prepared| prepared.height);
        content_height.saturating_sub(self.detail_viewport_rows)
    }

    fn prepare_detail(&mut self, include_identity: bool) {
        let Some(ticket) = self.selected_ticket() else {
            self.prepared_detail = None;
            return;
        };
        let selected_relation = (self.screen == Screen::Detail
            || (include_identity && self.pane_focus == PaneFocus::Details))
            .then_some(self.detail_relation);
        let key = DetailCacheKey {
            revision: ticket.revision(),
            width: self.detail_viewport_width.max(1),
            include_identity,
            selected_relation,
        };
        if self
            .prepared_detail
            .as_ref()
            .is_some_and(|prepared| prepared.key == key)
        {
            return;
        }
        let lines = detail_lines(self, include_identity);
        let height = render::detail_height(&lines, key.width);
        self.prepared_detail = Some(PreparedDetail { key, lines, height });
    }

    fn scroll_detail(&mut self, delta: isize) {
        let delta = i16::try_from(delta).unwrap_or(if delta.is_negative() {
            i16::MIN
        } else {
            i16::MAX
        });
        let bottom = self.detail_bottom();
        self.detail_scroll = self.detail_scroll.saturating_add_signed(delta).min(bottom);
    }

    fn clamp_detail_scroll(&mut self) {
        if self.detail_scroll > 0 {
            let bottom = self.detail_bottom();
            self.detail_scroll = self.detail_scroll.min(bottom);
        }
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
                    .model
                    .rows
                    .iter()
                    .find(|row| {
                        let target = self.model.store.tickets[row.ticket].title.to_lowercase();
                        target.contains(&query)
                    })
                    .map(|row| self.model.store.tickets[row.ticket].id().to_owned())
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

mod controller;
use controller::handle_event;
#[cfg(test)]
use controller::handle_key;

mod render;
#[cfg(test)]
use render::centered;
use render::{detail_lines, draw};

fn insight_counts<F>(store: &TicketStore, rows: &[Row], key: F) -> Vec<(String, usize)>
where
    F: Fn(&Ticket) -> String,
{
    let mut counts = HashMap::new();
    for row in rows {
        *counts.entry(key(&store.tickets[row.ticket])).or_insert(0) += 1;
    }
    let mut counts = counts.into_iter().collect::<Vec<_>>();
    counts.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    counts
}

fn insight_label_counts(store: &TicketStore, rows: &[Row]) -> Vec<(String, usize)> {
    let mut counts = HashMap::new();
    for row in rows {
        let tags = store.tickets[row.ticket].array("tags");
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

fn activity_series(store: &TicketStore, rows: &[Row], buckets: usize) -> Vec<u64> {
    let mut timestamps = rows
        .iter()
        .filter_map(|row| {
            chrono::DateTime::parse_from_rfc3339(store.tickets[row.ticket].field("created"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::Backend;
    use std::{collections::BTreeMap, time::SystemTime};

    pub(super) fn ticket(id: &str, parent: &str, priority: u8) -> Ticket {
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

    fn hierarchy_ids(tickets: Vec<Ticket>, filter: &str, view: TicketView) -> Vec<String> {
        let store = TicketStore::new(tickets);
        hierarchy_rows(&store, filter, view, None)
            .into_iter()
            .map(|row| store.tickets[row.ticket].id().to_owned())
            .collect()
    }

    #[test]
    fn hierarchy_keeps_parent_before_children_and_filter_context() {
        let tickets = vec![
            ticket("child", "parent", 0),
            ticket("other", "", 1),
            ticket("parent", "", 3),
        ];
        let store = TicketStore::new(tickets);
        let rows = hierarchy_rows(&store, "child", TicketView::Active, None);
        assert_eq!(
            rows.iter()
                .map(|row| store.tickets[row.ticket].id())
                .collect::<Vec<_>>(),
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

        let ids = |view| hierarchy_ids(tickets.clone(), "", view);
        assert_eq!(ids(TicketView::Active), ["active", "blocker"]);
        assert_eq!(ids(TicketView::All), ["active", "closed", "blocker"]);
        assert_eq!(ids(TicketView::Blocked), ["active"]);
        assert_eq!(ids(TicketView::Closed), ["closed"]);
    }

    #[tokio::test]
    async fn shifted_lowercase_reports_preserve_uppercase_shortcuts_and_text() {
        let (mut app, root) = app_with_tickets("shifted-keys", vec![ticket("one", "", 1)]).await;
        app.set_layout(TicketLayout::Tree);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('v'), KeyModifiers::SHIFT),
        )
        .await
        .unwrap();
        assert_eq!(app.layout, TicketLayout::Board);

        app.open_prompt(Prompt::Filter);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SHIFT),
        )
        .await
        .unwrap();
        assert_eq!(app.input, "A");
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[test]
    fn query_uses_advanced_syntax_and_keeps_ancestor_context() {
        let mut child = ticket("child", "parent", 0);
        child.fields.insert("tags".into(), "[frontend]".into());
        child.fields.insert("assignee".into(), "Ada".into());
        let tickets = vec![child, ticket("other", "", 1), ticket("parent", "", 3)];

        assert_eq!(
            hierarchy_ids(
                tickets,
                "status:todo label:frontend -assignee:unassigned",
                TicketView::Active,
            ),
            ["parent", "child"]
        );
    }

    #[test]
    fn query_context_may_include_an_ancestor_outside_the_selected_view() {
        let child = ticket("child", "parent", 0);
        let mut parent = ticket("parent", "", 3);
        parent.fields.insert("status".into(), "closed".into());
        let tickets = vec![child, parent];

        assert_eq!(
            hierarchy_ids(tickets, "id:child", TicketView::Active),
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
        app.replace_tickets(vec![main, dependency, blocked]);
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
        app.replace_tickets(vec![ticket("one", "", 0), ticket("two", "", 1)]);
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
        app.replace_tickets(vec![ticket("demo", "", 2)]);
        app.rebuild_rows();
        app.screen = Screen::Detail;
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE),
        )
        .await
        .unwrap();
        let bottom = app.detail_bottom();
        assert_eq!(app.detail_scroll, bottom);
        handle_key(&mut app, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
            .await
            .unwrap();
        let bottom = app.detail_bottom();
        assert_eq!(app.detail_scroll, bottom);
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn key_repeats_move_until_release_and_release_is_inert() {
        let root = std::env::temp_dir().join(format!(
            "ferricket-tui-navigation-repeat-{}",
            std::process::id()
        ));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let mut app = App::load(root.clone(), false).await.unwrap();
        app.replace_tickets(
            (0..300)
                .map(|index| ticket(&format!("ticket-{index:03}"), "", 2))
                .collect(),
        );
        app.view = TicketView::All;
        app.rebuild_rows();

        handle_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
        )
        .await
        .unwrap();
        for _ in 0..10 {
            handle_event(
                &mut app,
                Event::Key(KeyEvent::new_with_kind(
                    KeyCode::Down,
                    KeyModifiers::NONE,
                    KeyEventKind::Repeat,
                )),
            )
            .await
            .unwrap();
        }
        handle_event(
            &mut app,
            Event::Key(KeyEvent::new_with_kind(
                KeyCode::Down,
                KeyModifiers::NONE,
                KeyEventKind::Release,
            )),
        )
        .await
        .unwrap();

        assert_eq!(app.selected, 11);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn large_list_keeps_global_selection_and_renders_completely() {
        let root =
            std::env::temp_dir().join(format!("ferricket-tui-large-list-{}", std::process::id()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let mut app = App::load(root.clone(), false).await.unwrap();
        app.replace_tickets(
            (0..10_000)
                .map(|index| ticket(&format!("ticket-{index:05}"), "", 2))
                .collect(),
        );
        app.view = TicketView::All;
        app.layout = TicketLayout::Tree;
        app.rebuild_rows();
        update_viewport(&mut app, Rect::new(0, 0, 100, 24));
        app.select_bottom();
        assert_eq!(app.selected, app.model.rows.len() - 1);

        let backend = ratatui::backend::TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        for _ in 0..20 {
            app.move_selection(-1);
            terminal.draw(|frame| draw(frame, &app)).unwrap();
        }
        assert_eq!(app.selected, app.model.rows.len() - 21);
        assert!(app.list_offset <= app.selected);
        assert!(app.selected < app.list_offset + app.viewport_rows);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn cached_columns_and_detail_bounds_stay_valid_during_navigation() {
        let tickets = (0..2_000)
            .map(|index| {
                let mut item = ticket(&format!("ticket-{index:05}"), "", 2);
                if index > 0 {
                    item.fields
                        .insert("deps".into(), format!("[ticket-{:05}]", index - 1));
                }
                item.raw = format!(
                    "---\nid: ticket-{index:05}\n---\n# ticket-{index:05}\n\n{}",
                    "a long wrapped detail line ".repeat(200)
                );
                item
            })
            .collect();
        let (mut app, root) = app_with_tickets("cached-navigation", tickets).await;
        app.set_layout(TicketLayout::Roadmap);
        update_viewport(&mut app, Rect::new(0, 0, 120, 30));

        assert_eq!(app.model.column_position_by_row.len(), app.model.rows.len());
        assert!(app.model.column_position_by_row.iter().all(Option::is_some));
        for _ in 0..1_000 {
            app.move_column_horizontal(1);
        }
        assert_eq!(app.selected, 1_000);

        app.screen = Screen::Detail;
        update_viewport(&mut app, Rect::new(0, 0, 120, 30));
        for _ in 0..1_000 {
            app.scroll_detail(1);
        }
        assert!(app.prepared_detail.is_some());
        let bottom = app.detail_bottom();
        assert_eq!(app.detail_scroll, bottom.min(1_000));
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn resize_recomputes_layout_and_keeps_the_selection_visible() {
        let tickets = (0..100)
            .map(|index| ticket(&format!("ticket-{index:03}"), "", 2))
            .collect();
        let (mut app, root) = app_with_tickets("resize", tickets).await;
        app.view = TicketView::All;
        app.layout = TicketLayout::Tree;
        app.rebuild_rows();
        app.selected = 80;

        update_viewport(&mut app, Rect::new(0, 0, 140, 40));
        assert!(app.split_panes);
        assert_eq!(app.viewport_rows, 34);
        assert!((app.list_offset..app.list_offset + app.viewport_rows).contains(&app.selected));

        update_viewport(&mut app, Rect::new(0, 0, 70, 12));
        assert!(!app.split_panes);
        assert_eq!(app.viewport_rows, 6);
        assert!((app.list_offset..app.list_offset + app.viewport_rows).contains(&app.selected));
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
        app.replace_tickets(vec![ticket("one", "", 1), ticket("two", "", 2)]);
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
        assert_eq!(app.model.rows.len(), 1);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    pub(super) async fn app_with_tickets(name: &str, tickets: Vec<Ticket>) -> (App, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "ferricket-tui-{name}-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let mut app = App::load(root.clone(), false).await.unwrap();
        app.replace_tickets(tickets);
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
        assert_eq!(app.selected, app.model.rows.len() - 1);
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
