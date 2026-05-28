//! Единый токен подключения.
//!
//! Формат: `rlz_<base64url(JSON)>`
//!
//! JSON-содержимое:
//! ```json
//! {
//!   "s": "203.0.113.5:443",   // server address
//!   "k": "a1b2c3d4...",       // user UUID (32 hex chars)
//!   "m": "www.apple.com",     // mask domain
//!   "a": "deadbeef..."        // reality auth key (64 hex chars)
//! }
//! ```

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};

use crate::ClientConfig;

const TOKEN_PREFIX: &str = "rlz_";

/// Токен подключения — все параметры в одной строке.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionToken {
    /// Адрес сервера (`ip:port`)
    pub s: String,
    /// UUID пользователя (32 hex символа)
    pub k: String,
    /// Домен маскировки SNI
    pub m: String,
    /// Reality auth key (64 hex символа)
    pub a: String,
}

impl ConnectionToken {
    pub fn new(server_addr: String, user_id: String, mask_domain: String, auth_key: String) -> Self {
        Self {
            s: server_addr,
            k: user_id,
            m: mask_domain,
            a: auth_key,
        }
    }

    /// Закодировать токен в строку `rlz_<base64url>`.
    pub fn encode(&self) -> Result<String, TokenError> {
        let json = serde_json::to_string(self).map_err(TokenError::SerializeError)?;
        let b64 = URL_SAFE_NO_PAD.encode(json.as_bytes());
        Ok(format!("{}{}", TOKEN_PREFIX, b64))
    }

    /// Декодировать строку `rlz_<base64url>` в токен.
    pub fn decode(input: &str) -> Result<Self, TokenError> {
        let b64 = input
            .strip_prefix(TOKEN_PREFIX)
            .ok_or(TokenError::MissingPrefix)?;

        let json_bytes = URL_SAFE_NO_PAD
            .decode(b64)
            .map_err(TokenError::Base64Error)?;

        let token: ConnectionToken =
            serde_json::from_slice(&json_bytes).map_err(TokenError::DeserializeError)?;

        // Базовая валидация
        if token.s.is_empty() {
            return Err(TokenError::InvalidField("server address is empty"));
        }
        if token.k.len() != 32 || !token.k.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(TokenError::InvalidField(
                "user_id must be 32 hex characters",
            ));
        }
        if token.m.is_empty() {
            return Err(TokenError::InvalidField("mask domain is empty"));
        }

        Ok(token)
    }

    /// Преобразовать в `ClientConfig` для запуска прокси.
    pub fn to_client_config(&self) -> ClientConfig {
        ClientConfig {
            socks5_listen: "127.0.0.1:10808".to_string(),
            server_addr: self.s.clone(),
            user_id: self.k.clone(),
            enable_padding: true,
            enable_fragmentation: false,
            max_padding_len: 64,
            mask_domain: self.m.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("token must start with '{TOKEN_PREFIX}'")]
    MissingPrefix,

    #[error("invalid base64: {0}")]
    Base64Error(base64::DecodeError),

    #[error("invalid JSON: {0}")]
    DeserializeError(serde_json::Error),

    #[error("serialization failed: {0}")]
    SerializeError(serde_json::Error),

    #[error("invalid field: {0}")]
    InvalidField(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encode_decode() {
        let token = ConnectionToken::new(
            "203.0.113.5:443".to_string(),
            "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8".to_string(),
            "www.apple.com".to_string(),
            "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string(),
        );

        let encoded = token.encode().unwrap();
        assert!(encoded.starts_with("rlz_"));

        let decoded = ConnectionToken::decode(&encoded).unwrap();
        assert_eq!(decoded.s, "203.0.113.5:443");
        assert_eq!(decoded.k, "a1b2c3d4e5f6a7b8a1b2c3d4e5f6a7b8");
        assert_eq!(decoded.m, "www.apple.com");
        assert_eq!(decoded.a, token.a);
    }

    #[test]
    fn decode_rejects_missing_prefix() {
        let result = ConnectionToken::decode("not_a_token");
        assert!(result.is_err());
    }

    #[test]
    fn decode_rejects_invalid_user_id() {
        let token = ConnectionToken {
            s: "1.2.3.4:443".to_string(),
            k: "short".to_string(),
            m: "www.apple.com".to_string(),
            a: "aa".to_string(),
        };
        let encoded = token.encode().unwrap();
        let result = ConnectionToken::decode(&encoded);
        assert!(result.is_err());
    }

    #[test]
    fn to_client_config_works() {
        let token = ConnectionToken::new(
            "10.0.0.1:443".to_string(),
            "00000000000000000000000000000001".to_string(),
            "www.google.com".to_string(),
            "abcd".to_string(),
        );
        let cfg = token.to_client_config();
        assert_eq!(cfg.server_addr, "10.0.0.1:443");
        assert_eq!(cfg.user_id, "00000000000000000000000000000001");
        assert_eq!(cfg.socks5_listen, "127.0.0.1:10808");
    }
}
