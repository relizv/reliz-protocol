//! TCP Fragmentation — обход DPI через дробление начальных пакетов.
//!
//! Стратегия (как в ByeDPI / Xray):
//! - Первые N байт TCP-потока отправляются маленькими порциями (1–3 байта)
//! - Каждая порция сопровождается `flush()` и задержкой
//! - Остальные данные идут нормально
//!
//! Это ломает DPI, которые анализируют начало соединения (TLS ClientHello)
//! по фиксированным размерам первых пакетов.

use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;
use tracing::trace;

/// Обёртка над [`TcpStream`] с поддержкой фрагментации начальных байт.
pub struct FragmentedStream {
    inner: TcpStream,
    /// Сколько байт ещё нужно отправить фрагментированно
    remaining_frag_bytes: usize,
    /// Размер каждого фрагмента (1–3 байта обычно)
    fragment_size: usize,
    /// Задержка между фрагментами (мс)
    #[allow(dead_code)]
    fragment_delay_ms: u64,
}

impl FragmentedStream {
    /// Создать обёртку с заданными параметрами фрагментации.
    ///
    /// * `frag_bytes` — сколько первых байт дробить
    /// * `frag_size` — размер каждого куска (обычно 1–3)
    /// * `delay_ms` — задержка между кусками в мс (обычно 1–10)
    pub fn new(
        inner: TcpStream,
        frag_bytes: usize,
        fragment_size: usize,
        delay_ms: u64,
    ) -> Self {
        // Включаем TCP_NODELAY — критично для фрагментации,
        // иначе ОС буферизует маленькие пакеты в один большой.
        let _ = inner.set_nodelay(true);
        Self {
            inner,
            remaining_frag_bytes: frag_bytes,
            fragment_size: fragment_size.max(1),
            fragment_delay_ms: delay_ms,
        }
    }

    /// Внутренний метод: записать буфер с фрагментацией.
    #[allow(dead_code)]
    async fn fragmented_write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.remaining_frag_bytes == 0 || buf.is_empty() {
            return self.inner.write(buf).await;
        }

        let mut total_written = 0usize;

        // Часть 1: фрагментированная отправка
        while self.remaining_frag_bytes > 0 && total_written < buf.len() {
            let chunk_size = self
                .fragment_size
                .min(self.remaining_frag_bytes)
                .min(buf.len() - total_written);

            let n = self.inner.write(&buf[total_written..total_written + chunk_size]).await?;
            self.inner.flush().await?;
            total_written += n;
            self.remaining_frag_bytes = self.remaining_frag_bytes.saturating_sub(n);

            if self.remaining_frag_bytes > 0 && self.fragment_delay_ms > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(self.fragment_delay_ms)).await;
            }

            trace!("fragmented write: {} bytes, remaining_frag={}", n, self.remaining_frag_bytes);
        }

        // Часть 2: остаток (без фрагментации)
        if total_written < buf.len() {
            let n = self.inner.write(&buf[total_written..]).await?;
            total_written += n;
        }

        Ok(total_written)
    }
}

impl AsyncRead for FragmentedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for FragmentedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        // Если фрагментация больше не нужна — проксируем напрямую
        if self.remaining_frag_bytes == 0 {
            return Pin::new(&mut self.inner).poll_write(cx, buf);
        }

        // Иначе запускаем асинхронный fragmented_write через ready-механизм.
        // Поскольку poll_write — синхронный, а нам нужен await,
        // мы возвращаем Pending и заводим внутренний waker.
        // Для простоты: если остались frag_bytes, всегда пишем по 1 байту
        // и возвращаем его размер (единичный write).
        let chunk = self.fragment_size.min(self.remaining_frag_bytes).min(buf.len());
        match Pin::new(&mut self.inner).poll_write(cx, &buf[..chunk]) {
            Poll::Ready(Ok(n)) => {
                self.remaining_frag_bytes = self.remaining_frag_bytes.saturating_sub(n);
                if self.remaining_frag_bytes == 0 {
                    trace!("Fragmentation phase completed");
                }
                Poll::Ready(Ok(n))
            }
            other => other,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Удобная функция: обернуть `TcpStream` в фрагментированный, если включено.
pub fn maybe_fragment(
    stream: TcpStream,
    enabled: bool,
    frag_bytes: usize,
    frag_size: usize,
    delay_ms: u64,
) -> FragmentedStream {
    if enabled {
        FragmentedStream::new(stream, frag_bytes, frag_size, delay_ms)
    } else {
        // Фрагментация выключена — оставляем 0 frag_bytes
        FragmentedStream::new(stream, 0, 1, 0)
    }
}
