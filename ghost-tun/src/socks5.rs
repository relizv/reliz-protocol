//! Минимальный SOCKS5 клиент для tun2socks.
//!
//! Поддерживает:
//! - `CONNECT` (TCP)
//! - `UDP ASSOCIATE`
//!
//! Аутентификация не поддерживается (No Auth 0x00).

use anyhow::{bail, Context, Result};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Результат SOCKS5-рукопожатия: TCP поток для CONNECT или адрес UDP-релея.
pub enum Socks5Result {
    /// TCP CONNECT успешен — можно гонить данные.
    Connected(TcpStream),
    /// UDP ASSOCIATE успешен — адрес UDP-релея.
    UdpAssociated(SocketAddr, TcpStream),
}

/// Выполнить SOCKS5 CONNECT к `target` через `proxy_addr`.
pub async fn socks5_connect(proxy_addr: SocketAddr, target: SocketAddr) -> Result<TcpStream> {
    let mut stream = TcpStream::connect(proxy_addr)
        .await
        .with_context(|| format!("SOCKS5 connect to proxy {}", proxy_addr))?;

    // 1. Приветствие: VER=5, NMETHODS=1, METHOD=0x00 (No Auth)
    stream.write_all(&[0x05, 0x01, 0x00]).await?;

    // 2. Ответ: [VER, METHOD]
    let mut buf = [0u8; 2];
    stream.read_exact(&mut buf).await?;
    if buf[0] != 0x05 {
        bail!("SOCKS5 version mismatch: {:#x}", buf[0]);
    }
    if buf[1] != 0x00 {
        bail!("SOCKS5 auth method not accepted: {:#x}", buf[1]);
    }

    // 3. Запрос CONNECT
    let mut req = vec![0x05, 0x01, 0x00]; // VER, CMD=CONNECT, RSV
    encode_addr(target, &mut req);
    stream.write_all(&req).await?;

    // 4. Ответ
    let mut resp = [0u8; 4];
    stream.read_exact(&mut resp).await?;
    if resp[0] != 0x05 {
        bail!("SOCKS5 reply version mismatch: {:#x}", resp[0]);
    }
    if resp[1] != 0x00 {
        bail!("SOCKS5 CONNECT failed with code: {:#x}", resp[1]);
    }

    // Пропускаем BND.ADDR/BND.PORT (нам не нужны для CONNECT)
    skip_socks5_addr(&mut stream).await?;

    Ok(stream)
}

/// Выполнить SOCKS5 UDP ASSOCIATE через `proxy_addr`.
/// Возвращает адрес UDP-релея и управляющий TCP-поток.
pub async fn socks5_udp_associate(proxy_addr: SocketAddr) -> Result<(SocketAddr, TcpStream)> {
    let mut stream = TcpStream::connect(proxy_addr).await?;

    // Auth
    stream.write_all(&[0x05, 0x01, 0x00]).await?;
    let mut buf = [0u8; 2];
    stream.read_exact(&mut buf).await?;
    if buf[1] != 0x00 {
        bail!("SOCKS5 auth rejected");
    }

    // UDP ASSOCIATE request (DST.ADDR = 0.0.0.0:0)
    let req = vec![
        0x05, 0x03, 0x00, // VER, CMD=UDP ASSOCIATE, RSV
        0x01, 0x00, 0x00, 0x00, 0x00, // IPv4 0.0.0.0
        0x00, 0x00, // port 0
    ];
    stream.write_all(&req).await?;

    // Response
    let mut resp = [0u8; 4];
    stream.read_exact(&mut resp).await?;
    if resp[1] != 0x00 {
        bail!("SOCKS5 UDP ASSOCIATE failed: {:#x}", resp[1]);
    }

    let relay_addr = read_socks5_addr(&mut stream).await?;
    Ok((relay_addr, stream))
}

/// Закодировать SocketAddr в SOCKS5-формат (для запроса).
fn encode_addr(addr: SocketAddr, buf: &mut Vec<u8>) {
    match addr {
        SocketAddr::V4(v4) => {
            buf.push(0x01); // IPv4
            buf.extend_from_slice(&v4.ip().octets());
            buf.extend_from_slice(&v4.port().to_be_bytes());
        }
        SocketAddr::V6(v6) => {
            buf.push(0x04); // IPv6
            buf.extend_from_slice(&v6.ip().octets());
            buf.extend_from_slice(&v6.port().to_be_bytes());
        }
    }
}

/// Прочитать и проигнорировать SOCKS5-адрес из потока (для CONNECT ответа).
async fn skip_socks5_addr(stream: &mut TcpStream) -> Result<()> {
    let mut atyp = [0u8; 1];
    stream.read_exact(&mut atyp).await?;
    match atyp[0] {
        0x01 => {
            let mut buf = [0u8; 6]; // IP + port
            stream.read_exact(&mut buf).await?;
        }
        0x03 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let mut buf = vec![0u8; len[0] as usize + 2];
            stream.read_exact(&mut buf).await?;
        }
        0x04 => {
            let mut buf = [0u8; 18]; // IPv6 + port
            stream.read_exact(&mut buf).await?;
        }
        _ => bail!("Unknown SOCKS5 ATYP: {}", atyp[0]),
    }
    Ok(())
}

/// Прочитать SOCKS5-адрес из потока и вернуть SocketAddr.
async fn read_socks5_addr(stream: &mut TcpStream) -> Result<SocketAddr> {
    let mut atyp = [0u8; 1];
    stream.read_exact(&mut atyp).await?;
    match atyp[0] {
        0x01 => {
            let mut buf = [0u8; 6];
            stream.read_exact(&mut buf).await?;
            let ip = std::net::Ipv4Addr::new(buf[0], buf[1], buf[2], buf[3]);
            let port = u16::from_be_bytes([buf[4], buf[5]]);
            Ok(SocketAddr::from((ip, port)))
        }
        0x04 => {
            let mut buf = [0u8; 18];
            stream.read_exact(&mut buf).await?;
            let ip = std::net::Ipv6Addr::from([
                buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
                buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
            ]);
            let port = u16::from_be_bytes([buf[16], buf[17]]);
            Ok(SocketAddr::from((ip, port)))
        }
        _ => bail!("Unsupported SOCKS5 ATYP for UDP: {}", atyp[0]),
    }
}
