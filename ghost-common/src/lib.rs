//! ghost-common: Общие типы и константы протокола Ghost.
//!
//! Фрейминг протокола:
//! ```text
//! +----------+----------+-----------+----------+---------+
//! | Ver (1B) | ID (16B) | AddrType  | Addr     | Payload |
//! +----------+----------+-----------+----------+---------+
//! ```
//!
//! AddrType = 0x01 → IPv4 (4 байта + 2 байта порт)
//! AddrType = 0x03 → Domain (1 байт длина + домен + 2 байта порт)
//! AddrType = 0x04 → IPv6 (16 байт + 2 байта порт)

pub mod stealth;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use thiserror::Error;

// ── Константы протокола ────────────────────────────────────────────────

/// Версия протокола Ghost
pub const PROTOCOL_VERSION: u8 = 0x01;

/// Размер user ID (UUID без дефисов → 16 байт)
pub const USER_ID_LEN: usize = 16;

/// Максимальный размер одного payload-фрейма (64 KiB)
pub const MAX_FRAME_SIZE: usize = 64 * 1024;

/// Максимальный размер паддинга (до 255 байт мусора)
pub const MAX_PADDING_LEN: usize = 255;

// ── Типы адресов (аналогично SOCKS5) ───────────────────────────────────

pub const ADDR_TYPE_IPV4: u8 = 0x01;
pub const ADDR_TYPE_DOMAIN: u8 = 0x03;
pub const ADDR_TYPE_IPV6: u8 = 0x04;
/// Адрес уже известен серверу (используется в data-only фреймах после init)
pub const ADDR_TYPE_NONE: u8 = 0x00;

// ── Ошибки ─────────────────────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("invalid protocol version: expected {expected}, got {got}")]
    InvalidVersion { expected: u8, got: u8 },

    #[error("invalid address type: {0:#04x}")]
    InvalidAddrType(u8),

    #[error("frame too large: {size} > {max}")]
    FrameTooLarge { size: usize, max: usize },

    #[error("buffer underflow: need {need} bytes, have {have}")]
    BufferUnderflow { need: usize, have: usize },

    #[error("invalid domain length: {0}")]
    InvalidDomainLength(usize),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ── Целевой адрес ──────────────────────────────────────────────────────

/// Адрес целевого хоста, поддерживающий IPv4, IPv6 и доменные имена.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetAddr {
    /// Нет адреса (data-only фрейм, адрес уже известен серверу)
    None,
    IpV4(SocketAddrV4),
    IpV6(SocketAddrV6),
    Domain(String, u16),
}

impl TargetAddr {
    /// Извлечь порт.
    pub fn port(&self) -> u16 {
        match self {
            TargetAddr::None => 0,
            TargetAddr::IpV4(a) => a.port(),
            TargetAddr::IpV6(a) => a.port(),
            TargetAddr::Domain(_, p) => *p,
        }
    }

    /// Сериализация в байты (для фрейма протокола Ghost).
    pub fn encode(&self, buf: &mut BytesMut) {
        match self {
            TargetAddr::None => {
                buf.put_u8(ADDR_TYPE_NONE);
            }
            TargetAddr::IpV4(a) => {
                buf.put_u8(ADDR_TYPE_IPV4);
                buf.put_slice(&a.ip().octets());
                buf.put_u16(a.port());
            }
            TargetAddr::IpV6(a) => {
                buf.put_u8(ADDR_TYPE_IPV6);
                buf.put_slice(&a.ip().octets());
                buf.put_u16(a.port());
            }
            TargetAddr::Domain(domain, port) => {
                buf.put_u8(ADDR_TYPE_DOMAIN);
                let domain_bytes = domain.as_bytes();
                assert!(
                    domain_bytes.len() <= 255,
                    "domain name too long for encoding"
                );
                buf.put_u8(domain_bytes.len() as u8);
                buf.put_slice(domain_bytes);
                buf.put_u16(*port);
            }
        }
    }

