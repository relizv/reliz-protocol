//! ghost-common/src/stealth: Стелс-механизмы протокола Ghost.
//!
//! - Dynamic Padding: рандомный мусор в конце пакетов
//! - TCP Fragmentation: дробление первых пакетов (ByeDPI-style)
//! - Packet size normalization

use bytes::{BufMut, BytesMut};
use rand::Rng;

// ═══════════════════════════════════════════════════════════════════════
//  Dynamic Padding
// ═══════════════════════════════════════════════════════════════════════

/// Стратегия выбора размера паддинга.
#[derive(Debug, Clone, Copy)]
pub enum PaddingStrategy {
    /// Фиксированный размер паддинга
    Fixed(usize),
    /// Случайный размер в диапазоне [min, max]
    Random { min: usize, max: usize },
    /// Нормализация размера пакета до ближайшего кратного (например, 16, 32, 64 байта)
    NormalizeToMultiple { multiple: usize, max_padding: usize },
}

impl Default for PaddingStrategy {
    fn default() -> Self {
        PaddingStrategy::Random { min: 0, max: 64 }
    }
}

/// Вычислить размер паддинга по стратегии.
pub fn calculate_padding_len(data_len: usize, strategy: &PaddingStrategy) -> usize {
    match strategy {
        PaddingStrategy::Fixed(len) => *len,
        PaddingStrategy::Random { min, max } => {
            let max_val = (*max).min(255);
            let min_val = (*min).min(max_val);
            rand::thread_rng().gen_range(min_val..=max_val)
        }
        PaddingStrategy::NormalizeToMultiple { multiple, max_padding } => {
            let remainder = data_len % multiple;
            if remainder == 0 {
                // Уже кратно — добавляем случайный multiple
                let extra = rand::thread_rng().gen_range(0..=2) * multiple;
                extra.min(*max_padding)
            } else {
                let needed = multiple - remainder;
                needed.min(*max_padding)
            }
        }
    }
}

/// Добавить паддинг к буферу. Возвращает (буфер_с_паддингом, длина_паддинга).
pub fn apply_padding(buf: &mut BytesMut, strategy: &PaddingStrategy) -> usize {
    let pad_len = calculate_padding_len(buf.len(), strategy);
    if pad_len > 0 {
        let mut padding = vec![0u8; pad_len];
        rand::thread_rng().fill(&mut padding[..]);
        buf.put_u8(pad_len as u8); // маркер длины паддинга
        buf.put_slice(&padding);
    } else {
        buf.put_u8(0); // паддинг отсутствует
    }
    pad_len
}

// ═══════════════════════════════════════════════════════════════════════
//  TCP Fragmentation (ByeDPI-style)
// ═══════════════════════════════════════════════════════════════════════

/// Конфигурация TCP-фрагментации для обхода DPI.
#[derive(Debug, Clone)]
pub struct FragmentationConfig {
    /// Включена ли фрагментация
    pub enabled: bool,
    /// Размер первого фрагмента (обычно 2–5 байт для ломания сигнатур TLS ClientHello)
    pub first_fragment_size: usize,
    /// Задержка между фрагментами в миллисекундах (0 = без задержки)
    pub inter_fragment_delay_ms: u64,
    /// Применять ли фрагментацию только к первому пакету соединения
    pub first_packet_only: bool,
}

impl Default for FragmentationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            first_fragment_size: 2,
            inter_fragment_delay_ms: 0,
            first_packet_only: true,
        }
    }
}

/// Результат фрагментации: список фрагментов для отправки.
#[derive(Debug)]
pub struct FragmentedData {
    /// Фрагменты данных для отправки по порядку
    pub fragments: Vec<BytesMut>,
    /// Задержка между фрагментами в мс
    pub inter_delay_ms: u64,
}

/// Фрагментировать данные для обхода DPI.
///
/// Стратегия: разбиваем первый пакет на два фрагмента так, чтобы
/// сигнатура протокола (например, TLS ClientHello) была разорвана
/// между фрагментами. DPI-системы, анализирующие только первый пакет,
/// не смогут распознать протокол.
pub fn fragment_data(data: &[u8], config: &FragmentationConfig) -> FragmentedData {
    if !config.enabled || data.len() <= config.first_fragment_size {
        return FragmentedData {
            fragments: vec![BytesMut::from(data)],
            inter_delay_ms: 0,
        };
    }

    let split_point = config.first_fragment_size.min(data.len());
    let first = BytesMut::from(&data[..split_point]);
    let second = BytesMut::from(&data[split_point..]);

    FragmentedData {
        fragments: vec![first, second],
        inter_delay_ms: config.inter_fragment_delay_ms,
    }
}

