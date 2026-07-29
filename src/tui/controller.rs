use super::*;

pub(super) async fn handle_event(app: &mut App, event: Event) -> Result<bool> {
    let Event::Key(key) = event else {
        return Ok(false);
    };
    if key.kind == KeyEventKind::Release {
        return Ok(false);
    }
    handle_key(app, key).await
}

pub(super) async fn handle_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    let key = normalize_shifted_key(key);
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
        KeyCode::Down | KeyCode::Char('j') => app.scroll_detail(1),
        KeyCode::Up | KeyCode::Char('k') => app.scroll_detail(-1),
        KeyCode::Char('d')
            if key.modifiers.contains(KeyModifiers::CONTROL) && app.screen == Screen::Detail =>
        {
            app.scroll_detail(10)
        }
        KeyCode::Char('u')
            if key.modifiers.contains(KeyModifiers::CONTROL) && app.screen == Screen::Detail =>
        {
            app.scroll_detail(-10)
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

fn normalize_shifted_key(mut key: KeyEvent) -> KeyEvent {
    if key.modifiers.contains(KeyModifiers::SHIFT)
        && let KeyCode::Char(character) = key.code
        && character.is_ascii_lowercase()
    {
        key.code = KeyCode::Char(character.to_ascii_uppercase());
    }
    key
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