    /// Десериализация из буфера.
    pub fn decode(buf: &mut BytesMut) -> Result<Self, ProtocolError> {
        if buf.remaining() < 1 {
            return Err(ProtocolError::BufferUnderflow { need: 1, have: 0 });
        }
        let addr_type = buf.get_u8();
        match addr_type {
            ADDR_TYPE_NONE => Ok(TargetAddr::None),
            ADDR_TYPE_IPV4 => {
                if buf.remaining() < 4 + 2 {
                    return Err(ProtocolError::BufferUnderflow {
                        need: 6,
                        have: buf.remaining(),
                    });
                }
                let octets: [u8; 4] = buf.copy_to_bytes(4).as_ref().try_into().unwrap();
                let port = buf.get_u16();
                Ok(TargetAddr::IpV4(SocketAddrV4::new(
                    Ipv4Addr::from(octets),
                    port,
                )))
            }
            ADDR_TYPE_IPV6 => {
                if buf.remaining() < 16 + 2 {
                    return Err(ProtocolError::BufferUnderflow {
                        need: 18,
                        have: buf.remaining(),
                    });
                }
                let octets: [u8; 16] = buf.copy_to_bytes(16).as_ref().try_into().unwrap();
                let port = buf.get_u16();
                Ok(TargetAddr::IpV6(SocketAddrV6::new(
                    Ipv6Addr::from(octets),
                    port,
                    0,
                    0,
                )))
            }
            ADDR_TYPE_DOMAIN => {
                if buf.remaining() < 1 {
                    return Err(ProtocolError::BufferUnderflow {
                        need: 1,
                        have: 0,
                    });
                }
                let domain_len = buf.get_u8() as usize;
                if domain_len == 0 {
                    return Err(ProtocolError::InvalidDomainLength(0));
                }
                if buf.remaining() < domain_len + 2 {
                    return Err(ProtocolError::BufferUnderflow {
                        need: domain_len + 2,
                        have: buf.remaining(),
                    });
                }
                let domain_bytes = buf.copy_to_bytes(domain_len);
                let domain = String::from_utf8(domain_bytes.to_vec())
                    .map_err(|_| ProtocolError::InvalidDomainLength(domain_len))?;
                let port = buf.get_u16();
                Ok(TargetAddr::Domain(domain, port))
            }
            _ => Err(ProtocolError::InvalidAddrType(addr_type)),
        }
    }

    /// Закодированная длина адреса (без payload).
    pub fn encoded_len(&self) -> usize {
        match self {
            TargetAddr::None => 1,                    // type only
            TargetAddr::IpV4(_) => 1 + 4 + 2,        // type + ip + port
            TargetAddr::IpV6(_) => 1 + 16 + 2,        // type + ip + port
            TargetAddr::Domain(d, _) => 1 + 1 + d.len() + 2, // type + len + domain + port
        }
    }
}

impl std::fmt::Display for TargetAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TargetAddr::None => write!(f, "None"),
            TargetAddr::IpV4(a) => write!(f, "{}", a),
            TargetAddr::IpV6(a) => write!(f, "{}", a),
            TargetAddr::Domain(d, p) => write!(f, "{}:{}", d, p),
        }
    }
}

impl From<SocketAddr> for TargetAddr {
    fn from(addr: SocketAddr) -> Self {
        match addr {
            SocketAddr::V4(v4) => TargetAddr::IpV4(v4),
            SocketAddr::V6(v6) => TargetAddr::IpV6(v6),
        }
    }
}

// ── Фрейм протокола Ghost ──────────────────────────────────────────────

/// Один фрейм данных протокола Ghost (до шифрования).
///
/// Wire-формат:
/// ```text
/// [Ver:1][UserID:16][TargetAddr][PayloadLen:2][Payload:~][PaddingLen:1][Padding:~]
/// ```
#[derive(Debug, Clone)]
pub struct GhostFrame {
    pub version: u8,
    pub user_id: [u8; USER_ID_LEN],
    pub target: TargetAddr,
    pub payload: Bytes,
    pub padding: Bytes,
}

impl GhostFrame {
    /// Создать новый фрейм.
    pub fn new(user_id: [u8; USER_ID_LEN], target: TargetAddr, payload: Bytes) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            user_id,
            target,
            payload,
            padding: Bytes::new(),
        }
    }

    /// Добавить рандомный паддинг.
    pub fn with_random_padding(mut self, max_len: usize) -> Self {
        use rand::Rng;
        let pad_len = rand::thread_rng().gen_range(0..=max_len.min(MAX_PADDING_LEN));
        let mut pad = vec![0u8; pad_len];
        rand::thread_rng().fill(&mut pad[..]);
        self.padding = Bytes::from(pad);
        self
    }

    /// Закодировать фрейм в байты.
    pub fn encode(&self) -> BytesMut {
        let addr_len = self.target.encoded_len();
        // Ver(1) + UserID(16) + Addr + PayloadLen(2) + Payload + PaddingLen(1) + Padding
        let total = 1 + USER_ID_LEN + addr_len + 2 + self.payload.len() + 1 + self.padding.len();
        let mut buf = BytesMut::with_capacity(total);

        buf.put_u8(self.version);
        buf.put_slice(&self.user_id);
        self.target.encode(&mut buf);
        buf.put_u16(self.payload.len() as u16);
        buf.put_slice(&self.payload);
        buf.put_u8(self.padding.len() as u8);
        buf.put_slice(&self.padding);

        buf
    }

    /// Декодировать фрейм из байтов.
    pub fn decode(mut buf: BytesMut) -> Result<Self, ProtocolError> {
        // Версия
        if buf.remaining() < 1 {
            return Err(ProtocolError::BufferUnderflow { need: 1, have: 0 });
        }
        let version = buf.get_u8();
        if version != PROTOCOL_VERSION {
            return Err(ProtocolError::InvalidVersion {
                expected: PROTOCOL_VERSION,
                got: version,
            });
        }

        // UserID
        if buf.remaining() < USER_ID_LEN {
            return Err(ProtocolError::BufferUnderflow {
                need: USER_ID_LEN,
                have: buf.remaining(),
            });
        }
        let mut user_id = [0u8; USER_ID_LEN];
        buf.copy_to_slice(&mut user_id);

        // Целевой адрес
        let target = TargetAddr::decode(&mut buf)?;

        // Payload
        if buf.remaining() < 2 {
            return Err(ProtocolError::BufferUnderflow {
                need: 2,
                have: buf.remaining(),
            });
        }
        let payload_len = buf.get_u16() as usize;
        if buf.remaining() < payload_len {
            return Err(ProtocolError::BufferUnderflow {
                need: payload_len,
                have: buf.remaining(),
            });
        }
        let payload = buf.copy_to_bytes(payload_len);

        // Padding
        if buf.remaining() < 1 {
            return Err(ProtocolError::BufferUnderflow {
                need: 1,
                have: buf.remaining(),
            });
        }
        let padding_len = buf.get_u8() as usize;
        let padding = if padding_len > 0 && buf.remaining() >= padding_len {
            buf.copy_to_bytes(padding_len)
        } else {
            Bytes::new()
        };

        Ok(GhostFrame {
            version,
            user_id,
            target,
            payload,
            padding,
        })
    }
}

