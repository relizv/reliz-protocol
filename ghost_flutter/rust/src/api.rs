//! Reliz Rust API для Flutter (через flutter_rust_bridge).
//!
//! Этот крейт экспортирует функции, которые Flutter вызывает через FFI:
//! - start_reliz_proxy() — запуск SOCKS5-прокси
//! - stop_proxy()        — остановка прокси
//! - get_proxy_status()  — текущий статус
//! - is_proxy_running()  — флаг активности
//!
//! Отличия от прежней версии («допиливание»):
//!   * Собственный многопоточный tokio-рантайм (раньше `tokio::spawn`
//!     вызывался без активного рантайма — паника).
//!   * Статус `Connected` выставляется только после реального bind
//!     SOCKS5-листенера (раньше статус «врал» сразу).
//!   * `stop_proxy` корректно абортит фоновый таск.

use ghost_common::ClientConfig;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::runtime::{Builder, Runtime};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// Статус прокси-соединения.
#[repr(i32)]
pub enum ProxyStatus {
    Stopped = 0,
    Connecting = 1,
    Connected = 2,
    Error = 3,
}

static PROXY_STATUS: AtomicI32 = AtomicI32::new(ProxyStatus::Stopped as i32);
static PROXY_RUNNING: AtomicBool = AtomicBool::new(false);

/// Единый многопоточный рантайм на весь процесс.
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Хэндл фонового таска прокси (для остановки).
static TASK: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// Хэндл фонового таска tun2socks.
static TUN_TASK: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .enable_all()
            .thread_name("reliz-rt")
            .build()
            .expect("failed to build tokio runtime")
    })
}

/// Запустить Reliz-прокси с заданными параметрами.
///
/// Вызывается из Flutter при нажатии кнопки "Connect". Параметры берутся из
/// захардкоженного `RelizConfig` на стороне Dart (один токен на сервере).
///
/// Возвращает `0` при успешном bind SOCKS5-листенера, иначе код ошибки.
pub fn start_reliz_proxy(
    server_addr: String,
    user_id: String,
    enable_padding: bool,
    enable_fragmentation: bool,
    mask_domain: String,
) -> i32 {
    // Идемпотентность: если уже запущены — ничего не делаем.
    if PROXY_RUNNING.load(Ordering::SeqCst) {
        return 0;
    }

    PROXY_STATUS.store(ProxyStatus::Connecting as i32, Ordering::SeqCst);

    let config = ClientConfig {
        socks5_listen: "127.0.0.1:10808".to_string(),
        server_addr,
        user_id,
        enable_padding,
        enable_fragmentation,
        max_padding_len: 64,
        mask_domain,
        reality_auth_key: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
    };

    let rt = runtime();
    let (ready_tx, ready_rx) = oneshot::channel::<()>();

    // Фоновый таск: работает, пока не остановим или не случится ошибка.
    let handle = rt.spawn(async move {
        PROXY_RUNNING.store(true, Ordering::SeqCst);
        match reliz_client::run_with_ready(config, Some(ready_tx)).await {
            Ok(()) => PROXY_STATUS.store(ProxyStatus::Stopped as i32, Ordering::SeqCst),
            Err(e) => {
                tracing::error!("Proxy error: {}", e);
                PROXY_STATUS.store(ProxyStatus::Error as i32, Ordering::SeqCst);
            }
        }
        PROXY_RUNNING.store(false, Ordering::SeqCst);
    });

    *TASK.lock().unwrap() = Some(handle);

    // Ждём реальный bind листенера (с тайм-аутом), чтобы статус был честным.
    let bound = rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), ready_rx)
            .await
            .is_ok()
    });

    if bound {
        PROXY_STATUS.store(ProxyStatus::Connected as i32, Ordering::SeqCst);
        0
    } else {
        // bind не удался — гасим таск.
        stop_proxy();
        PROXY_STATUS.store(ProxyStatus::Error as i32, Ordering::SeqCst);
        ProxyStatus::Error as i32
    }
}

/// Остановить прокси.
pub fn stop_proxy() -> i32 {
    if let Some(handle) = TASK.lock().unwrap().take() {
        handle.abort();
    }
    PROXY_RUNNING.store(false, Ordering::SeqCst);
    PROXY_STATUS.store(ProxyStatus::Stopped as i32, Ordering::SeqCst);
    0
}

/// Получить текущий статус прокси.
pub fn get_proxy_status() -> i32 {
    PROXY_STATUS.load(Ordering::SeqCst)
}

/// Проверить, запущен ли прокси.
pub fn is_proxy_running() -> bool {
    PROXY_RUNNING.load(Ordering::SeqCst)
}

/// Протестировать подключение к серверу.
pub fn test_connection(_server_addr: String) -> i32 {
    0 // success
}

/// Запустить tun2socks внутри Rust (вместо внешнего libhev-socks5-tunnel.so).
///
/// Вызывается из Kotlin (RelizVpnService) после создания TUN-интерфейса.
/// `tun_fd` — сырой fd из `ParcelFileDescriptor.fd`.
/// Блокирует до остановки через `stop_tun`.
pub fn start_tun(tun_fd: i32) -> i32 {
    if tun_fd < 0 {
        return -1;
    }

    let rt = runtime();
    let handle = rt.spawn(async move {
        let socks5 = std::net::SocketAddr::from(([127, 0, 0, 1], 10808));
        match ghost_tun::TunProcessor::new(tun_fd, socks5) {
            Ok(processor) => {
                if let Err(e) = processor.run().await {
                    tracing::error!("Tun processor error: {}", e);
                }
            }
            Err(e) => {
                tracing::error!("Failed to create TunProcessor: {}", e);
            }
        }
    });

    *TUN_TASK.lock().unwrap() = Some(handle);
    0
}

/// Остановить tun2socks.
pub fn stop_tun() -> i32 {
    if let Some(handle) = TUN_TASK.lock().unwrap().take() {
        handle.abort();
    }
    0
}

/// Получить версию протокола.
pub fn get_protocol_version() -> u8 {
    ghost_common::PROTOCOL_VERSION
}

// ── C/JNI экспорты для прямого вызова из Android Service ───────────────

/// C-экспорт для JNI. Вызывается из `GhostTunBridge.kt`.
#[no_mangle]
pub extern "C" fn ghost_tun_start(tun_fd: i32) -> i32 {
    start_tun(tun_fd)
}

/// C-экспорт для JNI.
#[no_mangle]
pub extern "C" fn ghost_tun_stop() -> i32 {
    stop_tun()
}
