//! JA4 Fingerprint Spoofing: подмена TLS-отпечатка под реальный браузер.
//!
//! JA4 — это метод фингерпринтинга TLS-клиентов, анализирующий:
//! 1. Версию TLS
//! 2. Набор шифров (cipher suites)
//! 3. Расширения TLS
//! 4. Формат elliptic curves и signature algorithms
//!
//! Для обхода DPI мы должны формировать TLS ClientHello, отпечаток которого
//! совпадает с реальным Google Chrome.

use std::collections::BTreeMap;

/// Предустановленные JA4-отпечатки популярных браузеров.
///
/// Формат JA4: `<tls_version><cipher_count><extension_count><first_cipher_hex>`
/// Подробнее: https://engineering.salesforce.com/the-ja4-fingerprint/
pub struct Ja4Profile {
    /// Имя браузера
    pub name: &'static str,
    /// Версия браузера
    pub version: &'static str,
    /// Cipher suites в порядке предпочтения (как в Chrome)
    pub cipher_suites: Vec<u16>,
    /// TLS-расширения в порядке Chrome
    pub extensions: Vec<u16>,
    /// Supported groups (elliptic curves)
    pub supported_groups: Vec<u16>,
    /// Signature algorithms
    pub signature_algorithms: Vec<u16>,
    /// ALPN protocols
    pub alpn: Vec<&'static str>,
    /// Версия TLS (0x0303 = TLS 1.2, 0x0304 = TLS 1.3)
    pub tls_version: u16,
}

/// JA4-профиль Google Chrome 131 (актуальный на 2025).
///
/// Этот профиль имитирует реальный ClientHello Chrome, включая:
/// - Порядок cipher suites
/// - Порядок расширений
/// - Supported groups
/// - Signature algorithms
pub fn chrome_131_profile() -> Ja4Profile {
    Ja4Profile {
        name: "Chrome",
        version: "131",
        tls_version: 0x0304, // TLS 1.3

        cipher_suites: vec![
            // TLS 1.3
            0x1301, // TLS_AES_128_GCM_SHA256
            0x1302, // TLS_AES_256_GCM_SHA384
            0x1303, // TLS_CHACHA20_POLY1305_SHA256
            // TLS 1.2 (fallback)
            0xC02B, // TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256
            0xC02F, // TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256
            0xC02C, // TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384
            0xC030, // TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384
            0xCCA9, // TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256
            0xCCA8, // TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256
            0xC013, // TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA
            0xC014, // TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA
            0x009C, // TLS_RSA_WITH_AES_128_GCM_SHA256
            0x009D, // TLS_RSA_WITH_AES_256_GCM_SHA384
            0x002F, // TLS_RSA_WITH_AES_128_CBC_SHA
            0x0035, // TLS_RSA_WITH_AES_256_CBC_SHA
        ],

        extensions: vec![
            0x0000, // server_name (SNI)
            0x0010, // application_layer_protocol_negotiation (ALPN)
            0x0017, // extended_master_secret
            0x001B, // compress_certificate
            0x0033, // key_share
            0x002B, // supported_versions
            0x002D, // psk_key_exchange_modes
            0x0039, // pre_shared_key
            0x000D, // signature_algorithms
            0x000B, // ec_point_formats
            0x000A, // supported_groups
            0x0015, // padding
            0xFF01, // renegotiation_info
            0x0012, // signed_certificate_timestamp
        ],

        supported_groups: vec![
            0x001D, // x25519
            0x0017, // secp256r1
            0x0018, // secp384r1
            0x0019, // secp521r1
            0x0100, // ffdhe2048
            0x0101, // ffdhe3072
        ],

        signature_algorithms: vec![
            0x0403, // ecdsa_secp256r1_sha256
            0x0503, // ecdsa_secp384r1_sha384
            0x0603, // ecdsa_secp521r1_sha512
            0x0804, // rsa_pss_rsae_sha256
            0x0805, // rsa_pss_rsae_sha384
            0x0806, // rsa_pss_rsae_sha512
            0x0401, // rsa_pkcs1_sha256
            0x0501, // rsa_pkcs1_sha384
            0x0601, // rsa_pkcs1_sha512
            0x0201, // rsa_pkcs1_sha1
        ],

        alpn: vec!["h2", "http/1.1"],
    }
}

/// JA4-профиль Firefox 133.
pub fn firefox_133_profile() -> Ja4Profile {
    Ja4Profile {
        name: "Firefox",
        version: "133",
        tls_version: 0x0304,

        cipher_suites: vec![
            0x1301, // TLS_AES_128_GCM_SHA256
            0x1303, // TLS_CHACHA20_POLY1305_SHA256
            0x1302, // TLS_AES_256_GCM_SHA384
            0xC02B, // TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256
            0xCCA9, // TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256
            0xC02F, // TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256
            0xCCA8, // TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256
            0xC02C, // TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384
            0xC030, // TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384
            0xC013, // TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA
            0xC014, // TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA
        ],

        extensions: vec![
            0x0000, // server_name
            0x0010, // ALPN
            0x0005, // status_request
            0x0017, // extended_master_secret
            0x002B, // supported_versions
            0x000D, // signature_algorithms
            0x000A, // supported_groups
            0x0033, // key_share
            0x002D, // psk_key_exchange_modes
            0x0015, // padding
            0x0012, // signed_certificate_timestamp
        ],

        supported_groups: vec![
            0x001D, // x25519
            0x0017, // secp256r1
            0x0018, // secp384r1
        ],

        signature_algorithms: vec![
            0x0403, // ecdsa_secp256r1_sha256
            0x0503, // ecdsa_secp384r1_sha384
            0x0603, // ecdsa_secp521r1_sha512
            0x0804, // rsa_pss_rsae_sha256
            0x0805, // rsa_pss_rsae_sha384
            0x0806, // rsa_pss_rsae_sha512
            0x0401, // rsa_pkcs1_sha256
            0x0501, // rsa_pkcs1_sha384
            0x0601, // rsa_pkcs1_sha512
        ],

        alpn: vec!["h2", "http/1.1"],
    }
}

