//! ghost-crypto: Шифрование и аутентификация протокола Ghost.
//!
//! Использует ChaCha20-Poly1305 (AEAD) для шифрования фреймов.
//! Каждый фрейм шифруется с уникальным nonce (12 байт), который
//! передаётся перед ciphertext + tag.

use bytes::{BufMut, Bytes, BytesMut};
use chacha20poly1305::{
    aead::{Aead, KeyInit, OsRng},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::Rng;

/// Размер ключа ChaCha20-Poly1305 (256 бит = 32 байта)
pub const KEY_SIZE: usize = 32;

/// Размер nonce ChaCha20-Poly1305 (96 бит = 12 байт)
pub const NONCE_SIZE: usize = 12;

/// Размер тега аутентификации (128 бит = 16 байт)
pub const TAG_SIZE: usize = 16;

/// Ошибка криптографической операции.
#[derive(Debug)]
pub enum CryptoError {
    /// Ошибка шифрования
    EncryptionFailed(String),
    /// Ошибка расшифровки (включая нарушение целостности)
    DecryptionFailed(String),
    /// Неверный размер ключа
    InvalidKeyLength(usize),
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptoError::EncryptionFailed(e) => write!(f, "encryption failed: {}", e),
            CryptoError::DecryptionFailed(e) => write!(f, "decryption failed: {}", e),
            CryptoError::InvalidKeyLength(len) => {
                write!(f, "invalid key length: {} (expected {})", len, KEY_SIZE)
            }
        }
    }
}

impl std::error::Error for CryptoError {}

// ── GhostCipher ────────────────────────────────────────────────────────

/// Шифратор/дешифратор протокола Ghost.
///
/// Wire-формат зашифрованного фрейма:
/// ```text
/// [Nonce:12][Ciphertext + Tag: N+16]
/// ```
pub struct GhostCipher {
    cipher: ChaCha20Poly1305,
}

impl GhostCipher {
    /// Создать шифратор из 32-байтового ключа.
    pub fn new(key: &[u8]) -> Result<Self, CryptoError> {
        if key.len() != KEY_SIZE {
            return Err(CryptoError::InvalidKeyLength(key.len()));
        }
        let key = Key::from_slice(key);
        let cipher = ChaCha20Poly1305::new(key);
        Ok(Self { cipher })
    }

    /// Сгенерировать случайный ключ.
    pub fn generate_key() -> [u8; KEY_SIZE] {
        let mut key = [0u8; KEY_SIZE];
        OsRng.fill(&mut key);
        key
    }

    /// Зашифровать данные. Возвращает nonce + ciphertext + tag.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<BytesMut, CryptoError> {
        // Генерируем случайный nonce для каждого фрейма
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        OsRng.fill(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

        // [Nonce:12][Ciphertext + Tag]
        let total = NONCE_SIZE + ciphertext.len();
        let mut buf = BytesMut::with_capacity(total);
        buf.put_slice(&nonce_bytes);
        buf.put_slice(&ciphertext);

        Ok(buf)
    }

    /// Расшифровать данные. Ожидает nonce + ciphertext + tag.
    pub fn decrypt(&self, data: &[u8]) -> Result<Bytes, CryptoError> {
        if data.len() < NONCE_SIZE + TAG_SIZE {
            return Err(CryptoError::DecryptionFailed(
                "data too short".to_string(),
            ));
        }

        let nonce = Nonce::from_slice(&data[..NONCE_SIZE]);
        let ciphertext = &data[NONCE_SIZE..];

        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;

        Ok(Bytes::from(plaintext))
    }

    /// Получить размер зашифрованных данных для заданного plaintext.
    pub fn encrypted_size(plaintext_len: usize) -> usize {
        NONCE_SIZE + plaintext_len + TAG_SIZE
    }
}

// ── Утилиты для деривации ключей ───────────────────────────────────────

/// Простейшая деривация ключа из UUID пользователя и pre-shared secret.
/// В реальном продакшене стоит использовать HKDF или Argon2.
pub fn derive_key(user_id: &[u8; 16], secret: &[u8]) -> [u8; KEY_SIZE] {
    // Простая (но рабочая) деривация через циклическое XOR
    let mut key = [0u8; KEY_SIZE];
    for i in 0..16 {
        key[i] = user_id[i] ^ secret[i % secret.len()];
        key[i + 16] = user_id[i] ^ secret[(i + 8) % secret.len()];
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = GhostCipher::generate_key();
        let cipher = GhostCipher::new(&key).unwrap();

        let plaintext = b"Hello, Ghost Protocol!";
        let encrypted = cipher.encrypt(plaintext).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();

        assert_eq!(&decrypted[..], plaintext);
    }

    #[test]
    fn test_different_nonces_per_encryption() {
        let key = GhostCipher::generate_key();
        let cipher = GhostCipher::new(&key).unwrap();

        let plaintext = b"same data";
        let enc1 = cipher.encrypt(plaintext).unwrap();
        let enc2 = cipher.encrypt(plaintext).unwrap();

        // Nonce должны быть разными
        assert_ne!(&enc1[..12], &enc2[..12]);
        // И весь ciphertext тоже
        assert_ne!(enc1, enc2);

        // Но оба расшифровываются корректно
        assert_eq!(&cipher.decrypt(&enc1).unwrap()[..], plaintext);
        assert_eq!(&cipher.decrypt(&enc2).unwrap()[..], plaintext);
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let key = GhostCipher::generate_key();
        let cipher = GhostCipher::new(&key).unwrap();

        let plaintext = b"important data";
        let mut encrypted = cipher.encrypt(plaintext).unwrap();

        // Подменяем один байт ciphertext
        let tamper_idx = NONCE_SIZE + 1;
        encrypted[tamper_idx] ^= 0xFF;

        // Расшифровка должна провалиться (Poly1305 тег не совпадёт)
        assert!(cipher.decrypt(&encrypted).is_err());
    }

    #[test]
    fn test_derive_key_deterministic() {
        let user_id = [0x01u8; 16];
        let secret = b"super_secret_key_12345";

        let key1 = derive_key(&user_id, secret);
        let key2 = derive_key(&user_id, secret);

        assert_eq!(key1, key2);
    }
}
