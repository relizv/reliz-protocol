//! Reliz Protocol — Tauri v2 приложение.
//!
//! Три команды:
//! - `connect(token)` — парсит rlz_ токен, запускает SOCKS5 прокси
//! - `disconnect()`   — останавливает прокси
//! - `get_status()`   — возвращает статус и аптайм

use ghost_common::token::ConnectionToken;
use serde::Serialize;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

const STATUS_STOPPED: i32 = 0;
const STATUS_CONNECTING: i32 = 1;
const STATUS_CONNECTED: i32 = 2;
const STATUS_ERROR: i32 = 3;

/// Shared state — Arc-обёртка для безопасной передачи в tokio::spawn.
struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    status: AtomicI32,
    cancel_token: Mutex<Option<CancellationToken>>,
    connected_at: Mutex<Option<std::time::Instant>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                status: AtomicI32::new(STATUS_STOPPED),
                cancel_token: Mutex::new(None),
                connected_at: Mutex::new(None),
            }),
        }
    }
}

#[derive(Serialize)]
struct StatusResponse {
    status: i32,
    uptime_secs: u64,
}

#[tauri::command]
async fn connect(
    token: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let current = state.inner.status.load(Ordering::SeqCst);
    if current == STATUS_CONNECTED || current == STATUS_CONNECTING {
        return Err("Already connected".into());
    }

    let parsed = ConnectionToken::decode(&token).map_err(|e| format!("Bad token: {e}"))?;
    let config = parsed.to_client_config();

    info!("Connecting to {} (mask: {})", config.server_addr, config.mask_domain);
    state.inner.status.store(STATUS_CONNECTING, Ordering::SeqCst);

    let cancel = CancellationToken::new();
    *state.inner.cancel_token.lock().await = Some(cancel.clone());

    let inner = state.inner.clone();

    tokio::spawn(async move {
        let result = tokio::select! {
            r = ghost_client::run(config) => r,
            _ = cancel.cancelled() => {
                info!("Proxy stopped by user");
                Ok(())
            }
        };

        if let Err(e) = result {
            error!("Proxy error: {e}");
            inner.status.store(STATUS_ERROR, Ordering::SeqCst);
        } else {
            inner.status.store(STATUS_STOPPED, Ordering::SeqCst);
        }
        *inner.connected_at.lock().await = None;
    });

    state.inner.status.store(STATUS_CONNECTED, Ordering::SeqCst);
    *state.inner.connected_at.lock().await = Some(std::time::Instant::now());

    Ok(())
}

#[tauri::command]
async fn disconnect(state: tauri::State<'_, AppState>) -> Result<(), String> {
    if let Some(cancel) = state.inner.cancel_token.lock().await.take() {
        cancel.cancel();
    }
    state.inner.status.store(STATUS_STOPPED, Ordering::SeqCst);
    *state.inner.connected_at.lock().await = None;
    info!("Disconnected");
    Ok(())
}

#[tauri::command]
async fn get_status(state: tauri::State<'_, AppState>) -> Result<StatusResponse, String> {
    Ok(StatusResponse {
        status: state.inner.status.load(Ordering::SeqCst),
        uptime_secs: state
            .inner
            .connected_at
            .lock()
            .await
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0),
    })
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ghost_client=info,reliz_app=info".into()),
        )
        .init();

    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![connect, disconnect, get_status])
        .run(tauri::generate_context!())
        .expect("error while running Reliz Protocol");
}