/// Интеллектуальная фрагментация TLS ClientHello.
///
/// TLS ClientHello начинается с:
/// ```text
/// [Content Type: 0x16][Version: 0x03 0x01][Length: 2 bytes][Handshake...]
/// ```
///
/// Мы ломаем заголовок записи между Content Type + Version и Length,
/// чтобы DPI не мог прочитать длину и тип записи в одном пакете.
pub fn fragment_tls_client_hello(data: &[u8], config: &FragmentationConfig) -> FragmentedData {
    if !config.enabled || data.len() < 5 {
        return FragmentedData {
            fragments: vec![BytesMut::from(data)],
            inter_delay_ms: 0,
        };
    }

    // Проверяем, что это действительно TLS record
    if data[0] != 0x16 {
        // Не TLS Handshake — фрагментируем по общему правилу
        return fragment_data(data, config);
    }

    // Ломаем после первых 2–3 байт (Content Type + половина Version)
    // Это гарантирует, что DPI не увидит полную сигнатуру TLS
    let split_point = config.first_fragment_size.min(4).max(2);
    let first = BytesMut::from(&data[..split_point]);
    let second = BytesMut::from(&data[split_point..]);

    FragmentedData {
        fragments: vec![first, second],
        inter_delay_ms: config.inter_fragment_delay_ms,
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Size Obfuscation
// ═══════════════════════════════════════════════════════════════════════

/// Нормализовать размер пакета, добавив паддинг до нужного размера.
///
/// Полезно для предотвращения анализа по размерам пакетов
/// (size-based fingerprinting).
pub fn normalize_packet_size(data: &mut BytesMut, target_size: usize) {
    if data.len() >= target_size {
        return;
    }
    let padding_needed = target_size - data.len();
    let mut padding = vec![0u8; padding_needed];
    rand::thread_rng().fill(&mut padding[..]);
    data.put_slice(&padding);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_padding_strategy_random() {
        let strategy = PaddingStrategy::Random { min: 10, max: 50 };
        for _ in 0..100 {
            let len = calculate_padding_len(100, &strategy);
            assert!(len >= 10 && len <= 50);
        }
    }

    #[test]
    fn test_padding_strategy_fixed() {
        let strategy = PaddingStrategy::Fixed(32);
        assert_eq!(calculate_padding_len(100, &strategy), 32);
    }

    #[test]
    fn test_padding_strategy_normalize() {
        let strategy = PaddingStrategy::NormalizeToMultiple {
            multiple: 16,
            max_padding: 64,
        };
        // 100 % 16 = 4 → нужно 12 байт паддинга
        let len = calculate_padding_len(100, &strategy);
        assert!((100 + len) % 16 == 0 || len == 64); // кратно или упёрлись в лимит
    }

    #[test]
    fn test_fragment_data_basic() {
        let config = FragmentationConfig {
            enabled: true,
            first_fragment_size: 2,
            inter_fragment_delay_ms: 0,
            first_packet_only: true,
        };
        let data = b"hello world";
        let result = fragment_data(data, &config);

        assert_eq!(result.fragments.len(), 2);
        assert_eq!(&result.fragments[0][..], b"he");
        assert_eq!(&result.fragments[1][..], b"llo world");
    }

    #[test]
    fn test_fragment_tls_client_hello() {
        let config = FragmentationConfig {
            enabled: true,
            first_fragment_size: 2,
            inter_fragment_delay_ms: 0,
            first_packet_only: true,
        };

        // Имитация TLS ClientHello
        let mut data = vec![0x16, 0x03, 0x01, 0x00, 0x80]; // заголовок TLS
        data.extend_from_slice(&[0u8; 128]); // тело

        let result = fragment_tls_client_hello(&data, &config);
        assert_eq!(result.fragments.len(), 2);
        // Первый фрагмент: только Content Type + часть Version
        assert_eq!(result.fragments[0].len(), 2);
        assert_eq!(result.fragments[0][0], 0x16);
    }

    #[test]
    fn test_fragment_disabled() {
        let config = FragmentationConfig {
            enabled: false,
            ..Default::default()
        };
        let data = b"hello";
        let result = fragment_data(data, &config);
        assert_eq!(result.fragments.len(), 1);
    }
}
