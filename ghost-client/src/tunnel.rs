//! Туннель клиента: пересылка данных между SOCKS5 и Ghost-сервером.
//!
//! Этап 2: Транспорт и Крипта
//! - Шаг 4: Кастомный фрейминг (GhostFrame)
//! - Шаг 5: ChaCha20-Poly1305 шифрование
//! - Шаг 6: copy_bidirectional через сервер

use anyhow::{Context, Result};
use bytes::Bytes;
use ghost_common::{ClientConfig, GhostFrame, TargetAddr, USER_ID_LEN};
use ghost_crypto::GhostCipher;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, error};

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
/// 1. Подключаемся к Ghost-серверу
/// 2. Шифруем целевой адрес и данные в GhostFrame
/// 3. Пересылаем данные в обе стороны
pub async fn proxy_through_ghost(
    mut socks5_stream: TcpStream,
    target: TargetAddr,
    config: Arc<ClientConfig>,
) -> Result<()> {
    // Парсим User ID
    let user_id = parse_user_id(&config.user_id)?;

    // Подключаемся к Ghost-серверу
    let mut ghost_stream = TcpStream::connect(&config.server_addr).await?;
    debug!("Connected to Ghost server at {}", config.server_addr);

    // Деривация ключа из user_id и встроенного секрета
    // TODO: В продакшене — чтение pre-shared key из конфига
    let secret = b"ghost_default_key!";
    let key = ghost_crypto::derive_key(&user_id, secret);
    let cipher = GhostCipher::new(&key)?;

    // ── Фаза 1: Отправляем целевой адрес серверу ──────────────────────

    // Создаём начальный фрейм: payload = пустой (только адрес)
    let init_frame = GhostFrame::new(user_id, target.clone(), Bytes::new());
    let init_frame = if config.enable_padding {
        init_frame.with_random_padding(config.max_padding_len as usize)
    } else {
        init_frame
    };

    let init_data = init_frame.encode().freeze();
    let encrypted = cipher.encrypt(&init_data)?;

    // Отправляем: [Len:2][EncryptedFrame]
    let len = encrypted.len() as u16;
    ghost_stream.write_all(&len.to_be_bytes()).await?;
    ghost_stream.write_all(&encrypted).await?;

    debug!("Sent init frame to Ghost server, target={}", target);

    // ── Фаза 2: Двунаправленная пересылка данных ──────────────────────

    // Клиент → Сервер: читаем из SOCKS5, шифруем, отправляем
    let cipher_c2s = cipher;
    let cipher_s2c = GhostCipher::new(&key)?;

    let (mut s5_rd, mut s5_wr) = socks5_stream.split();
    let (mut gh_rd, mut gh_wr) = ghost_stream.split();

    // Задача: SOCKS5 → Ghost (upload)
    let upload = async {
        let mut buf = vec![0u8; 8192];
        loop {
            let n = match s5_rd.read(&mut buf).await {
                Ok(0) => break, // EOF
                Ok(n) => n,
                Err(e) => {
                    error!("SOCKS5 read error: {}", e);
                    break;
                }
            };

            // Формируем Ghost-фрейм с payload
            let frame = GhostFrame::new(
                user_id,
                TargetAddr::None, // Адрес уже известен серверу
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

    // Задача: Ghost → SOCKS5 (download)
    let download = async {
        let mut len_buf = [0u8; 2];
        let mut enc_buf = vec![0u8; 65536 + 256]; // Достаточно для любого фрейма

        loop {
            // Читаем длину зашифрованного фрейма
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

            // Читаем зашифрованный фрейм
            if let Err(e) = gh_rd.read_exact(&mut enc_buf[..frame_len]).await {
                error!("Ghost read frame error: {}", e);
                break;
            }

            // Расшифровываем
            match cipher_s2c.decrypt(&enc_buf[..frame_len]) {
                Ok(plaintext) => {
                    // Парсим Ghost-фрейм
                    let frame_data = bytes::BytesMut::from(&plaintext[..]);
                    match GhostFrame::decode(frame_data) {
                        Ok(frame) => {
                            // Пишем payload в SOCKS5
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

    // Запускаем обе задачи одновременно
    tokio::select! {
        _ = upload => debug!("Upload stream finished"),
        _ = download => debug!("Download stream finished"),
    }

    Ok(())
}
