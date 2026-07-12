use std::{
    future::IntoFuture,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, patch, post},
};
use futures::StreamExt;
use include_dir::{Dir, include_dir};
use serde::{Deserialize, Serialize};
use tokio::{
    net::TcpListener,
    sync::{Mutex, broadcast, watch},
};
use tokio_stream::wrappers::BroadcastStream;

use crate::{
    github, preferences,
    storage::{self, CreateTicket},
};

static WEB: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/web/dist");

#[derive(Clone)]
struct AppState {
    tickets_dir: PathBuf,
    preferences_key: String,
    events: broadcast::Sender<u64>,
    shutdown: watch::Receiver<bool>,
    watch_enabled: bool,
    mutation_lock: std::sync::Arc<Mutex<()>>,
}

#[derive(Serialize)]
struct Meta {
    tickets_dir: String,
    workspace: String,
    version: &'static str,
    watch_enabled: bool,
    current_user: Option<String>,
}

#[derive(Deserialize)]
struct CreateRequest {
    title: String,
    description: Option<String>,
    #[serde(rename = "type")]
    issue_type: Option<String>,
    priority: Option<u8>,
    assignee: Option<String>,
    tags: Option<Vec<String>>,
    parent: Option<String>,
    status: Option<String>,
}

#[derive(Clone, Default, Deserialize)]
struct UpdateRequest {
    status: Option<String>,
    title: Option<String>,
    description: Option<String>,
    priority: Option<u8>,
    assignee: Option<String>,
    tags: Option<Vec<String>>,
    parent: Option<Option<String>>,
}

#[derive(Deserialize)]
struct NoteRequest {
    text: String,
}

#[derive(Deserialize)]
struct AttachmentRequest {
    name: String,
    content: String,
}

#[derive(Serialize)]
struct AttachmentResponse {
    name: String,
    url: String,
    markdown: String,
    media_type: String,
    size: usize,
}

#[derive(Deserialize)]
struct BulkRequest {
    tickets: Vec<BulkTicket>,
    patch: UpdateRequest,
}

#[derive(Deserialize)]
struct BulkTicket {
    id: String,
    revision: String,
}

#[derive(Serialize)]
struct BulkResponse {
    updated: Vec<storage::WebTicket>,
    conflicts: Vec<BulkConflict>,
}

#[derive(Serialize)]
struct BulkConflict {
    id: String,
    message: String,
    latest: Option<storage::WebTicket>,
}

type ApiResult<T> = std::result::Result<T, ApiError>;

struct ApiError {
    status: StatusCode,
    error: Box<anyhow::Error>,
    latest: Option<Box<storage::WebTicket>>,
}

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(error: E) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error: Box::new(error.into()),
            latest: None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.error.to_string(), "latest": self.latest })),
        )
            .into_response()
    }
}

impl ApiError {
    fn bad_request(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error: Box::new(error),
            latest: None,
        }
    }
}

fn required_if_match(headers: &HeaderMap) -> ApiResult<String> {
    let value = headers
        .get(header::IF_MATCH)
        .ok_or_else(|| ApiError {
            status: StatusCode::PRECONDITION_REQUIRED,
            error: Box::new(anyhow::anyhow!("If-Match is required for ticket mutations")),
            latest: None,
        })?
        .to_str()
        .map_err(|error| ApiError::bad_request(error.into()))?;
    parse_revision_tag(value)
}

fn parse_revision_tag(value: &str) -> ApiResult<String> {
    let value = value.trim();
    let revision = value
        .strip_prefix('\"')
        .and_then(|value| value.strip_suffix('\"'))
        .filter(|value| !value.contains('\"'))
        .ok_or_else(|| {
            ApiError::bad_request(anyhow::anyhow!(
                "If-Match must contain a quoted ticket revision"
            ))
        })?;
    if revision.len() != 64 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "If-Match contains an invalid ticket revision"
        )));
    }
    Ok(revision.to_ascii_lowercase())
}

fn validate_patch(request: &UpdateRequest) -> ApiResult<()> {
    if let Some(status) = request.status.as_deref()
        && !matches!(status, "open" | "in_progress" | "closed")
    {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "invalid status '{status}'"
        )));
    }
    if request
        .title
        .as_deref()
        .is_some_and(|title| title.trim().is_empty())
    {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "ticket title cannot be empty"
        )));
    }
    if request.priority.is_some_and(|priority| priority > 4) {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "priority must be between 0 and 4"
        )));
    }
    Ok(())
}