/// Вычислить JA4-отпечаток из профиля (упрощённый расчёт).
///
/// Полный JA4 расчёт: `<a><b><c><d>` где:
/// - a: TLS version + cipher count + extension count
/// - b: first cipher suite
/// - c: last cipher suite
/// - d: sorted extension list hash
pub fn calculate_ja4(profile: &Ja4Profile) -> String {
    let tls_ver = match profile.tls_version {
        0x0304 => "t13",
        0x0303 => "t12",
        _ => "txx",
    };
    let cipher_count = profile.cipher_suites.len();
    let ext_count = profile.extensions.len();

    // Сортируем расширения для хеширования (часть JA4)
    let mut sorted_exts: Vec<u16> = profile.extensions.clone();
    sorted_exts.sort();
    let ext_hex: String = sorted_exts
        .iter()
        .map(|e| format!("{:04x}", e))
        .collect();

    format!(
        "{}{:02x}{:02x}{:04x}{:04x}{}",
        tls_ver,
        cipher_count,
        ext_count,
        profile.cipher_suites.first().unwrap_or(&0),
        profile.cipher_suites.last().unwrap_or(&0),
        simple_hash(&ext_hex)
    )
}

/// Простая хеш-функция для JA4 (часть 'd' фингерпринта).
fn simple_hash(s: &str) -> String {
    let mut hash: u32 = 0x811c9dc5; // FNV offset basis
    for b in s.bytes() {
        hash ^= b as u32;
        hash = hash.wrapping_mul(0x01000193); // FNV prime
    }
    format!("{:08x}", hash)
}

/// Проверить JA4-отпечаток клиента против списка разрешённых.
pub fn verify_ja4(client_ja4: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|a| a == client_ja4)
}

/// Извлечь JA4-отпечаток из сырых байтов TLS ClientHello.
///
/// Упрощённая реализация: извлекает базовую информацию из ClientHello
/// и формирует JA4-подобный отпечаток.
pub fn extract_ja4_from_client_hello(data: &[u8]) -> Option<String> {
    use tls_parser::{parse_tls_plaintext, TlsMessage, TlsMessageHandshake};

    let result = parse_tls_plaintext(data);
    match result {
        Ok((_, record)) => {
            for msg in &record.msg {
                match msg {
                    TlsMessage::Handshake(TlsMessageHandshake::ClientHello(ch)) => {
                        let cipher_count = ch.ciphers.len();
                        let ext_count = ch.ext.as_ref().map(|e| e.len()).unwrap_or(0);

                        let first_cipher = ch.ciphers.first().map(|c| c.0).unwrap_or(0);
                        let last_cipher = ch.ciphers.last().map(|c| c.0).unwrap_or(0);

                        let tls_ver = match ch.version.0 {
                            0x0304 => "t13",
                            0x0303 => "t12",
                            _ => "txx",
                        };

                        let ext_hash = ch.ext
                            .as_ref()
                            .map(|e| e.len() as u32)
                            .unwrap_or(0);

                        return Some(format!(
                            "{}{:02x}{:02x}{:04x}{:04x}{:08x}",
                            tls_ver,
                            cipher_count,
                            ext_count,
                            first_cipher,
                            last_cipher,
                            ext_hash.wrapping_mul(0x01000193)
                        ));
                    }
                    _ => {}
                }
            }
            None
        }
        Err(_) => None,
    }
}

/// Получить отпечаток профиля Chrome (для добавления в allowed_ja4).
pub fn chrome_ja4_fingerprint() -> String {
    calculate_ja4(&chrome_131_profile())
}

/// Получить отпечаток профиля Firefox.
pub fn firefox_ja4_fingerprint() -> String {
    calculate_ja4(&firefox_133_profile())
}

/// База данных известных JA4-отпечатков по браузерам.
pub fn known_ja4_database() -> BTreeMap<String, &'static str> {
    let mut db = BTreeMap::new();
    db.insert(chrome_ja4_fingerprint(), "Chrome 131");
    db.insert(firefox_ja4_fingerprint(), "Firefox 133");
    db
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chrome_profile_ja4() {
        let profile = chrome_131_profile();
        let ja4 = calculate_ja4(&profile);
        assert!(ja4.starts_with("t13"));
        assert!(ja4.len() > 10);
        println!("Chrome 131 JA4: {}", ja4);
    }

    #[test]
    fn test_firefox_profile_ja4() {
        let profile = firefox_133_profile();
        let ja4 = calculate_ja4(&profile);
        assert!(ja4.starts_with("t13"));
        println!("Firefox 133 JA4: {}", ja4);
    }

    #[test]
    fn test_ja4_verification() {
        let chrome_ja4 = chrome_ja4_fingerprint();
        let allowed = vec![chrome_ja4.clone()];

        assert!(verify_ja4(&chrome_ja4, &allowed));
        assert!(!verify_ja4("t13deadbeef", &allowed));
    }

    #[test]
    fn test_known_database() {
        let db = known_ja4_database();
        assert!(!db.is_empty());
        for (ja4, name) in &db {
            println!("{} → {}", ja4, name);
        }
    }
}
