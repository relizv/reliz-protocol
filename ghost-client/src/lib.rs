//! ghost-client: Клиентское ядро протокола Ghost.
//!
//! Этап 1: Локальный SOCKS5-Inbound
//! - Шаг 1: tokio::net::TcpListener на порту 10808
//! - Шаг 2: SOCKS5-хэндшейк
//! - Шаг 3: Парсинг целевого адреса

pub mod tunnel;

use anyhow::Result;
use ghost_common::ClientConfig;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{info, warn};

/// Запустить клиент Ghost: слушать SOCKS5 и проксировать через сервер.
pub async fn run(config: ClientConfig) -> Result<()> {
    // Инициализация логирования
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ghost_client=info".into()),
        )
        .init();

    info!("🚀 Ghost Client starting...");
    info!("   SOCKS5 listen : {}", config.socks5_listen);
    info!("   Ghost server  : {}", config.server_addr);
    info!("   Padding       : {} (max {} bytes)", 
          config.enable_padding, config.max_padding_len);
    info!("   Fragmentation : {}", config.enable_fragmentation);

    let config = Arc::new(config);

    // Шаг 1: TcpListener
    let listener = TcpListener::bind(&config.socks5_listen).await?;
    info!("✅ SOCKS5 proxy listening on {}", config.socks5_listen);

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let cfg = config.clone();

        tokio::spawn(async move {
            info!("[{}] New SOCKS5 connection", peer_addr);
            if let Err(e) = handle_socks5_client(stream, cfg).await {
                warn!("[{}] Connection error: {}", peer_addr, e);
            }
        });
    }
}

/// Обработка одного SOCKS5-клиента: хэндшейк → парсинг → туннель.
async fn handle_socks5_client(
    stream: tokio::net::TcpStream,
    config: Arc<ClientConfig>,
) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = stream;

    // ── Шаг 2: SOCKS5-хэндшейк ────────────────────────────────────────

    // Читаем приветствие клиента: VER(1) NMETHODS(1) METHODS(NMETHODS)
    let mut buf = [0u8; 256];
    stream.read_exact(&mut buf[..2]).await?;

    let version = buf[0];
    let nmethods = buf[1] as usize;

    if version != 0x05 {
        anyhow::bail!("Not a SOCKS5 connection (version={:#04x})", version);
    }

    if nmethods > 0 {
        stream.read_exact(&mut buf[..nmethods]).await?;
    }

    // Проверяем, есть ли NO AUTH (0x00) среди методов
    let no_auth_supported = buf[..nmethods].iter().any(|&m| m == 0x00);

    if !no_auth_supported {
        // Отвечаем: нет приемлемого метода
        stream.write_all(&[0x05, 0xFF]).await?;
        anyhow::bail!("Client doesn't support NO AUTH method");
    }

    // Отвечаем: VER=5, METHOD=0x00 (No Auth)
    stream.write_all(&[0x05, 0x00]).await?;
    tracing::debug!("SOCKS5 handshake: method selected = No Auth");

    // ── Шаг 3: Парсинг SOCKS5-запроса ─────────────────────────────────

    // Запрос: VER(1) CMD(1) RSV(1) ATYP(1) DST.ADDR(?) DST.PORT(2)
    stream.read_exact(&mut buf[..4]).await?;

    let version = buf[0];
    let cmd = buf[1];
    let _rsv = buf[2]; // зарезервировано
    let atyp = buf[3];

    if version != 0x05 {
        anyhow::bail!("Invalid SOCKS5 version in request: {:#04x}", version);
    }

    // Поддерживаем только CONNECT (0x01)
    if cmd != 0x01 {
        // Отвечаем: command not supported
        let reply = socks5_reply(0x07, &ghost_common::TargetAddr::Domain(
            "0.0.0.0".to_string(), 0
        ));
        stream.write_all(&reply).await?;
        anyhow::bail!("Unsupported SOCKS5 command: {:#04x} (only CONNECT supported)", cmd);
    }

    // Парсинг целевого адреса в зависимости от ATYP
    let target_addr = match atyp {
        0x01 => {
            // IPv4: 4 байта + 2 байта порт
            stream.read_exact(&mut buf[..6]).await?;
            let octets: [u8; 4] = buf[..4].try_into()?;
            let port = u16::from_be_bytes([buf[4], buf[5]]);
            ghost_common::TargetAddr::IpV4(std::net::SocketAddrV4::new(
                std::net::Ipv4Addr::from(octets),
                port,
            ))
        }
        0x03 => {
            // Domain: 1 байт длина + домен + 2 байта порт
            stream.read_exact(&mut buf[..1]).await?;
            let domain_len = buf[0] as usize;
            if domain_len == 0 || domain_len > 255 {
                anyhow::bail!("Invalid domain length: {}", domain_len);
            }
            stream.read_exact(&mut buf[..domain_len + 2]).await?;
            let domain = String::from_utf8(buf[..domain_len].to_vec())?;
            let port = u16::from_be_bytes([buf[domain_len], buf[domain_len + 1]]);
            ghost_common::TargetAddr::Domain(domain, port)
        }
        0x04 => {
            // IPv6: 16 байт + 2 байта порт
            stream.read_exact(&mut buf[..18]).await?;
            let octets: [u8; 16] = buf[..16].try_into()?;
            let port = u16::from_be_bytes([buf[16], buf[17]]);
            ghost_common::TargetAddr::IpV6(std::net::SocketAddrV6::new(
                std::net::Ipv6Addr::from(octets),
                port,
                0,
                0,
            ))
        }
        _ => {
            anyhow::bail!("Unsupported SOCKS5 address type: {:#04x}", atyp);
        }
    };

    info!("SOCKS5 target: {}", target_addr);

    // ── Отправляем SOCKS5-ответ: успех ────────────────────────────────

    // BND.ADDR = 0.0.0.0:0 (мы не знаем реальный bind-адрес удалённого сервера)
    let reply = socks5_reply(0x00, &ghost_common::TargetAddr::IpV4(
        std::net::SocketAddrV4::new(std::net::Ipv4Addr::new(0, 0, 0, 0), 0)
    ));
    stream.write_all(&reply).await?;

    // ── Передаём управление туннелю ───────────────────────────────────

    tunnel::proxy_through_ghost(stream, target_addr, config).await
}

/// Формирует SOCKS5-ответ.
///
/// Формат: VER(1) REP(1) RSV(1) ATYP(1) BND.ADDR(?) BND.PORT(2)
fn socks5_reply(rep: u8, bind_addr: &ghost_common::TargetAddr) -> Vec<u8> {
    let mut reply = vec![0x05, rep, 0x00];

    match bind_addr {
        ghost_common::TargetAddr::None => {
            // В SOCKS5-ответе None не используется, записываем 0.0.0.0:0
            reply.push(0x01);
            reply.extend_from_slice(&[0, 0, 0, 0]);
            reply.extend_from_slice(&0u16.to_be_bytes());
        }
        ghost_common::TargetAddr::IpV4(a) => {
            reply.push(0x01);
            reply.extend_from_slice(&a.ip().octets());
            reply.extend_from_slice(&a.port().to_be_bytes());
        }
        ghost_common::TargetAddr::IpV6(a) => {
            reply.push(0x04);
            reply.extend_from_slice(&a.ip().octets());
            reply.extend_from_slice(&a.port().to_be_bytes());
        }
        ghost_common::TargetAddr::Domain(d, p) => {
            reply.push(0x03);
            reply.push(d.len() as u8);
            reply.extend_from_slice(d.as_bytes());
            reply.extend_from_slice(&p.to_be_bytes());
        }
    }

    reply
}
