//! Обработка входящих подключений на Ghost-сервере.
//!
//! 1. Читаем зашифрованный фрейм (инициализация с целевым адресом)
//! 2. Расшифровываем, проверяем User ID
//! 3. Подключаемся к целевому хосту
//! 4. copy_bidirectional между клиентом и целевым хостом

use anyhow::{bail, Context, Result};
use bytes::BytesMut;
use ghost_common::{GhostFrame, TargetAddr, USER_ID_LEN};
use ghost_crypto::GhostCipher;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, error, info};

use crate::ServerState;

/// Обработка одного клиентского подключения (TCP или TLS).
pub async fn handle_connection<S>(
    mut stream: S,
    state: Arc<ServerState>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Send + Unpin,
{
    let mut len_buf = [0u8; 2];
    let mut enc_buf = vec![0u8; 65536 + 256];

    // ── Читаем User ID (16 байт открытым текстом) ─────────────────────

    let mut user_id = [0u8; USER_ID_LEN];
    stream.read_exact(&mut user_id).await?;
    let user_id_hex = hex_encode(&user_id);

    // Проверяем авторизацию до расшифровки
    if !state.allowed_users.contains(&user_id_hex) {
        bail!("Unauthorized user: {}", user_id_hex);
    }

    let secret = b"ghost_default_key!";
    let key = ghost_crypto::derive_key(&user_id, secret);
    let cipher = GhostCipher::new(&key)?;

    // ── Читаем зашифрованный init frame ───────────────────────────────

    stream.read_exact(&mut len_buf).await?;
    let frame_len = u16::from_be_bytes(len_buf) as usize;

    if frame_len > enc_buf.len() {
        bail!("Init frame too large: {}", frame_len);
    }

    stream.read_exact(&mut enc_buf[..frame_len]).await?;
    let encrypted_data = &enc_buf[..frame_len];

    let plaintext = cipher.decrypt(encrypted_data)?;
    let frame_data = BytesMut::from(&plaintext[..]);
    let frame = GhostFrame::decode(frame_data)?;

    debug!("Init frame from user {}: target={}", user_id_hex, frame.target);

    // ── Подключаемся к целевому хосту ─────────────────────────────────

    let target_addr = resolve_target(&frame.target).await?;
    let mut remote_stream = TcpStream::connect(target_addr).await
        .with_context(|| format!("Failed to connect to target: {}", frame.target))?;

    info!("Connected to target: {} → {}", frame.target, target_addr);

    // Если в первом фрейме уже есть payload — отправляем его
    if !frame.payload.is_empty() {
        remote_stream.write_all(&frame.payload).await?;
        debug!("Forwarded {} bytes from init frame", frame.payload.len());
    }

    // ── copy_bidirectional ────────────────────────────────────────────

    let (mut client_rd, mut client_wr) = tokio::io::split(stream);
    let (mut remote_rd, mut remote_wr) = remote_stream.split();

    let cipher_c2s = GhostCipher::new(&key)?;
    let cipher_s2c = GhostCipher::new(&key)?;

    // Клиент → Цель (расшифровка и пересылка)
    let upload = async {
        let mut len_buf = [0u8; 2];
        let mut enc_buf = vec![0u8; 65536 + 256];

        loop {
            match client_rd.read_exact(&mut len_buf).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => {
                    error!("Client read length error: {}", e);
                    break;
                }
            }
            let frame_len = u16::from_be_bytes(len_buf) as usize;
            if frame_len > enc_buf.len() {
                error!("Client frame too large: {}", frame_len);
                break;
            }

            if let Err(e) = client_rd.read_exact(&mut enc_buf[..frame_len]).await {
                error!("Client read frame error: {}", e);
                break;
            }

            match cipher_c2s.decrypt(&enc_buf[..frame_len]) {
                Ok(plaintext) => {
                    let frame_data = BytesMut::from(&plaintext[..]);
                    match GhostFrame::decode(frame_data) {
                        Ok(frame) => {
                            if !frame.payload.is_empty() {
                                if let Err(e) = remote_wr.write_all(&frame.payload).await {
                                    error!("Target write error: {}", e);
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            error!("Client frame decode error: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("Client frame decrypt error: {}", e);
                    break;
                }
            }
        }
    };

    // Цель → Клиент (шифровка и пересылка)
    let download = async {
        let mut buf = vec![0u8; 8192];

        loop {
            let n = match remote_rd.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    error!("Target read error: {}", e);
                    break;
                }
            };

            let frame = GhostFrame::new(
                user_id,
                TargetAddr::None,
                bytes::Bytes::copy_from_slice(&buf[..n]),
            );
            let frame = if state.config.enable_padding {
                frame.with_random_padding(state.config.max_padding_len as usize)
            } else {
                frame
            };

            let frame_data = frame.encode().freeze();
            match cipher_s2c.encrypt(&frame_data) {
                Ok(encrypted) => {
                    let len = encrypted.len() as u16;
                    if let Err(e) = client_wr.write_all(&len.to_be_bytes()).await {
                        error!("Client write length error: {}", e);
                        break;
                    }
                    if let Err(e) = client_wr.write_all(&encrypted).await {
                        error!("Client write frame error: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Server encrypt error: {}", e);
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

/// Резолвинг целевого адреса в SocketAddr.
async fn resolve_target(target: &TargetAddr) -> Result<std::net::SocketAddr> {
    match target {
        TargetAddr::None => bail!("No target address in init frame"),
        TargetAddr::IpV4(a) => Ok(std::net::SocketAddr::V4(*a)),
        TargetAddr::IpV6(a) => Ok(std::net::SocketAddr::V6(*a)),
        TargetAddr::Domain(domain, port) => {
            use tokio::net::lookup_host;
            let addr = format!("{}:{}", domain, port);
            let mut addrs = lookup_host(&addr).await
                .with_context(|| format!("DNS resolution failed for {}", addr))?;
            addrs.next()
                .context(format!("No addresses found for {}", addr))
        }
    }
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}
