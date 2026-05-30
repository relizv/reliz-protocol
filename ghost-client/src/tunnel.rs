//! Туннель клиента: пересылка данных между SOCKS5 и Ghost-сервером.
//!
//! Этап 2: Транспорт и Крипта
//! - Шаг 4: Кастомный фрейминг (GhostFrame)
//! - Шаг 5: ChaCha20-Poly1305 шифрование
//! - Шаг 6: copy_bidirectional через сервер
//! - Шаг 9: Reality-маскировка (TLS + SNI spoofing + auth)
//! - Шаг 10: TCP-фрагментация (дробление начальных пакетов)

use anyhow::{Context, Result};
use bytes::Bytes;
use ghost_common::{ClientConfig, GhostFrame, TargetAddr, USER_ID_LEN};
use ghost_crypto::GhostCipher;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, error, info};

use crate::fragment::FragmentedStream;

/// Парсит hex-строку UUID в 16-байтовый массив.
fn parse_user_id(hex: &str) -> Result<[u8; USER_ID_LEN]> {
    let hex = hex.replace('-', "");
    if hex.len() != 32 {
        anyhow::bail!("User ID must be 32 hex characters, got {}", hex.len());
    }
    let mut id = [0u8; USER_ID_LEN];
    for i in 0..USER_ID_LEN {
        id[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .context(format!("Invalid hex in user ID at position {}", i))?;
    }
    Ok(id)
}

/// Проксирование данных через Ghost-сервер.
///
/// 1. Подключаемся к Ghost-серверу (TCP → опционально TLS/Reality)
/// 2. Шифруем целевой адрес и данные в GhostFrame
/// 3. Пересылаем данные в обе стороны
pub async fn proxy_through_ghost(
    mut socks5_stream: TcpStream,
    target: TargetAddr,
    config: Arc<ClientConfig>,
) -> Result<()> {
    // Парсим User ID
    let user_id = parse_user_id(&config.user_id)?;

    // ── Подключаемся к Ghost-серверу ──────────────────────────────────

    let stream = TcpStream::connect(&config.server_addr).await
        .with_context(|| format!("Failed to connect to Ghost server at {}", config.server_addr))?;
    debug!("TCP connected to {}", config.server_addr);

    // Опционально: TCP-фрагментация (перед TLS, чтобы дробить ClientHello)
    let frag_stream = if config.enable_fragmentation {
        FragmentedStream::new(stream, 128, 2, 5) // первые 128 байт по 2 байта с задержкой 5мс
    } else {
        FragmentedStream::new(stream, 0, 1, 0)
    };

    // Опционально: Reality TLS-маскировка
    let (mut gh_rd, mut gh_wr): (Box<dyn tokio::io::AsyncRead + Send + Unpin>, Box<dyn tokio::io::AsyncWrite + Send + Unpin>) =
        if config.mask_domain.is_empty() || config.mask_domain == "none" {
            // Обычный режим (без TLS)
            let (r, w) = tokio::io::split(frag_stream);
            (Box::new(r), Box::new(w))
        } else {
            // Reality-режим: TLS с SNI = mask_domain
            let tls_stream = connect_reality(frag_stream, &config.mask_domain, &config.server_addr, &config.reality_auth_key).await?;
            let (r, w) = tokio::io::split(tls_stream);
            (Box::new(r), Box::new(w))
        };

    // Деривация ключа из user_id и встроенного секрета
    let secret = b"ghost_default_key!";
    let key = ghost_crypto::derive_key(&user_id, secret);
    let cipher = GhostCipher::new(&key)?;

    // ── Фаза 1: Отправляем User ID открытым текстом + зашифрованный init frame ─

    // User ID (16 байт) — сервер ищет его в allowed_users до расшифровки
    gh_wr.write_all(&user_id).await?;
    debug!("Sent User ID in cleartext: {}", config.user_id);

    let init_frame = GhostFrame::new(user_id, target.clone(), Bytes::new());
    let init_frame = if config.enable_padding {
        init_frame.with_random_padding(config.max_padding_len as usize)
    } else {
        init_frame
    };

    let init_data = init_frame.encode().freeze();
    let encrypted = cipher.encrypt(&init_data)?;

    let len = encrypted.len() as u16;
    gh_wr.write_all(&len.to_be_bytes()).await?;
    gh_wr.write_all(&encrypted).await?;

    debug!("Sent init frame to Ghost server, target={}", target);

    // ── Фаза 2: Двунаправленная пересылка данных ──────────────────────

    let cipher_c2s = cipher;
    let cipher_s2c = GhostCipher::new(&key)?;

    let (mut s5_rd, mut s5_wr) = socks5_stream.split();

    // Клиент → Сервер (upload)
    let upload = async {
        let mut buf = vec![0u8; 8192];
        loop {
            let n = match s5_rd.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    error!("SOCKS5 read error: {}", e);
                    break;
                }
            };

            let frame = GhostFrame::new(
                user_id,
                TargetAddr::None,
                Bytes::copy_from_slice(&buf[..n]),
            );
            let frame = if config.enable_padding {
                frame.with_random_padding(config.max_padding_len as usize)
            } else {
                frame
            };

            let frame_data = frame.encode().freeze();
            match cipher_c2s.encrypt(&frame_data) {
                Ok(encrypted) => {
                    let len = encrypted.len() as u16;
                    if let Err(e) = gh_wr.write_all(&len.to_be_bytes()).await {
                        error!("Ghost write error: {}", e);
                        break;
                    }
                    if let Err(e) = gh_wr.write_all(&encrypted).await {
                        error!("Ghost write error: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Encryption error: {}", e);
                    break;
                }
            }
        }
    };

    // Сервер → Клиент (download)
    let download = async {
        let mut len_buf = [0u8; 2];
        let mut enc_buf = vec![0u8; 65536 + 256];

        loop {
            if let Err(e) = gh_rd.read_exact(&mut len_buf).await {
                if e.kind() != std::io::ErrorKind::UnexpectedEof {
                    error!("Ghost read length error: {}", e);
                }
                break;
            }
            let frame_len = u16::from_be_bytes(len_buf) as usize;

            if frame_len > enc_buf.len() {
                error!("Frame too large: {}", frame_len);
                break;
            }

            if let Err(e) = gh_rd.read_exact(&mut enc_buf[..frame_len]).await {
                error!("Ghost read frame error: {}", e);
                break;
            }

            match cipher_s2c.decrypt(&enc_buf[..frame_len]) {
                Ok(plaintext) => {
                    let frame_data = bytes::BytesMut::from(&plaintext[..]);
                    match GhostFrame::decode(frame_data) {
                        Ok(frame) => {
                            if !frame.payload.is_empty() {
                                if let Err(e) = s5_wr.write_all(&frame.payload).await {
                                    error!("SOCKS5 write error: {}", e);
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            error!("Frame decode error: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("Decryption error: {}", e);
                    break;
                }
            }
        }
    };

    tokio::select! {
        _ = upload => debug!("Upload stream finished"),
        _ = download => debug!("Download stream finished"),
    }

    Ok(())
}

/// Подключиться к серверу через TLS с Reality-маскировкой.
///
/// 1. TLS handshake с SNI = mask_domain
/// 2. Отправка auth-запроса (HTTP Upgrade)
/// 3. Проверка ответа
/// 4. Возврат TlsStream для дальнейшей передачи Ghost-фреймов
async fn connect_reality(
    stream: FragmentedStream,
    mask_domain: &str,
    server_addr: &str,
    reality_auth_key: &str,
) -> Result<tokio_rustls::client::TlsStream<FragmentedStream>> {
    // Строим TLS-конфигурацию: проверяем fingerprint сертификата по auth_key
    let tls_config = build_reality_tls_config(reality_auth_key)?;
    let connector = tokio_rustls::TlsConnector::from(Arc::new(tls_config));

    // SNI = mask_domain (например, www.apple.com)
    let domain = rustls::pki_types::ServerName::try_from(mask_domain.to_string())
        .map_err(|e| anyhow::anyhow!("Invalid SNI domain: {:?}", e))?;

    let mut tls_stream = connector.connect(domain, stream).await
        .with_context(|| format!("TLS handshake failed to {} (SNI={})", server_addr, mask_domain))?;

    info!("TLS connected to {} (SNI={})", server_addr, mask_domain);

    // Auth: отправляем HTTP-запрос с заголовком X-Ghost-Auth
    // В текущей реализации токен = hex(ghost_default_key) — упрощённо
    let auth_token = hex_encode(b"ghost_default_key!");
    let request = format!(
        "GET /ghost HTTP/1.1\r\n\
         Host: {}\r\n\
         X-Ghost-Auth: {}\r\n\
         Connection: upgrade\r\n\
         Upgrade: ghost-tunnel\r\n\r\n",
        mask_domain, auth_token
    );

    tls_stream.write_all(request.as_bytes()).await?;
    tls_stream.flush().await?;
    debug!("Sent Reality auth request");

    // Читаем ответ
    let mut buf = vec![0u8; 1024];
    let n = tls_stream.read(&mut buf).await?;
    if n == 0 {
        anyhow::bail!("Reality auth failed: server closed connection");
    }

    let response = String::from_utf8_lossy(&buf[..n]);
    if !response.contains("200 OK") && !response.contains("Ghost Proxy") && !response.contains("101 Switching Protocols") {
        anyhow::bail!("Reality auth failed: unexpected response: {}", response.lines().next().unwrap_or("empty"));
    }

    info!("Reality authentication successful");
    Ok(tls_stream)
}

/// TLS-конфигурация для Reality: проверяем fingerprint сертификата по auth_key.
fn build_reality_tls_config(reality_auth_key_hex: &str) -> Result<rustls::ClientConfig> {
    let expected_fp = parse_hex_32(reality_auth_key_hex)
        .map_err(|e| anyhow::anyhow!("Invalid reality_auth_key: {}", e))?;

    #[derive(Debug)]
    struct FingerprintVerifier {
        expected: [u8; 32],
    }

    impl rustls::client::danger::ServerCertVerifier for FingerprintVerifier {
        fn verify_server_cert(
            &self,
            end_entity: &rustls::pki_types::CertificateDer<'_>,
            _intermediates: &[rustls::pki_types::CertificateDer<'_>],
            _server_name: &rustls::pki_types::ServerName<'_>,
            _ocsp_response: &[u8],
            _now: rustls::pki_types::UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            use sha2::{Sha256, Digest};
            let mut hasher = Sha256::new();
            hasher.update(end_entity.as_ref());
            let fp = hasher.finalize();
            if fp.as_slice() == self.expected.as_slice() {
                Ok(rustls::client::danger::ServerCertVerified::assertion())
            } else {
                Err(rustls::Error::General(
                    format!("Certificate fingerprint mismatch: expected {}, got {}",
                        hex_encode(&self.expected), hex_encode(fp.as_slice()))
                ))
            }
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &rustls::pki_types::CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &rustls::pki_types::CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            vec![
                rustls::SignatureScheme::RSA_PKCS1_SHA256,
                rustls::SignatureScheme::RSA_PKCS1_SHA384,
                rustls::SignatureScheme::RSA_PKCS1_SHA512,
                rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                rustls::SignatureScheme::ED25519,
            ]
        }
    }

    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(FingerprintVerifier { expected: expected_fp }))
        .with_no_client_auth();

    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}

fn parse_hex_32(hex: &str) -> Result<[u8; 32]> {
    let hex = hex.replace('-', "");
    if hex.len() != 64 {
        anyhow::bail!("Expected 64 hex chars, got {}", hex.len());
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)?;
    }
    Ok(out)
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}
