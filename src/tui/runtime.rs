use super::*;
use ratatui::{Terminal, backend::CrosstermBackend};

#[derive(Clone)]
struct RefreshRequest {
    epoch: u64,
    dir: PathBuf,
    layout: TicketLayout,
    view: TicketView,
    filter: String,
    current_user: Option<String>,
}

struct RefreshResult {
    epoch: u64,
    layout: TicketLayout,
    view: TicketView,
    filter: String,
    model: Result<LayoutModel>,
}

async fn build_refresh(request: RefreshRequest) -> RefreshResult {
    let RefreshRequest {
        epoch,
        dir,
        layout,
        view,
        filter,
        current_user,
    } = request;
    let result_filter = filter.clone();
    let model = async {
        let tickets = storage::load_all(&dir).await?;
        tokio::task::spawn_blocking(move || {
            LayoutModel::build(tickets, layout, &filter, view, current_user.as_deref())
        })
        .await
        .map_err(anyhow::Error::from)
    }
    .await;
    RefreshResult {
        epoch,
        layout,
        view,
        filter: result_filter,
        model,
    }
}

struct TerminalGuard {
    keyboard_enhancement: bool,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }
        let keyboard_enhancement = execute!(
            io::stdout(),
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
            )
        )
        .is_ok();
        Ok(Self {
            keyboard_enhancement,
        })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.keyboard_enhancement {
            let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
        }
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
        let _ = io::stdout().flush();
    }
}

pub(crate) async fn run(dir: PathBuf, watch_enabled: bool) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("Error: fer tui requires an interactive terminal");
    }
    let _guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    let mut input = tui_input::InputCoordinator::spawn();
    let mut app = App::load(dir.clone(), watch_enabled).await?;
    let (watcher, mut changes) = if watch_enabled {
        let (watcher, receiver) = storage::watch_ticket_files(&dir).await?;
        (Some(watcher), Some(receiver))
    } else {
        (None, None)
    };
    let _watcher = watcher;

    update_viewport(&mut app, terminal.size()?.into());
    terminal.draw(|frame| draw(frame, &app))?;
    let mut redraw = tokio::time::interval(std::time::Duration::from_millis(16));
    redraw.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    redraw.tick().await;
    let mut dirty = false;
    let mut refresh_deadline = None;
    let (refresh_requests, refresh_rx) = tokio::sync::watch::channel(None);
    let (refresh_results_tx, mut refresh_results) = tokio::sync::mpsc::channel(1);
    let refresh_worker = tokio::spawn(refresh_worker(refresh_rx, refresh_results_tx));

    loop {
        tokio::select! {
            event = input.next(text_input_owns_chars(&app)) => {
                let Some(event) = event? else { break };
                if handle_event(&mut app, event).await? {
                    break;
                }
                dirty = true;
            }
            change = async {
                match changes.as_mut() {
                    Some(receiver) => receiver.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if change.is_none() {
                    changes = None;
                    refresh_deadline = None;
                } else if app.live_updates {
                    app.invalidate_data_epoch();
                    refresh_deadline = Some(
                        tokio::time::Instant::now() + std::time::Duration::from_millis(120),
                    );
                }
            }
            _ = async {
                match refresh_deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending().await,
                }
            } => {
                refresh_deadline = None;
                if app.live_updates {
                    refresh_requests.send_replace(Some(RefreshRequest {
                        epoch: app.data_epoch,
                        dir: dir.clone(),
                        layout: app.layout,
                        view: app.view,
                        filter: app.filter.clone(),
                        current_user: app.current_user.clone(),
                    }));
                }
            }
            result = refresh_results.recv() => {
                let Some(result) = result else { continue };
                if result.epoch == app.data_epoch
                    && app.live_updates
                    && app.layout == result.layout
                    && app.view == result.view
                    && app.filter == result.filter
                {
                    app.replace_model(result.model?);
                    app.message = "Updated from disk".into();
                    dirty = true;
                }
            }
            _ = redraw.tick(), if dirty => {
                update_viewport(&mut app, terminal.size()?.into());
                terminal.draw(|frame| draw(frame, &app))?;
                dirty = false;
            }
        }
    }
    drop(refresh_requests);
    refresh_worker.abort();
    drop(input);
    app.save_preferences().await;
    Ok(())
}