async fn apply_request(
    ticket: &storage::Ticket,
    expected: Option<&str>,
    request: &UpdateRequest,
) -> anyhow::Result<storage::Ticket> {
    storage::apply_changes(
        ticket,
        expected,
        storage::TicketChanges {
            status: request.status.as_deref(),
            title: request.title.as_deref(),
            description: request.description.as_deref(),
            priority: request.priority,
            assignee: request.assignee.as_deref(),
            tags: request.tags.as_deref(),
            parent: request.parent.as_ref().map(|parent| parent.as_deref()),
        },
    )
    .await
}

async fn conflict_error(state: &AppState, id: &str, error: anyhow::Error) -> ApiError {
    let latest = match storage::resolve(&state.tickets_dir, id).await {
        Ok(ticket) => {
            let all = storage::load_all(&state.tickets_dir)
                .await
                .unwrap_or_default();
            Some(Box::new(storage::to_web(
                &ticket,
                &storage::ticket_map(&all),
            )))
        }
        Err(_) => None,
    };
    ApiError {
        status: StatusCode::PRECONDITION_FAILED,
        error: Box::new(error),
        latest,
    }
}

fn with_etag(mut response: Response, revision: &str) -> Response {
    if let Ok(value) = HeaderValue::from_str(&format!("\"{revision}\"")) {
        response.headers_mut().insert(header::ETAG, value);
    }
    response
}

pub async fn serve(
    tickets_dir: PathBuf,
    host: &str,
    port: u16,
    launch_browser: bool,
    watch_enabled: bool,
) -> Result<()> {
    let ip: IpAddr = host
        .parse()
        .with_context(|| format!("Error: invalid host '{host}'"))?;
    let listener = TcpListener::bind(SocketAddr::new(ip, port)).await?;
    let address = listener.local_addr()?;
    let url = format!("http://{address}");
    let (event_sender, _) = broadcast::channel(32);
    let (shutdown_sender, shutdown_receiver) = watch::channel(false);
    if watch_enabled {
        let (watcher, mut receiver) = storage::watch_ticket_files(&tickets_dir).await?;
        let sender = event_sender.clone();
        tokio::spawn(async move {
            let _watcher = watcher;
            let mut version = 0_u64;
            while receiver.recv().await.is_some() {
                tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                while receiver.try_recv().is_ok() {}
                version += 1;
                let _ = sender.send(version);
            }
        });
    }
    let preferences_key = preferences::workspace_key(&tickets_dir).await;
    let state = AppState {
        tickets_dir,
        preferences_key,
        events: event_sender,
        shutdown: shutdown_receiver.clone(),
        watch_enabled,
        mutation_lock: std::sync::Arc::new(Mutex::new(())),
    };
    let app = Router::new()
        .route("/api/meta", get(meta))
        .route("/api/events", get(events))
        .route(
            "/api/preferences/{section}",
            get(preferences).put(save_preferences),
        )
        .route("/api/tickets", get(tickets).post(create_ticket))
        .route("/api/tickets/bulk", patch(bulk_update))
        .route("/api/tickets/{id}", get(ticket).patch(update_ticket))
        .route("/api/tickets/{id}/notes", post(add_note))
        .route(
            "/api/tickets/{id}/notes/{index}",
            patch(edit_note).delete(delete_note),
        )
        .route(
            "/api/tickets/{id}/attachments",
            post(upload_attachment).layer(DefaultBodyLimit::max(28 * 1024 * 1024)),
        )
        .route(
            "/api/tickets/{id}/attachments/{file}",
            get(download_attachment),
        )
        .route("/api/tickets/{id}/open", post(open_ticket_file))
        .route("/api/github/references", get(github_references))
        .fallback(get(static_asset))
        .with_state(state);

    println!("Ferricket UI: {url}");
    if launch_browser {
        let target = url.clone();
        tokio::task::spawn_blocking(move || open::that(target)).await??;
    }
    let mut server_shutdown = shutdown_receiver.clone();
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            if !*server_shutdown.borrow() {
                let _ = server_shutdown.changed().await;
            }
        })
        .into_future();
    tokio::pin!(server);
    tokio::select! {
        result = &mut server => result?,
        () = wait_for_shutdown() => {
            let _ = shutdown_sender.send(true);
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), &mut server).await;
        }
    }
    Ok(())
}

