use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Sparkline, Wrap},
};

use super::{
    ACCENT, App, BORDER, MUTED, PaneFocus, Prompt, Row, Screen, Ticket, TicketLayout,
    activity_series, insight_counts, insight_label_counts,
};

pub(super) fn draw(frame: &mut Frame<'_>, app: &App) {
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
    let sections = &app.model.grouped_sections;
    if sections.is_empty() {
        draw_empty_layout(frame, area, app);
        return;
    }
    let constraints = sections
        .iter()
        .map(|section| Constraint::Length((section.indices.len() as u16 + 2).min(10)))
        .chain(std::iter::once(Constraint::Min(0)))
        .collect::<Vec<_>>();
    let chunks = Layout::vertical(constraints).split(area);
    for (group_index, section) in sections.iter().enumerate() {
        let status = section.label.as_str();
        let indices = &section.indices;
        let selected = indices.iter().position(|index| *index == app.selected);
        let capacity = usize::from(chunks[group_index].height.saturating_sub(2)).max(1);
        let start = selected
            .map(|position| position.saturating_sub(capacity / 2))
            .unwrap_or_default()
            .min(indices.len().saturating_sub(capacity));
        let end = (start + capacity).min(indices.len());
        let items = indices[start..end]
            .iter()
            .map(|index| ListItem::new(ticket_line(app, &app.model.rows[*index], false)))
            .collect::<Vec<_>>();
        let mut state = ListState::default().with_selected(
            selected
                .and_then(|position| (start..end).contains(&position).then_some(position - start)),
        );
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
    let columns = &app.model.layout_columns;
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
        let selected = app
            .model
            .column_position_by_row
            .get(app.selected)
            .copied()
            .flatten()
            .and_then(|(selected_column, position)| {
                (selected_column == column_index).then_some(position)
            });
        let item_height = if app.layout == TicketLayout::Dag {
            3
        } else {
            2
        };
        let capacity = usize::from(chunks[slot].height.saturating_sub(2) / item_height).max(1);
        let offset = app
            .column_offset(column_index)
            .min(column.indices.len().saturating_sub(capacity));
        let end = (offset + capacity).min(column.indices.len());
        let items = column.indices[offset..end]
            .iter()
            .map(|index| {
                let row = &app.model.rows[*index];
                let ticket = &app.model.store.tickets[row.ticket];
                let mut lines = vec![
                    ticket_line(app, row, true),
                    Line::styled(
                        format!("  {}", column_ticket_detail(app.layout, ticket)),
                        Style::default().fg(MUTED),
                    ),
                ];
                if app.layout == TicketLayout::Dag {
                    let dependencies = ticket.array("deps");
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
        let mut state = ListState::default().with_selected(selected.and_then(|position| {
            (offset..end)
                .contains(&position)
                .then_some(position - offset)
        }));
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
                .model
                .rows
                .iter()
                .filter(|row| app.model.store.tickets[row.ticket].status() == status)
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
                format!("{} tickets in {}", app.model.rows.len(), app.view.label()),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::styled(statuses, Style::default().fg(MUTED)),
        ])
        .block(panel(" Overview ")),
        chunks[0],
    );
    let series = activity_series(&app.model.store, &app.model.rows, 24);
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
            insight_counts(&app.model.store, &app.model.rows, |ticket| {
                format!("P{}", ticket.priority())
            }),
        ),
        (
            "Type",
            insight_counts(&app.model.store, &app.model.rows, |ticket| {
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
            insight_counts(&app.model.store, &app.model.rows, |ticket| {
                let value = ticket.field("assignee");
                if value.is_empty() {
                    "Unassigned".to_owned()
                } else {
                    value.to_owned()
                }
            }),
        ),
        (
            "Labels",
            insight_label_counts(&app.model.store, &app.model.rows),
        ),
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
    let capacity = usize::from(area.height.saturating_sub(2)).max(1);
    let start = app
        .list_offset
        .min(app.model.rows.len().saturating_sub(capacity));
    let end = (start + capacity).min(app.model.rows.len());
    let items = app.model.rows[start..end]
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
            app.model.rows.len()
        )
    } else {
        format!(
            " {focus} {} · {} · {} · /{} ",
            app.view.label(),
            app.layout.label(),
            app.model.rows.len(),
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
    let mut state = ListState::default().with_selected(
        (start..end)
            .contains(&app.selected)
            .then_some(app.selected - start),
    );
    frame.render_stateful_widget(list, area, &mut state);
}

fn ticket_line<'a>(app: &App, row: &'a Row, tree_visible: bool) -> Line<'a> {
    let ticket = &app.model.store.tickets[row.ticket];
    let tree = if !tree_visible || row.depth == 0 {
        String::new()
    } else {
        format!(
            "{}{} ",
            "  ".repeat(row.depth.saturating_sub(1)),
            if row.last { "└─" } else { "├─" }
        )
    };
    let blocked = app.model.store.blocked_ids.contains(ticket.id());
    Line::from(vec![
        Span::styled(
            if app.selected_ids.contains(ticket.id()) {
                "✓ "
            } else {
                "  "
            },
            Style::default().fg(ACCENT),
        ),
        Span::styled(
            format!("{:<3}", status_glyph(ticket.status())),
            Style::default().fg(status_color(ticket.status())),
        ),
        Span::styled(
            format!("P{} ", ticket.priority()),
            priority_style(ticket.priority()),
        ),
        Span::styled(
            format!("{}{:<10}", tree, ticket.id()),
            Style::default().fg(MUTED),
        ),
        Span::raw(" "),
        Span::styled(ticket.title.clone(), Style::default().fg(Color::White)),
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
    if app.selected_ticket().is_none() {
        frame.render_widget(
            Paragraph::new("No tickets yet. Press c to create one.").block(panel(" Preview ")),
            area,
        );
        return;
    }
    let block = Block::default()
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
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(prepared) = app.prepared_detail.as_ref() else {
        return;
    };
    frame.render_widget(
        Paragraph::new(prepared.lines.as_slice())
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0)),
        inner,
    );
}

pub(super) fn detail_height(lines: &[Line<'static>], width: u16) -> u16 {
    u16::try_from(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .line_count(width.max(1)),
    )
    .unwrap_or(u16::MAX)
    .max(1)
}

pub(super) fn detail_lines(app: &App, include_identity: bool) -> Vec<Line<'static>> {
    let Some(ticket) = app.selected_ticket() else {
        return Vec::new();
    };
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
                Span::styled(ticket.id().to_owned(), Style::default().fg(ACCENT)),
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
            app,
            selected_relation,
            &mut relation_index,
        );
    }
    let notes = ticket.notes();
    if !notes.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "Notes",
            Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
        ));
        for note in notes {
            if let Some(timestamp) = note.timestamp {
                lines.push(Line::styled(timestamp, Style::default().fg(MUTED)));
            }
            lines.extend(note.text.lines().map(|line| Line::raw(format!("  {line}"))));
        }
    }
    lines
}

fn relationship_lines(
    lines: &mut Vec<Line<'static>>,
    heading: &str,
    ids: &[String],
    app: &App,
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
        let title = app
            .ticket(id)
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

pub(super) fn centered(area: Rect, width: u16, height: u16) -> Rect {
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