async fn refresh_worker(
    mut requests: tokio::sync::watch::Receiver<Option<RefreshRequest>>,
    results: tokio::sync::mpsc::Sender<RefreshResult>,
) {
    loop {
        if requests.changed().await.is_err() {
            break;
        }
        let Some(request) = requests.borrow_and_update().clone() else {
            continue;
        };
        if results.send(build_refresh(request).await).await.is_err() {
            break;
        }
    }
}

fn text_input_owns_chars(app: &App) -> bool {
    app.prompt.is_some() || app.palette.is_some() || app.create_form.is_some()
}

pub(super) fn update_viewport(app: &mut App, terminal_size: Rect) {
    app.split_panes =
        terminal_size.width >= 110 && app.preview_visible && app.layout_supports_preview();
    app.viewport_rows = if app.layout_uses_columns() {
        usize::from(terminal_size.height.saturating_sub(7) / 4).max(1)
    } else {
        usize::from(terminal_size.height.saturating_sub(6)).max(1)
    };
    let previous_detail_size = (app.detail_viewport_width, app.detail_viewport_rows);
    if app.screen == Screen::Detail {
        app.detail_viewport_width = terminal_size.width.saturating_sub(2).max(1);
        app.detail_viewport_rows = terminal_size.height.saturating_sub(10).max(1);
    } else if app.split_panes {
        app.detail_viewport_width = terminal_size
            .width
            .saturating_mul(45)
            .checked_div(100)
            .unwrap_or(1)
            .saturating_sub(2)
            .max(1);
        app.detail_viewport_rows = terminal_size.height.saturating_sub(6).max(1);
    }
    app.ensure_selection_visible();
    if previous_detail_size != (app.detail_viewport_width, app.detail_viewport_rows) {
        app.clamp_detail_scroll();
    }
    match app.screen {
        Screen::Detail => app.prepare_detail(false),
        Screen::List if app.split_panes => app.prepare_detail(true),
        _ => app.prepared_detail = None,
    }
    if !app.split_panes {
        app.pane_focus = PaneFocus::List;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn refresh_worker_serializes_and_coalesces_requests() {
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let completed = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let (requests, mut receiver) = tokio::sync::watch::channel(None::<usize>);
        let active_worker = Arc::clone(&active);
        let max_worker = Arc::clone(&max_active);
        let completed_worker = Arc::clone(&completed);
        let worker = tokio::spawn(async move {
            while receiver.changed().await.is_ok() {
                let Some(value) = *receiver.borrow_and_update() else {
                    continue;
                };
                let now = active_worker.fetch_add(1, Ordering::SeqCst) + 1;
                max_worker.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                active_worker.fetch_sub(1, Ordering::SeqCst);
                completed_worker.lock().await.push(value);
            }
        });
        requests.send_replace(Some(1));
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        for value in 2..=20 {
            requests.send_replace(Some(value));
        }
        tokio::time::sleep(std::time::Duration::from_millis(70)).await;
        drop(requests);
        worker.await.unwrap();
        assert_eq!(max_active.load(Ordering::SeqCst), 1);
        assert_eq!(*completed.lock().await, vec![1, 20]);
    }

    #[tokio::test]
    async fn hidden_detail_surfaces_do_not_prepare_ticket_content() {
        let (mut app, root) = super::super::tests::app_with_tickets(
            "hidden-detail",
            vec![super::super::tests::ticket("long", "", 1)],
        )
        .await;
        app.preview_visible = false;
        for layout in [
            TicketLayout::Tree,
            TicketLayout::Board,
            TicketLayout::Roadmap,
            TicketLayout::Dag,
            TicketLayout::Insights,
        ] {
            app.set_layout(layout);
            update_viewport(&mut app, Rect::new(0, 0, 140, 40));
            assert!(app.prepared_detail.is_none(), "{layout:?}");
        }
        app.preview_visible = true;
        app.set_layout(TicketLayout::Tree);
        update_viewport(&mut app, Rect::new(0, 0, 80, 20));
        assert!(app.prepared_detail.is_none());
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
}