async fn wait_for_shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut terminate = signal(SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = async {
                if let Some(signal) = terminate.as_mut() {
                    signal.recv().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}

async fn meta(State(state): State<AppState>) -> Json<Meta> {
    Json(Meta {
        tickets_dir: state.tickets_dir.display().to_string(),
        workspace: state
            .tickets_dir
            .parent()
            .unwrap_or(&state.tickets_dir)
            .display()
            .to_string(),
        version: env!("CARGO_PKG_VERSION"),
        watch_enabled: state.watch_enabled,
        current_user: crate::query::current_user().await,
    })
}

async fn preferences(
    State(state): State<AppState>,
    Path(section): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(
        preferences::load(&state.preferences_key, &section)
            .await?
            .unwrap_or(serde_json::Value::Null),
    ))
}

async fn save_preferences(
    State(state): State<AppState>,
    Path(section): Path<String>,
    Json(value): Json<serde_json::Value>,
) -> ApiResult<Json<serde_json::Value>> {
    let _mutation = state.mutation_lock.lock().await;
    preferences::save(&state.preferences_key, &section, &value).await?;
    Ok(Json(value))
}

async fn events(
    State(state): State<AppState>,
) -> Sse<impl futures::Stream<Item = std::result::Result<Event, std::convert::Infallible>>> {
    let stream = ticket_event_stream(state.events.subscribe(), state.shutdown.clone());
    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn ticket_event_stream(
    receiver: broadcast::Receiver<u64>,
    mut shutdown: watch::Receiver<bool>,
) -> impl futures::Stream<Item = std::result::Result<Event, std::convert::Infallible>> {
    BroadcastStream::new(receiver)
        .filter_map(|message| async move {
            match message {
                Ok(version) => Some(Ok(Event::default()
                    .event("tickets")
                    .data(version.to_string()))),
                Err(_) => None,
            }
        })
        .take_until(async move {
            if !*shutdown.borrow() {
                let _ = shutdown.changed().await;
            }
        })
}

async fn tickets(State(state): State<AppState>) -> ApiResult<Json<Vec<storage::WebTicket>>> {
    let tickets = storage::load_all(&state.tickets_dir).await?;
    let map = storage::ticket_map(&tickets);
    Ok(Json(
        tickets
            .iter()
            .map(|ticket| storage::to_web(ticket, &map))
            .collect(),
    ))
}

async fn ticket(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Response> {
    let ticket = storage::resolve(&state.tickets_dir, &id).await?;
    let all = storage::load_all(&state.tickets_dir).await?;
    let map = storage::ticket_map(&all);
    let web = storage::to_web(&ticket, &map);
    Ok(with_etag(Json(web).into_response(), &ticket.revision()))
}

async fn create_ticket(
    State(state): State<AppState>,
    Json(request): Json<CreateRequest>,
) -> ApiResult<(StatusCode, Json<storage::WebTicket>)> {
    let _mutation = state.mutation_lock.lock().await;
    if request.title.trim().is_empty() {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "ticket title cannot be empty"
        )));
    }
    if let Some(status) = request.status.as_deref()
        && !matches!(status, "open" | "in_progress" | "closed")
    {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "invalid status '{status}'"
        )));
    }
    if request.priority.is_some_and(|priority| priority > 4) {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "priority must be between 0 and 4"
        )));
    }
    let mut ticket = storage::create(
        &state.tickets_dir,
        CreateTicket {
            title: request.title,
            description: request.description.unwrap_or_default(),
            issue_type: request.issue_type.unwrap_or_else(|| "task".into()),
            priority: request.priority.unwrap_or(2),
            assignee: request.assignee.unwrap_or_default(),
            tags: request.tags.unwrap_or_default(),
            parent: request.parent,
            ..CreateTicket::default()
        },
    )
    .await?;
    if let Some(status) = request.status {
        ticket = storage::update_field(&ticket, "status", &status).await?;
    }
    let all = storage::load_all(&state.tickets_dir).await?;
    let map = storage::ticket_map(&all);
    Ok((StatusCode::CREATED, Json(storage::to_web(&ticket, &map))))
}

