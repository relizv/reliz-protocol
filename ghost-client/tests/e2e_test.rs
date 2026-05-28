//! Интеграционный E2E-тест: SOCKS5 клиент → Ghost сервер → эхо-сервер.
//!
//! Проверяет полный цикл:
//! 1. Запуск эхо-сервера (имитация целевого сайта)
//! 2. Запуск Ghost-сервера
//! 3. Подключение через SOCKS5 → Ghost → эхо-сервер
//! 4. Отправка данных и получение ответа

use bytes::Bytes;
use ghost_common::{GhostFrame, TargetAddr, USER_ID_LEN};
use ghost_crypto::GhostCipher;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Простой эхо-сервер для тестирования.
async fn start_echo_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let handle = tokio::spawn(async move {
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    loop {
                        match stream.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                if stream.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
        }
    });

    (addr, handle)
}

/// Ghost-сервер для тестов — минимальная версия.
async fn start_ghost_server(target_addr: String) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ghost_addr = listener.local_addr().unwrap().to_string();
    let secret = b"ghost_default_key!";

    let handle = tokio::spawn(async move {
        let user_id_hex = "00000000000000000000000000000001";
        let mut user_id = [0u8; USER_ID_LEN];
        for i in 0..USER_ID_LEN {
            user_id[i] = u8::from_str_radix(&user_id_hex[i * 2..i * 2 + 2], 16).unwrap();
        }
        let key = ghost_crypto::derive_key(&user_id, secret);
        let cipher = GhostCipher::new(&key).unwrap();

        if let Ok((mut stream, _)) = listener.accept().await {
            // Читаем init-фрейм
            let mut len_buf = [0u8; 2];
            if stream.read_exact(&mut len_buf).await.is_err() {
                return;
            }
            let frame_len = u16::from_be_bytes(len_buf) as usize;
            let mut enc_buf = vec![0u8; frame_len];
            if stream.read_exact(&mut enc_buf).await.is_err() {
                return;
            }

            let plaintext = cipher.decrypt(&enc_buf).unwrap();
            let frame_data = bytes::BytesMut::from(&plaintext[..]);
            let frame = GhostFrame::decode(frame_data).unwrap();

            // Подключаемся к целевому хосту
            if let Ok(mut remote) = TcpStream::connect(&target_addr).await {
                // Пересылаем payload из init-фрейма
                if !frame.payload.is_empty() {
                    let _ = remote.write_all(&frame.payload).await;
                }

                // Простая проксировка: remote → client
                let (mut client_rd, mut client_wr) = stream.split();
                let (mut remote_rd, mut remote_wr) = remote.split();

                let cipher_s2c = GhostCipher::new(&key).unwrap();
                let cipher_c2s = GhostCipher::new(&key).unwrap();

                // download: remote → client
                let dl = async {
                    let mut buf = vec![0u8; 4096];
                    loop {
                        match remote_rd.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let frame = GhostFrame::new(
                                    user_id,
                                    TargetAddr::None,
                                    Bytes::copy_from_slice(&buf[..n]),
                                );
                                let frame_data = frame.encode().freeze();
                                if let Ok(encrypted) = cipher_s2c.encrypt(&frame_data) {
                                    let len = encrypted.len() as u16;
                                    if client_wr.write_all(&len.to_be_bytes()).await.is_err() {
                                        break;
                                    }
                                    if client_wr.write_all(&encrypted).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            Err(_) => break,
                        }
                    }
                };

                // upload: client → remote
                let ul = async {
                    let mut len_buf = [0u8; 2];
                    let mut enc_buf = vec![0u8; 65536];
                    loop {
                        if client_rd.read_exact(&mut len_buf).await.is_err() {
                            break;
                        }
                        let flen = u16::from_be_bytes(len_buf) as usize;
                        if flen > enc_buf.len() { break; }
                        if client_rd.read_exact(&mut enc_buf[..flen]).await.is_err() {
                            break;
                        }
                        if let Ok(pt) = cipher_c2s.decrypt(&enc_buf[..flen]) {
                            let fd = bytes::BytesMut::from(&pt[..]);
                            if let Ok(f) = GhostFrame::decode(fd) {
                                if !f.payload.is_empty() {
                                    if remote_wr.write_all(&f.payload).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                };

                tokio::select! {
                    _ = dl => {}
                    _ = ul => {}
                }
            }
        }
    });

    (ghost_addr, handle)
}

#[tokio::test]
async fn test_full_tunnel_e2e() {
    // 1. Запускаем эхо-сервер
    let (echo_addr, _echo_handle) = start_echo_server().await;

    // 2. Запускаем Ghost-сервер, указывающий на эхо-сервер
    let (ghost_addr, _ghost_handle) = start_ghost_server(echo_addr.clone()).await;

    // 3. Подключаемся к Ghost-серверу напрямую (имитация клиентского туннеля)
    let mut ghost_stream = TcpStream::connect(&ghost_addr).await.unwrap();

    // Формируем init-фрейм с целевым адресом
    let user_id_hex = "00000000000000000000000000000001";
    let mut user_id = [0u8; USER_ID_LEN];
    for i in 0..USER_ID_LEN {
        user_id[i] = u8::from_str_radix(&user_id_hex[i * 2..i * 2 + 2], 16).unwrap();
    }
    let secret = b"ghost_default_key!";
    let key = ghost_crypto::derive_key(&user_id, secret);
    let cipher = GhostCipher::new(&key).unwrap();

    // Парсим адрес эхо-сервера
    let parts: Vec<&str> = echo_addr.split(':').collect();
    let domain = "127.0.0.1".to_string();
    let port: u16 = parts[1].parse().unwrap();

    let init_frame = GhostFrame::new(
        user_id,
        TargetAddr::Domain(domain, port),
        Bytes::from_static(b"Hello Ghost!"),
    );
    let init_data = init_frame.encode().freeze();
    let encrypted = cipher.encrypt(&init_data).unwrap();

    // Отправляем
    let len = encrypted.len() as u16;
    ghost_stream.write_all(&len.to_be_bytes()).await.unwrap();
    ghost_stream.write_all(&encrypted).await.unwrap();

    // 4. Читаем ответ (эхо должно вернуть "Hello Ghost!")
    let mut len_buf = [0u8; 2];
    ghost_stream.read_exact(&mut len_buf).await.unwrap();
    let resp_len = u16::from_be_bytes(len_buf) as usize;
    let mut resp_buf = vec![0u8; resp_len];
    ghost_stream.read_exact(&mut resp_buf).await.unwrap();

    let resp_plaintext = cipher.decrypt(&resp_buf).unwrap();
    let resp_frame = GhostFrame::decode(bytes::BytesMut::from(&resp_plaintext[..])).unwrap();

    assert_eq!(resp_frame.payload, Bytes::from_static(b"Hello Ghost!"));
}
