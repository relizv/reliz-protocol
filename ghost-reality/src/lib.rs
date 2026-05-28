//! ghost-reality: Reality-маскировка протокола Ghost.
//!
//! Ключевые возможности:
//! - **JA4 Fingerprint Spoofing**: подмена TLS-отпечатка под Chrome
//! - **Reality Server**: при сканировании цензором — прикидывается легальным сайтом
//! - **SNI Masquerading**: подмена Server Name Indication
//! - **Auth-за-ECDHE**: валидация клиента на основе shared secret внутри TLS-хэндшейка
//!
//! Архитектура:
//! ```text
//!   Цензор сканирует порт 443 → видит легальный TLS-сервер (apple.com)
//!   Настоящий клиент → передаёт auth-token внутри ClientHello → получает прокси
//! ```

pub mod ja4;
pub mod reality_server;
pub mod tls_wrapper;

// Реэкспорт основных типов для удобства
pub use reality_server::RealityServer;

use thiserror::Error;

/// Ошибка Reality-модуля.
#[derive(Error, Debug)]
pub enum RealityError {
    #[error("TLS handshake failed: {0}")]
    TlsHandshake(String),

    #[error("client authentication failed: invalid auth token")]
    AuthFailed,

    #[error("SNI mismatch: expected {expected}, got {got}")]
    SniMismatch { expected: String, got: String },

    #[error("JA4 fingerprint rejected: {0}")]
    Ja4Rejected(String),

    #[error("certificate generation failed: {0}")]
    CertificateError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Конфигурация Reality-маскировки.
#[derive(Debug, Clone)]
pub struct RealityConfig {
    /// Домен, под который маскируется сервер (для SNI и сертификата)
    pub mask_domain: String,

    /// Приватный ключ авторизации (shared secret между клиентом и сервером)
    pub auth_key: [u8; 32],

    /// Включить JA4-проверку клиентов
    pub verify_ja4: bool,

    /// Разрешённые JA4-отпечатки (если verify_ja4 = true)
    pub allowed_ja4: Vec<String>,

    /// Время жизни сессии в секундах
    pub session_ttl_secs: u64,
}

impl Default for RealityConfig {
    fn default() -> Self {
        Self {
            mask_domain: "www.apple.com".to_string(),
            auth_key: [0u8; 32],
            verify_ja4: false,
            allowed_ja4: vec![],
            session_ttl_secs: 300,
        }
    }
}