async fn update_ticket(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<UpdateRequest>,
) -> ApiResult<Response> {
    let _mutation = state.mutation_lock.lock().await;
    validate_patch(&request)?;
    let ticket = storage::resolve(&state.tickets_dir, &id).await?;
    let expected = required_if_match(&headers)?;
    let ticket = match apply_request(&ticket, Some(&expected), &request).await {
        Ok(ticket) => ticket,
        Err(error) if error.downcast_ref::<storage::RevisionConflict>().is_some() => {
            return Err(conflict_error(&state, &id, error).await);
        }
        Err(error) => return Err(error.into()),
    };
    let all = storage::load_all(&state.tickets_dir).await?;
    let map = storage::ticket_map(&all);
    let web = storage::to_web(&ticket, &map);
    Ok(with_etag(Json(web).into_response(), &ticket.revision()))
}

async fn add_note(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<NoteRequest>,
) -> ApiResult<Response> {
    let _mutation = state.mutation_lock.lock().await;
    let ticket = storage::resolve(&state.tickets_dir, &id).await?;
    let expected = required_if_match(&headers)?;
    let ticket = match storage::append_note_checked(&ticket, Some(&expected), &request.text).await {
        Ok(ticket) => ticket,
        Err(error) if error.downcast_ref::<storage::RevisionConflict>().is_some() => {
            return Err(conflict_error(&state, &id, error).await);
        }
        Err(error) => return Err(error.into()),
    };
    let all = storage::load_all(&state.tickets_dir).await?;
    let map = storage::ticket_map(&all);
    let web = storage::to_web(&ticket, &map);
    Ok(with_etag(Json(web).into_response(), &ticket.revision()))
}

async fn edit_note(
    State(state): State<AppState>,
    Path((id, index)): Path<(String, usize)>,
    headers: HeaderMap,
    Json(request): Json<NoteRequest>,
) -> ApiResult<Response> {
    mutate_note(&state, &id, index, &headers, Some(request.text.as_str())).await
}

async fn delete_note(
    State(state): State<AppState>,
    Path((id, index)): Path<(String, usize)>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    mutate_note(&state, &id, index, &headers, None).await
}

async fn mutate_note(
    state: &AppState,
    id: &str,
    index: usize,
    headers: &HeaderMap,
    replacement: Option<&str>,
) -> ApiResult<Response> {
    let _mutation = state.mutation_lock.lock().await;
    let ticket = storage::resolve(&state.tickets_dir, id).await?;
    let expected = required_if_match(headers)?;
    let result = match replacement {
        Some(text) => storage::edit_note_checked(&ticket, Some(&expected), index, text).await,
        None => storage::delete_note_checked(&ticket, Some(&expected), index).await,
    };
    let ticket = match result {
        Ok(ticket) => ticket,
        Err(error) if error.downcast_ref::<storage::RevisionConflict>().is_some() => {
            return Err(conflict_error(state, id, error).await);
        }
        Err(error) => return Err(error.into()),
    };
    let all = storage::load_all(&state.tickets_dir).await?;
    let web = storage::to_web(&ticket, &storage::ticket_map(&all));
    Ok(with_etag(Json(web).into_response(), &ticket.revision()))
}

async fn bulk_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<BulkRequest>,
) -> ApiResult<Json<BulkResponse>> {
    let _mutation = state.mutation_lock.lock().await;
    validate_patch(&request.patch)?;
    let expected = required_bulk_if_match(&headers)?;
    let mut updated_tickets = Vec::new();
    let mut conflict_records = Vec::new();
    for item in request.tickets {
        if !expected.contains(&item.revision) {
            conflict_records.push(BulkConflict {
                id: item.id,
                message: "If-Match revision does not match the request body".into(),
                latest: None,
            });
            continue;
        }
        let ticket = match storage::resolve(&state.tickets_dir, &item.id).await {
            Ok(ticket) => ticket,
            Err(error) => {
                conflict_records.push(BulkConflict {
                    id: item.id,
                    message: error.to_string(),
                    latest: None,
                });
                continue;
            }
        };
        match apply_request(&ticket, Some(&item.revision), &request.patch).await {
            Ok(ticket) => updated_tickets.push(ticket),
            Err(error) => {
                let latest = storage::resolve(&state.tickets_dir, &item.id).await.ok();
                conflict_records.push(BulkConflict {
                    id: item.id,
                    message: error.to_string(),
                    latest: latest
                        .map(|ticket| storage::to_web(&ticket, &std::collections::HashMap::new())),
                });
            }
        }
    }
    let all = storage::load_all(&state.tickets_dir).await?;
    let map = storage::ticket_map(&all);
    let updated = updated_tickets
        .iter()
        .map(|ticket| storage::to_web(ticket, &map))
        .collect();
    Ok(Json(BulkResponse {
        updated,
        conflicts: conflict_records,
    }))
}

