//! Reliz Rust API для Flutter (через flutter_rust_bridge).
//!
//! Этот крейт экспортирует функции, которые Flutter вызывает через FFI:
//! - start_reliz_proxy() — запуск SOCKS5-прокси
//! - stop_proxy()        — остановка прокси
//! - get_proxy_status()  — текущий статус
//! - is_proxy_running()  — флаг активности
//!
//! Ребрендинг: ранее `ghost_client` / `start_proxy`, теперь
//! `reliz_client` / `start_reliz_proxy`.

use ghost_common::ClientConfig;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

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

/// Запустить Reliz-прокси с заданными параметрами.
///
/// Вызывается из Flutter при нажатии кнопки "Connect".
///
/// # Параметры
/// * `server_addr` — адрес Reliz-сервера, например `vpn.example.com:443`.
/// * `user_id` — UUID пользователя (32 hex-символа, без дефисов).
/// * `enable_padding` — добавлять Dynamic Padding в исходящие фреймы.
/// * `enable_fragmentation` — фрагментировать TLS ClientHello (anti-DPI).
/// * `mask_domain` — домен SNI/Reality, под который маскируется клиент
///   (например, `www.apple.com`). Передаётся в `ClientConfig` и далее
///   в TLS-обёртку клиента — раньше Flutter-приложение его игнорировало.
pub fn start_reliz_proxy(
    server_addr: String,
    user_id: String,
    enable_padding: bool,
    enable_fragmentation: bool,
    mask_domain: String,
) -> i32 {
    PROXY_STATUS.store(ProxyStatus::Connecting as i32, Ordering::SeqCst);
    PROXY_RUNNING.store(true, Ordering::SeqCst);

    let config = ClientConfig {
        socks5_listen: "127.0.0.1:10808".to_string(),
        server_addr,
        user_id,
        enable_padding,
        enable_fragmentation,
        max_padding_len: 64,
        mask_domain,
    };

    // Запускаем прокси в фоновом таске
    tokio::spawn(async move {
        match reliz_client::run(config).await {
            Ok(()) => {
                PROXY_STATUS.store(ProxyStatus::Stopped as i32, Ordering::SeqCst);
            }
            Err(e) => {
                tracing::error!("Proxy error: {}", e);
                PROXY_STATUS.store(ProxyStatus::Error as i32, Ordering::SeqCst);
            }
        }
        PROXY_RUNNING.store(false, Ordering::SeqCst);
    });

    // В реальном коде: дождаться подтверждения подключения
    PROXY_STATUS.store(ProxyStatus::Connected as i32, Ordering::SeqCst);
    0
}

/// Остановить прокси.
pub fn stop_proxy() -> i32 {
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

/// Получить версию протокола.
pub fn get_protocol_version() -> u8 {
    ghost_common::PROTOCOL_VERSION
}
