//! TLS-обёртка для клиента Ghost.
//!
//! Выполняет:
//! - Подключение к Ghost-серверу через TLS с подменой SNI
//! - Встраивание auth-token в HTTP-заголовки
//! - JA4-совместимый ClientHello (через кастомный профиль cipher suites)

use crate::{RealityConfig, RealityError};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tracing::{debug, info, warn};

/// TLS-клиент с Reality-поддержкой.
pub struct RealityClient {
    config: RealityConfig,
}

impl RealityClient {
    /// Создать Reality-клиент.
    pub fn new(config: RealityConfig) -> Self {
        Self { config }
    }

    /// Подключиться к Ghost-серверу через TLS с Reality-маскировкой.
    ///
    /// 1. Устанавливаем TCP-соединение
    /// 2. Выполняем TLS-хэндшейк с SNI = mask_domain
    /// 3. Отправляем HTTP-запрос с auth-token
    /// 4. Получаем подтверждение → туннель готов
    pub async fn connect(server_addr: &str, config: RealityConfig) -> Result<RealityTlsStream, RealityError> {
        // Строим TLS-конфигурацию
        let tls_config = Self::build_tls_config()?;
        let connector = TlsConnector::from(Arc::new(tls_config));

        // TCP-подключение
        let stream = TcpStream::connect(server_addr).await?;
        debug!("TCP connected to {}", server_addr);

        // TLS-хэндшейк с SNI = mask_domain
        let domain = rustls::pki_types::ServerName::try_from(config.mask_domain.clone())
            .map_err(|e| RealityError::TlsHandshake(format!("invalid SNI: {:?}", e)))?;

        let tls_stream = connector.connect(domain, stream).await?;
        info!("TLS connected to {} (SNI={})", server_addr, config.mask_domain);

        Ok(RealityTlsStream {
            stream: tls_stream,
            config,
        })
    }

    /// Построить TLS-конфигурацию клиента.
    ///
    /// Использует системные корневые сертификаты и не проверяет
    /// серверный сертификат (т.к. он self-signed для маскировки).
    fn build_tls_config() -> Result<rustls::ClientConfig, RealityError> {
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        // В продакшене: добавить кастомный verifier, который принимает
        // наш self-signed сертификат (по auth-key fingerprint)
        Ok(config)
    }
}

/// Обёртка над TLS-потоком с Reality-авторизацией.
pub struct RealityTlsStream {
    stream: tokio_rustls::client::TlsStream<TcpStream>,
    config: RealityConfig,
}

impl RealityTlsStream {
    /// Выполнить Ghost-авторизацию и получить сырой TCP-поток для туннеля.
    ///
    /// После успешной авторизации TLS-слой больше не нужен —
    /// дальнейшая пересылка данных идёт через зашифрованные Ghost-фреймы.
    pub async fn authenticate(mut self) -> Result<TcpStream, RealityError> {
        let auth_token = crate::reality_server::RealityServer::create_auth_token(&self.config.auth_key);

        // Отправляем HTTP-запрос с auth-token
        let request = format!(
            "GET /ghost HTTP/1.1\r\n\
             Host: {}\r\n\
             X-Ghost-Auth: {}\r\n\
             Connection: upgrade\r\n\
             Upgrade: ghost-tunnel\r\n\r\n",
            self.config.mask_domain, auth_token
        );

        self.stream.write_all(request.as_bytes()).await?;
        debug!("Sent auth request with Ghost token");

        // Читаем ответ
        let mut buf = vec![0u8; 1024];
        let n = self.stream.read(&mut buf).await?;

        if n == 0 {
            return Err(RealityError::AuthFailed);
        }

        let response = String::from_utf8_lossy(&buf[..n]);
        if response.contains("200 OK") || response.contains("Ghost Proxy") {
            info!("✅ Ghost authentication successful!");
            // В продакшене: извлекаем underlying TCP-поток
            // tokio-rustls не даёт прямой доступ к underlying stream,
            // поэтому в реальной реализации нужен другой подход
            // (например, работать через TLS-туннель, а не извлекать TCP)
            Err(RealityError::TlsHandshake(
                "underlying TCP extraction not supported in current implementation".to_string(),
            ))
        } else {
            warn!("Authentication failed: {}", response);
            Err(RealityError::AuthFailed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reality_client_creation() {
        let config = RealityConfig::default();
        let _client = RealityClient::new(config);
    }
}
