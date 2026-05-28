//! ghost-server: Серверная часть протокола Ghost.
//!
//! Этап 2: Сервер
//! - Шаг 4: Приём подключений, разбор фреймов (ID + Addr + Payload)
//! - Шаг 5: Расшифровка ChaCha20-Poly1305
//! - Шаг 6: copy_bidirectional — проксирование в реальный интернет
//! - Шаг 9: Reality-маскировка (SNI/certificate spoofing)

pub mod handler;

use anyhow::Result;
use ghost_common::ServerConfig;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{info, warn};

/// Состояние сервера, разделяемое между соединениями.
pub struct ServerState {
    /// Набор разрешённых User ID (16-байтовые, хранятся как hex-строки)
    pub allowed_users: HashSet<String>,
    /// Конфигурация
    pub config: ServerConfig,
}

/// Запустить Ghost-сервер.
pub async fn run(config: ServerConfig) -> Result<()> {
    // Инициализация логирования
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ghost_server=info".into()),
        )
        .init();

    info!("Ghost Server starting...");
    info!("   Listen  : {}", config.listen_addr);
    info!("   Users   : {}", config.allowed_users.len());
    info!("   Padding : {} (max {} bytes)",
          config.enable_padding, config.max_padding_len);
    info!("   Reality : {} (mask: {})",
          config.enable_reality, config.mask_domain);

    // Если включена Reality — запускаем TLS-обёртку
    if config.enable_reality {
        info!("   Reality mode enabled, starting TLS wrapper...");
        run_with_reality(config).await
    } else {
        run_plain(config).await
    }
}

/// Обычный режим (без TLS-маскировки).
async fn run_plain(config: ServerConfig) -> Result<()> {
    let allowed_users: HashSet<String> = config.allowed_users.iter().cloned().collect();
    let state = Arc::new(ServerState {
        allowed_users,
        config: config.clone(),
    });

    let listener = TcpListener::bind(&config.listen_addr).await?;
    info!("Ghost server listening on {}", config.listen_addr);

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let state = state.clone();

        tokio::spawn(async move {
            info!("[{}] New connection", peer_addr);
            if let Err(e) = handler::handle_connection(stream, state).await {
                warn!("[{}] Connection error: {}", peer_addr, e);
            }
        });
    }
}

/// Reality-режим: TLS-маскировка под легальный сервер.
async fn run_with_reality(config: ServerConfig) -> Result<()> {
    // Парсим auth key из hex
    let auth_key_hex = &config.reality_auth_key;
    let mut auth_key = [0u8; 32];
    if auth_key_hex.len() == 64 {
        for i in 0..32 {
            auth_key[i] = u8::from_str_radix(&auth_key_hex[i * 2..i * 2 + 2], 16)
                .unwrap_or(0);
        }
    } else {
        warn!("Invalid reality_auth_key length (expected 64 hex chars), using zeros");
    }

    let reality_config = ghost_reality::RealityConfig {
        mask_domain: config.mask_domain.clone(),
        auth_key,
        verify_ja4: config.verify_ja4,
        allowed_ja4: config.allowed_ja4.clone(),
        session_ttl_secs: 300,
    };

    let reality_server = ghost_reality::RealityServer::new(reality_config)
        .map_err(|e| anyhow::anyhow!("Reality server init failed: {}", e))?;

    info!("Reality server created, masking as {}", config.mask_domain);

    // Запускаем Reality TLS-сервер
    reality_server.run(&config.listen_addr).await
        .map_err(|e| anyhow::anyhow!("Reality server error: {}", e))
}
