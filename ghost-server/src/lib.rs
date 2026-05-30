//! ghost-server: Серверная часть протокола Ghost.
//!
//! Этап 2: Сервер
//! - Шаг 4: Приём подключений, разбор фреймов (ID + Addr + Payload)
//! - Шаг 5: Расшифровка ChaCha20-Poly1305
//! - Шаг 6: copy_bidirectional — проксирование в реальный интернет
//! - Шаг 9: Reality-маскировка (TLS + SNI/certificate spoofing + auth)

pub mod handler;

use anyhow::Result;
use ghost_common::ServerConfig;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{debug, info, warn};

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
use tokio_rustls::TlsAcceptor;

    let allowed_users: HashSet<String> = config.allowed_users.iter().cloned().collect();
    let state = Arc::new(ServerState {
        allowed_users,
        config: config.clone(),
    });

    // Генерируем TLS-конфиг с сертификатом для mask_domain
    let tls_config = build_reality_tls_config(&config)?;
    let acceptor = TlsAcceptor::from(Arc::new(tls_config));

    let listener = TcpListener::bind(&config.listen_addr).await?;
    info!("🛡️  Reality server listening on {} (masking as {})",
          config.listen_addr, config.mask_domain);

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let state = state.clone();
        let auth_key = config.reality_auth_key.clone();
        let mask_domain = config.mask_domain.clone();

        tokio::spawn(async move {
            info!("[{}] New TLS connection", peer_addr);

            match acceptor.accept(stream).await {
                Ok(mut tls_stream) => {
                    // Читаем первый HTTP-запрос для auth
                    let mut buf = vec![0u8; 4096];
                    match tls_stream.read(&mut buf).await {
                        Ok(0) => return,
                        Ok(n) => {
                            let is_ghost = detect_ghost_auth(&buf[..n], &auth_key);
                            if is_ghost {
                                // Отвечаем 101 Switching Protocols и передаём в Ghost handler
                                let response = b"HTTP/1.1 101 Switching Protocols\r\n\
                                                  Upgrade: ghost-tunnel\r\n\
                                                  Connection: upgrade\r\n\r\n";
                                if let Err(e) = tls_stream.write_all(response).await {
                                    warn!("[{}] Failed to send 101 response: {}", peer_addr, e);
                                    return;
                                }
                                if let Err(e) = tls_stream.flush().await {
                                    warn!("[{}] Flush error: {}", peer_addr, e);
                                    return;
                                }
                                info!("[{}] Ghost client authenticated via Reality", peer_addr);
                                if let Err(e) = handler::handle_connection(tls_stream, state).await {
                                    warn!("[{}] Ghost handler error: {}", peer_addr, e);
                                }
                            } else {
                                // Сканер/обычный браузер — отдаём фейковый редирект
                                let response = format!(
                                    "HTTP/1.1 301 Moved Permanently\r\n\
                                     Location: https://{}\r\n\
                                     Content-Length: 0\r\n\
                                     Connection: close\r\n\r\n",
                                    mask_domain
                                );
                                let _ = tls_stream.write_all(response.as_bytes()).await;
                            }
                        }
                        Err(e) => {
                            warn!("[{}] TLS read error: {}", peer_addr, e);
                        }
                    }
                }
                Err(e) => {
                    debug!("[{}] TLS handshake failed (possible scanner): {}", peer_addr, e);
                }
            }
        });
    }
}

/// Построить TLS-конфиг для Reality (self-signed cert для mask_domain).
fn build_reality_tls_config(config: &ServerConfig) -> Result<rustls::ServerConfig> {
    use rcgen::{CertificateParams, DistinguishedName, KeyPair};

    let key_pair = KeyPair::generate()
        .map_err(|e| anyhow::anyhow!("Key generation failed: {}", e))?;

    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(rcgen::DnType::CommonName, &config.mask_domain);
    params.distinguished_name = dn;
    params.subject_alt_names.push(
        rcgen::SanType::DnsName(
            rcgen::Ia5String::try_from(config.mask_domain.clone())
                .map_err(|e| anyhow::anyhow!("Invalid domain: {}", e))?
        )
    );

    let cert = params.self_signed(&key_pair)
        .map_err(|e| anyhow::anyhow!("Certificate generation failed: {}", e))?;

    let cert_der = cert.der().to_vec();
    let key_der = key_pair.serialize_der();

    let cert_chain = vec![rustls::pki_types::CertificateDer::from(cert_der)];
    let key_der = rustls::pki_types::PrivateKeyDer::try_from(key_der)
        .map_err(|e| anyhow::anyhow!("Private key error: {:?}", e))?;

    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key_der)
        .map_err(|e| anyhow::anyhow!("TLS config error: {}", e))?;

    Ok(server_config)
}

/// Проверить, содержит ли HTTP-запрос валидный Ghost auth-token.
fn detect_ghost_auth(data: &[u8], auth_key_hex: &str) -> bool {
    let data_str = String::from_utf8_lossy(data);
    if let Some(pos) = data_str.find("X-Ghost-Auth:") {
        let token_start = pos + "X-Ghost-Auth:".len();
        let remainder = &data_str[token_start..];
        let trimmed = remainder.trim_start();
        let token: String = trimmed
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '\r' && *c != '\n')
            .collect();

        // Простая проверка: токен = hex(auth_key)
        if token.len() == 64 && token == auth_key_hex {
            return true;
        }
    }
    false
}