fn required_bulk_if_match(headers: &HeaderMap) -> ApiResult<std::collections::HashSet<String>> {
    let value = headers
        .get(header::IF_MATCH)
        .ok_or_else(|| ApiError {
            status: StatusCode::PRECONDITION_REQUIRED,
            error: Box::new(anyhow::anyhow!("If-Match header is required")),
            latest: None,
        })?
        .to_str()
        .map_err(|error| ApiError::bad_request(error.into()))?;
    let revisions = value
        .split(',')
        .map(parse_revision_tag)
        .collect::<ApiResult<std::collections::HashSet<_>>>()?;
    if revisions.is_empty() {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "If-Match must include ticket revisions"
        )));
    }
    Ok(revisions)
}

async fn open_ticket_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let ticket = storage::resolve(&state.tickets_dir, &id).await?;
    let path = ticket.path;
    tokio::task::spawn_blocking(move || open::that(path)).await??;
    Ok(StatusCode::NO_CONTENT)
}

async fn upload_attachment(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<AttachmentRequest>,
) -> ApiResult<(StatusCode, Json<AttachmentResponse>)> {
    use base64::Engine;

    const MAX_ATTACHMENT_SIZE: usize = 20 * 1024 * 1024;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(request.content.trim())
        .map_err(|_| ApiError::bad_request(anyhow::anyhow!("invalid attachment data")))?;
    if bytes.is_empty() || bytes.len() > MAX_ATTACHMENT_SIZE {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "attachment must be between 1 byte and 20 MiB"
        )));
    }
    let _guard = state.mutation_lock.lock().await;
    let ticket = storage::resolve(&state.tickets_dir, &id).await?;
    let ticket_id = ticket.id().to_owned();
    let path =
        storage::save_attachment(&state.tickets_dir, &ticket_id, &request.name, &bytes).await?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("attachment")
        .to_owned();
    let media_type = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .to_string();
    let url = format!(
        "/api/tickets/{}/attachments/{}",
        encode_path_component(&ticket_id),
        encode_path_component(&name)
    );
    let relative_url = format!(
        "attachments/{}/{}",
        encode_path_component(&ticket_id),
        encode_path_component(&name)
    );
    let markdown = if previewable_image(&media_type) {
        format!("![{}]({relative_url})", markdown_text(&name))
    } else {
        format!("[{}]({relative_url})", markdown_text(&name))
    };
    Ok((
        StatusCode::CREATED,
        Json(AttachmentResponse {
            name,
            url,
            markdown,
            media_type,
            size: bytes.len(),
        }),
    ))
}

fn previewable_image(media_type: &str) -> bool {
    matches!(
        media_type,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/avif"
    )
}

