//! Web UI: встроенный HTTP-сервер для управления прокси.
//!
//! Маршруты:
//! - `GET /`               → HTML-страница (встроена в бинарь)
//! - `POST /api/connect`   → парсит токен, запускает SOCKS5-туннель
//! - `POST /api/disconnect`→ останавливает прокси
//! - `GET /api/status`     → JSON: `{status, uptime_secs}`

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use ghost_common::token::ConnectionToken;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicI32, Ordering},
    Arc,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

const INDEX_HTML: &str = include_str!("../static/index.html");

// Статусы прокси — совпадают с фронтендом.
const STATUS_STOPPED: i32 = 0;
const STATUS_CONNECTING: i32 = 1;
const STATUS_CONNECTED: i32 = 2;
const STATUS_ERROR: i32 = 3;

/// Разделяемое состояние между HTTP-хэндлерами и прокси-задачей.
pub struct AppState {
    status: AtomicI32,
    cancel_token: Mutex<Option<CancellationToken>>,
    connected_at: Mutex<Option<std::time::Instant>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            status: AtomicI32::new(STATUS_STOPPED),
            cancel_token: Mutex::new(None),
            connected_at: Mutex::new(None),
        }
    }
}

/// Запустить Web UI сервер на заданном адресе.
pub async fn serve(listen_addr: &str) -> anyhow::Result<()> {
    let state = Arc::new(AppState::new());

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/api/connect", post(connect_handler))
        .route("/api/disconnect", post(disconnect_handler))
        .route("/api/status", get(status_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    info!("🌐 Web UI listening on http://{}", listen_addr);

    axum::serve(listener, app).await?;
    Ok(())
}

// ── Handlers ──────────────────────────────────────────────────────────────

async fn index_handler() -> Html<&'static str> {
    Html(INDEX_HTML)
}

#[derive(Deserialize)]
struct ConnectRequest {
    token: String,
}

#[derive(Serialize)]
struct ApiResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct StatusResponse {
    status: i32,
    uptime_secs: u64,
}

async fn connect_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConnectRequest>,
) -> impl IntoResponse {
    // Если уже подключены — ошибка
    let current = state.status.load(Ordering::SeqCst);
    if current == STATUS_CONNECTED || current == STATUS_CONNECTING {
        return (
            StatusCode::CONFLICT,
            Json(ApiResponse {
                ok: false,
                error: Some("Already connected".to_string()),
            }),
        );
    }

    // Парсим токен
    let token = match ConnectionToken::decode(&req.token) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse {
                    ok: false,
                    error: Some(format!("Invalid token: {}", e)),
                }),
            );
        }
    };

    let config = token.to_client_config();
    info!("Connecting to {} via {}", config.server_addr, config.mask_domain);

    state.status.store(STATUS_CONNECTING, Ordering::SeqCst);

    // Создаём CancellationToken для graceful shutdown
    let cancel = CancellationToken::new();
    {
        let mut lock = state.cancel_token.lock().await;
        *lock = Some(cancel.clone());
    }

    // Запускаем прокси в фоне
    let state_bg = state.clone();
    tokio::spawn(async move {
        // Запускаем run с cancellation
        let proxy_result = tokio::select! {
            result = crate::run(config) => result,
            _ = cancel.cancelled() => {
                info!("Proxy cancelled by user");
                Ok(())
            }
        };

        match proxy_result {
            Ok(()) => {
                info!("Proxy stopped normally");
            }
            Err(e) => {
                error!("Proxy error: {}", e);
                state_bg.status.store(STATUS_ERROR, Ordering::SeqCst);
                return;
            }
        }

        state_bg.status.store(STATUS_STOPPED, Ordering::SeqCst);
        *state_bg.connected_at.lock().await = None;
    });

    // Ставим статус connected (listener bind практически мгновенный)
    state.status.store(STATUS_CONNECTED, Ordering::SeqCst);
    *state.connected_at.lock().await = Some(std::time::Instant::now());

    (
        StatusCode::OK,
        Json(ApiResponse {
            ok: true,
            error: None,
        }),
    )
}

async fn disconnect_handler(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let current = state.status.load(Ordering::SeqCst);
    if current == STATUS_STOPPED {
        return (
            StatusCode::OK,
            Json(ApiResponse {
                ok: true,
                error: None,
            }),
        );
    }

    // Отменяем прокси через CancellationToken
    {
        let mut lock = state.cancel_token.lock().await;
        if let Some(cancel) = lock.take() {
            cancel.cancel();
        }
    }

    state.status.store(STATUS_STOPPED, Ordering::SeqCst);
    *state.connected_at.lock().await = None;

    info!("Proxy disconnected by user");

    (
        StatusCode::OK,
        Json(ApiResponse {
            ok: true,
            error: None,
        }),
    )
}

async fn status_handler(
    State(state): State<Arc<AppState>>,
) -> Json<StatusResponse> {
    let status = state.status.load(Ordering::SeqCst);
    let uptime = state
        .connected_at
        .lock()
        .await
        .map(|t| t.elapsed().as_secs())
        .unwrap_or(0);

    Json(StatusResponse {
        status,
        uptime_secs: uptime,
    })
}
