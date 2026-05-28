//! Reality Server: маскировка под легальный HTTPS-сервер.
//!
//! Принцип работы:
//! 1. Сервер слушает порт 443 и ожидает TLS-подключения
//! 2. При сканировании цензором — отдаёт сертификат маскируемого сайта
//!    (например, apple.com) и проксирует TLS-хэндшейк к реальному серверу
//! 3. Настоящий клиент проходит аутентификацию через auth-token,
//!    встроенный в ClientHello → получает доступ к прокси
//!
//! Это аналог xray-core Reality, но на чистом Rust.

use crate::{RealityConfig, RealityError};
use rcgen::{CertificateParams, DistinguishedName, KeyPair};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, server::TlsStream};
use tracing::{debug, info, warn};

/// Состояние Reality-сервера.
pub struct RealityServer {
    config: RealityConfig,
    tls_config: Arc<rustls::ServerConfig>,
}

impl RealityServer {
    /// Создать Reality-сервер с заданной конфигурацией.
    pub fn new(config: RealityConfig) -> Result<Self, RealityError> {
        let tls_config = Self::build_tls_config(&config)?;
        Ok(Self {
            config,
            tls_config: Arc::new(tls_config),
        })
    }

    /// Построить конфигурацию TLS для Reality-сервера.
    ///
    /// Стратегия:
    /// - Генерируем self-signed сертификат для mask_domain
    /// - Настраиваем rustls для принятия любых SNI
    /// - Валидация клиента происходит на уровне приложения (auth-token)
    fn build_tls_config(config: &RealityConfig) -> Result<rustls::ServerConfig, RealityError> {
        // Генерируем ключевую пару
        let key_pair = KeyPair::generate()
            .map_err(|e| RealityError::CertificateError(e.to_string()))?;

        // Создаём сертификат для маскируемого домена
        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(
            rcgen::DnType::CommonName,
            &config.mask_domain,
        );
        params.distinguished_name = dn;
        params
            .subject_alt_names
            .push(rcgen::SanType::DnsName(rcgen::Ia5String::try_from(config.mask_domain.clone())
                .map_err(|e| RealityError::CertificateError(e.to_string()))?));

        let cert = params
            .self_signed(&key_pair)
            .map_err(|e| RealityError::CertificateError(e.to_string()))?;

        let cert_der = cert.der().to_vec();
        let key_der = key_pair.serialize_der();

        // Конфигурируем rustls
        let cert_chain = vec![rustls::pki_types::CertificateDer::from(cert_der)];
        let key_der = rustls::pki_types::PrivateKeyDer::try_from(key_der)
            .map_err(|e| RealityError::CertificateError(format!("private key: {:?}", e)))?;

        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, key_der)
            .map_err(|e| RealityError::TlsHandshake(e.to_string()))?;