// ── Конфигурация ───────────────────────────────────────────────────────

/// Конфигурация клиента Ghost.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClientConfig {
    /// Адрес локального SOCKS5-прокси
    pub socks5_listen: String,

    /// Адрес удалённого Ghost-сервера
    pub server_addr: String,

    /// UUID пользователя (hex-строка 32 символа)
    pub user_id: String,

    /// Включить Dynamic Padding
    pub enable_padding: bool,

    /// Включить TCP-фрагментацию
    pub enable_fragmentation: bool,

    /// Максимальный размер паддинга (0–255)
    pub max_padding_len: u8,

    /// Домен для маскировки SNI (Reality), например `www.apple.com`.
    /// Передаётся в TLS-обёртку клиента.
    pub mask_domain: String,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            socks5_listen: "127.0.0.1:10808".to_string(),
            server_addr: "127.0.0.1:443".to_string(),
            user_id: "00000000000000000000000000000001".to_string(),
            enable_padding: true,
            enable_fragmentation: false,
            max_padding_len: 64,
            mask_domain: "www.apple.com".to_string(),
        }
    }
}

/// Конфигурация сервера Ghost.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerConfig {
    /// Адрес, на котором сервер принимает подключения
    pub listen_addr: String,

    /// Список разрешённых UUID (hex-строки)
    pub allowed_users: Vec<String>,

    /// Включить Dynamic Padding (сервер тоже добавляет паддинг в ответ)
    pub enable_padding: bool,

    /// Максимальный размер паддинга в ответах
    pub max_padding_len: u8,

    /// Включить Reality-маскировку (TLS SNI)
    pub enable_reality: bool,

    /// Домен для маскировки SNI (например, www.apple.com)
    pub mask_domain: String,

    /// Приватный ключ авторизации (hex-строка 64 символа = 32 байта)
    pub reality_auth_key: String,

    /// Включить JA4-проверку клиентов
    pub verify_ja4: bool,

    /// Разрешённые JA4-отпечатки
    pub allowed_ja4: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:443".to_string(),
            allowed_users: vec!["00000000000000000000000000000001".to_string()],
            enable_padding: true,
            max_padding_len: 64,
            enable_reality: false,
            mask_domain: "www.apple.com".to_string(),
            reality_auth_key: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            verify_ja4: false,
            allowed_ja4: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_addr_domain_encode_decode() {
        let addr = TargetAddr::Domain("youtube.com".to_string(), 443);
        let mut buf = BytesMut::new();
        addr.encode(&mut buf);

        let decoded = TargetAddr::decode(&mut buf).unwrap();
        assert_eq!(addr, decoded);
    }

    #[test]
    fn test_target_addr_ipv4_encode_decode() {
        let addr = TargetAddr::IpV4(SocketAddrV4::new(Ipv4Addr::new(1, 1, 1, 1), 443));
        let mut buf = BytesMut::new();
        addr.encode(&mut buf);

        let decoded = TargetAddr::decode(&mut buf).unwrap();
        assert_eq!(addr, decoded);
    }

    #[test]
    fn test_ghost_frame_encode_decode() {
        let user_id = [0x01u8; 16];
        let target = TargetAddr::Domain("example.com".to_string(), 443);
        let payload = Bytes::from_static(b"hello world");
        let frame = GhostFrame::new(user_id, target, payload).with_random_padding(32);

        let encoded = frame.encode();
        let decoded = GhostFrame::decode(encoded).unwrap();

        assert_eq!(decoded.version, PROTOCOL_VERSION);
        assert_eq!(decoded.user_id, user_id);
        assert_eq!(decoded.payload, Bytes::from_static(b"hello world"));
    }
}