async fn download_attachment(
    State(state): State<AppState>,
    Path((id, file)): Path<(String, String)>,
) -> Response {
    match storage::read_attachment(&state.tickets_dir, &id, &file).await {
        Ok((path, bytes)) => {
            let media_type = mime_guess::from_path(&path).first_or_octet_stream();
            let safe_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("attachment");
            let disposition = if media_type.as_ref() == "application/pdf"
                || previewable_image(media_type.as_ref())
            {
                "inline"
            } else {
                "attachment"
            };
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, media_type.as_ref())
                .header("X-Content-Type-Options", "nosniff")
                .header(
                    "Content-Security-Policy",
                    "sandbox; default-src 'none'; style-src 'unsafe-inline'",
                )
                .header(header::CACHE_CONTROL, "private, max-age=3600")
                .header(
                    header::CONTENT_DISPOSITION,
                    format!("{disposition}; filename=\"{safe_name}\""),
                )
                .body(Body::from(bytes))
                .unwrap()
        }
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
        {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}

async fn github_references(
    State(state): State<AppState>,
    axum::extract::Query(parameters): axum::extract::Query<
        std::collections::HashMap<String, String>,
    >,
) -> Json<github::ReferenceResponse> {
    let workspace = state.tickets_dir.parent().unwrap_or(&state.tickets_dir);
    Json(
        github::references(
            workspace,
            parameters.get("q").map(String::as_str).unwrap_or(""),
        )
        .await,
    )
}

fn encode_path_component(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn markdown_text(value: &str) -> String {
    value.replace(['[', ']'], "_")
}

async fn static_asset(uri: axum::http::Uri) -> Response {
    if uri.path().starts_with("/api/") || uri.path() == "/api" {
        return StatusCode::NOT_FOUND.into_response();
    }
    let requested = uri.path().trim_start_matches('/');
    let requested = if requested.is_empty() {
        "index.html"
    } else {
        requested
    };
    let requested_file = WEB.get_file(requested);
    let app_shell = requested == "index.html" || requested_file.is_none();
    let file = requested_file.or_else(|| WEB.get_file("index.html"));
    match file {
        Some(file) => {
            let mime = mime_guess::from_path(file.path()).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .header(
                    header::CACHE_CONTROL,
                    if app_shell {
                        "no-cache"
                    } else {
                        "public, max-age=31536000, immutable"
                    },
                )
                .body(Body::from(file.contents().to_vec()))
                .unwrap()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::http::{Uri, header};
    use futures::StreamExt;
    use tokio::sync::{broadcast, watch};

    use super::{parse_revision_tag, required_bulk_if_match, static_asset, ticket_event_stream};

    #[test]
    fn bulk_mutations_require_ticket_revisions_in_if_match() {
        let mut headers = axum::http::HeaderMap::new();
        assert!(required_bulk_if_match(&headers).is_err());
        headers.insert(
            header::IF_MATCH,
            axum::http::HeaderValue::from_static(
                "\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\", \"abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789\"",
            ),
        );
        let revisions = match required_bulk_if_match(&headers) {
            Ok(revisions) => revisions,
            Err(_) => panic!("valid If-Match revisions should parse"),
        };
        assert!(
            revisions.contains("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
        assert!(
            revisions.contains("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
        );
    }

    #[test]
    fn ticket_revision_tags_must_be_strong_quoted_sha256_values() {
        let revision = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let parsed = parse_revision_tag(&format!("\"{revision}\""));
        assert!(parsed.is_ok());
        assert_eq!(parsed.ok().as_deref(), Some(revision));
        for invalid in [
            revision,
            "\"\"",
            "*",
            "W/\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"",
            "\"not-a-revision\"",
            "\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\", \"extra\"",
        ] {
            assert!(parse_revision_tag(invalid).is_err(), "accepted {invalid:?}");
        }
    }

    #[test]
    fn only_safe_raster_images_are_inline_previewable() {
        assert!(super::previewable_image("image/png"));
        assert!(super::previewable_image("image/webp"));
        assert!(!super::previewable_image("image/svg+xml"));
        assert!(!super::previewable_image("text/html"));
    }

    #[tokio::test]
    async fn deep_routes_serve_uncached_app_shell() {
        let response = static_asset(Uri::from_static("/all/ticket/fer-example")).await;

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
        assert_eq!(response.headers()[header::CONTENT_TYPE], "text/html");
    }

    #[tokio::test]
    async fn unknown_api_routes_return_not_found_instead_of_the_app_shell() {
        let response = static_asset(Uri::from_static("/api/unknown")).await;
        assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn live_event_stream_ends_on_server_shutdown() {
        let (_events, receiver) = broadcast::channel(2);
        let (shutdown, shutdown_receiver) = watch::channel(false);
        let stream = ticket_event_stream(receiver, shutdown_receiver);
        tokio::pin!(stream);

        shutdown.send(true).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), stream.next())
                .await
                .expect("event stream should stop promptly")
                .is_none()
        );
    }
}