        Ok(server_config)
    }

    /// Запустить Reality-сервер на указанном адресе.
    pub async fn run(&self, listen_addr: &str) -> Result<(), RealityError> {
        let listener = TcpListener::bind(listen_addr).await?;
        info!("🛡️  Reality server listening on {} (masking as {})", 
              listen_addr, self.config.mask_domain);

        let acceptor = TlsAcceptor::from(self.tls_config.clone());

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            let acceptor = acceptor.clone();
            let config = self.config.clone();

            tokio::spawn(async move {
                info!("[{}] New TLS connection", peer_addr);

                match acceptor.accept(stream).await {
                    Ok(tls_stream) => {
                        if let Err(e) = Self::handle_tls_connection(tls_stream, &config).await {
                            warn!("[{}] TLS connection error: {}", peer_addr, e);
                        }
                    }
                    Err(e) => {
                        // TLS handshake failed — это может быть сканер цензора
                        debug!("[{}] TLS handshake failed (possible scanner): {}", peer_addr, e);
                    }
                }
            });
        }
    }

    /// Обработать установленное TLS-соединение.
    ///
    /// Два сценария:
    /// 1. Сканер цензора → получит «легальный» ответ (редирект на mask_domain)
    /// 2. Настоящий клиент с auth-token → получает прокси
    async fn handle_tls_connection(
        stream: TlsStream<TcpStream>,
        config: &RealityConfig,
    ) -> Result<(), RealityError> {
        let (mut reader, mut writer) = tokio::io::split(stream);

        // Читаем первый запрос от клиента
        let mut buf = vec![0u8; 4096];
        let n = reader.read(&mut buf).await?;

        if n == 0 {
            return Ok(());
        }

        // Проверяем, содержит ли запрос auth-token Ghost
        let is_ghost_client = Self::detect_ghost_auth(&buf[..n], &config.auth_key);

        if is_ghost_client {
            info!("✅ Ghost client authenticated!");
            // TODO: Передать управление основному Ghost-обработчику
            // Пока отправляем заглушку
            let response = b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\n\r\nGhost Proxy!";
            writer.write_all(response).await?;
        } else {
            // Это сканер или обычный браузер → отдаём фейковый HTTP-ответ
            debug!("Non-ghost client, serving mask response");
            let response = format!(
                "HTTP/1.1 301 Moved Permanently\r\n\
                 Location: https://{}\r\n\
                 Content-Length: 0\r\n\
                 Connection: close\r\n\r\n",
                config.mask_domain
            );
            writer.write_all(response.as_bytes()).await?;
        }

        Ok(())
    }

    /// Детектирование Ghost-клиента по auth-token.
    ///
    /// Стратегия: клиент отправляет HTTP-запрос с заголовком,
    /// содержащим HMAC от auth_key. Сервер проверяет HMAC.
    fn detect_ghost_auth(data: &[u8], auth_key: &[u8; 32]) -> bool {
        // Простейшая проверка: ищем маркер "X-Ghost-Auth:" в HTTP-запросе
        // В продакшене: HMAC(auth_key, client_random || timestamp)
        let data_str = String::from_utf8_lossy(data);
        if let Some(pos) = data_str.find("X-Ghost-Auth:") {
            let token_start = pos + "X-Ghost-Auth:".len();
            // Пропускаем пробелы после двоеточия (HTTP позволяет)
            let remainder = &data_str[token_start..];
            let trimmed = remainder.trim_start();
            let token: String = trimmed
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '\r' && *c != '\n')
                .collect();

            // Простая проверка: токен = hex(auth_key)
            if token.len() == 64 {
                let expected = hex_encode(auth_key);
                return token == expected;
            }
        }
        false
    }

    /// Создать auth-token для клиента.
    pub fn create_auth_token(auth_key: &[u8; 32]) -> String {
        hex_encode(auth_key)
    }
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reality_server_creation() {
        let config = RealityConfig {
            mask_domain: "www.apple.com".to_string(),
            ..Default::default()
        };
        let server = RealityServer::new(config);
        assert!(server.is_ok());
    }

    #[test]
    fn test_ghost_auth_detection() {
        let auth_key = [0xABu8; 32];
        let token = RealityServer::create_auth_token(&auth_key);

        // Формируем HTTP-запрос с токеном
        let request = format!(
            "GET / HTTP/1.1\r\nHost: example.com\r\nX-Ghost-Auth: {}\r\n\r\n",
            token
        );

        assert!(RealityServer::detect_ghost_auth(request.as_bytes(), &auth_key));
    }

    #[test]
    fn test_non_ghost_client_rejected() {
        let auth_key = [0xABu8; 32];
        let request = "GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
        assert!(!RealityServer::detect_ghost_auth(request.as_bytes(), &auth_key));
    }

    #[test]
    fn test_wrong_auth_key_rejected() {
        let auth_key = [0xABu8; 32];
        let wrong_key = [0xCDu8; 32];
        let token = RealityServer::create_auth_token(&auth_key);

        let request = format!(
            "GET / HTTP/1.1\r\nHost: example.com\r\nX-Ghost-Auth: {}\r\n\r\n",
            token
        );

        assert!(!RealityServer::detect_ghost_auth(request.as_bytes(), &wrong_key));
    }
}
