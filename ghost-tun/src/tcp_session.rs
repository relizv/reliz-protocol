//! TCP NAT Session — userspace TCP endpoint для tun2socks.
//!
//! Когда приложение шлёт SYN в TUN:
//!   1. Мы открываем SOCKS5 CONNECT к цели
//!   2. Отправляем SYN-ACK в TUN (эмулируем TCP-handshake с приложением)
//!   3. Далее пересылаем payload между TUN ↔ SOCKS5
//!
//! Sequence numbers используются **относительные** (для простоты начинаем с 0).
//! Android-ядро нормально воспринимает любые начальные seq/ack.

use crate::socks5::socks5_connect;
use anyhow::Result;
use etherparse::Ipv4Header;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tracing::{debug, error, trace, warn};

/// Ключ NAT: (src_ip, src_port, dst_ip, dst_port) в сетевом порядке.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FlowKey {
    pub src_ip: u32,
    pub src_port: u16,
    pub dst_ip: u32,
    pub dst_port: u16,
}

impl FlowKey {
    pub fn new(src_ip: [u8; 4], src_port: u16, dst_ip: [u8; 4], dst_port: u16) -> Self {
        Self {
            src_ip: u32::from_be_bytes(src_ip),
            src_port,
            dst_ip: u32::from_be_bytes(dst_ip),
            dst_port,
        }
    }

    /// Обратный ключ (для пакетов от "сервера" к клиенту).
    pub fn reverse(&self) -> Self {
        Self {
            src_ip: self.dst_ip,
            src_port: self.dst_port,
            dst_ip: self.src_ip,
            dst_port: self.src_port,
        }
    }
}

/// Таблица активных TCP-сессий.
pub struct TcpNatTable {
    sessions: HashMap<FlowKey, mpsc::UnboundedSender<Vec<u8>>>,
}

impl TcpNatTable {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: FlowKey, tx: mpsc::UnboundedSender<Vec<u8>>) {
        self.sessions.insert(key, tx);
    }

    pub fn get(&self, key: &FlowKey) -> Option<&mpsc::UnboundedSender<Vec<u8>>> {
        self.sessions.get(key)
    }

    pub fn remove(&mut self, key: &FlowKey) {
        self.sessions.remove(key);
    }
}

impl Default for TcpNatTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Запустить TCP-сессию: подключается к SOCKS5 и пересылает данные.
///
/// * `key` — идентификатор потока
/// * `client_seq` — seq из SYN клиента (используем как базу для ack)
/// * `tun_tx` — канал для отправки пакетов обратно в TUN
/// * `socks5_proxy` — адрес локального SOCKS5 (127.0.0.1:10808)
/// * `target` — реальный целевой адрес
pub async fn run_tcp_session(
    key: FlowKey,
    client_initial_seq: u32,
    tun_tx: mpsc::UnboundedSender<Vec<u8>>,
    data_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    socks5_proxy: SocketAddr,
    target: SocketAddr,
) {
    // 1. Подключаемся к SOCKS5
    let socks = match socks5_connect(socks5_proxy, target).await {
        Ok(s) => s,
        Err(e) => {
            warn!("SOCKS5 CONNECT failed for {:?}: {}", key, e);
            // Отправляем RST
            let _ = send_tcp_packet(
                &tun_tx,
                key.dst_ip,
                key.src_ip,
                key.dst_port,
                key.src_port,
                0,
                client_initial_seq.wrapping_add(1),
                true,  // rst
                false, // syn
                true,  // ack
                false, // fin
                false, // psh
                &[],
            );
            return;
        }
    };

    debug!("TCP session established via SOCKS5: {:?} -> {}", key, target);

    // 2. Отправляем SYN-ACK
    let server_seq: u32 = 0; // начинаем с 0 (относительно)
    let client_ack = client_initial_seq.wrapping_add(1);

    if let Err(e) = send_tcp_packet(
        &tun_tx,
        key.dst_ip,
        key.src_ip,
        key.dst_port,
        key.src_port,
        server_seq,
        client_ack,
        false, // rst
        true,  // syn
        true,  // ack
        false, // fin
        false, // psh
        &[],
    ) {
        warn!("Failed to send SYN-ACK: {}", e);
        return;
    }
    trace!("Sent SYN-ACK to client, seq={}, ack={}", server_seq, client_ack);

    // 3. Сплитим SOCKS5 поток
    let (mut socks_rd, mut socks_wr) = socks.into_split();

    let mut next_server_seq = server_seq.wrapping_add(1); // после SYN-ACK
    let next_client_ack = client_ack; // что мы подтвердили от клиента

    let mut data_rx = data_rx;

    // Task A: читаем из SOCKS5 → заворачиваем в IP/TCP → TUN
    let tun_tx_clone = tun_tx.clone();
    let key_clone = key;
    let socks_to_tun = tokio::spawn(async move {
        let mut buf = vec![0u8; 8192];
        loop {
            match socks_rd.read(&mut buf).await {
                Ok(0) => {
                    debug!("SOCKS5 EOF for {:?}", key_clone);
                    break;
                }
                Ok(n) => {
                    trace!("SOCKS5 -> TUN: {} bytes for {:?}", n, key_clone);
                    if let Err(e) = send_tcp_packet(
                        &tun_tx_clone,
                        key_clone.dst_ip,
                        key_clone.src_ip,
                        key_clone.dst_port,
                        key_clone.src_port,
                        next_server_seq,
                        next_client_ack,
                        false, false, true, false, true, // ACK + PSH
                        &buf[..n],
                    ) {
                        error!("Failed to send data packet to TUN: {}", e);
                        break;
                    }
                    next_server_seq = next_server_seq.wrapping_add(n as u32);
                }
                Err(e) => {
                    warn!("SOCKS5 read error for {:?}: {}", key_clone, e);
                    break;
                }
            }
        }

        // SOCKS5 закрылся — шлём FIN-ACK
        let _ = send_tcp_packet(
            &tun_tx_clone,
            key_clone.dst_ip,
            key_clone.src_ip,
            key_clone.dst_port,
            key_clone.src_port,
            next_server_seq,
            next_client_ack,
            false, false, true, true, false, // ACK + FIN
            &[],
        );
    });

    // Task B: читаем из канала (данные от клиента из TUN) → SOCKS5
    let key_clone2 = key;
    let data_to_socks = tokio::spawn(async move {
        while let Some(payload) = data_rx.recv().await {
            if payload.is_empty() {
                continue;
            }
            if let Err(e) = socks_wr.write_all(&payload).await {
                warn!("SOCKS5 write error for {:?}: {}", key_clone2, e);
                break;
            }
            if let Err(e) = socks_wr.flush().await {
                warn!("SOCKS5 flush error for {:?}: {}", key_clone2, e);
                break;
            }
        }
        let _ = socks_wr.shutdown().await;
    });

    // Ждём завершения любой из сторон
    tokio::select! {
        _ = socks_to_tun => {}
        _ = data_to_socks => {}
    }

    debug!("TCP session closed: {:?}", key);
}

/// Отправить IP/TCP пакет в TUN (через канал).
fn send_tcp_packet(
    tun_tx: &mpsc::UnboundedSender<Vec<u8>>,
    src_ip_u32: u32,
    dst_ip_u32: u32,
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    rst: bool,
    syn: bool,
    ack_flag: bool,
    fin: bool,
    psh: bool,
    payload: &[u8],
) -> Result<()> {
    let src_ip = src_ip_u32.to_be_bytes();
    let dst_ip = dst_ip_u32.to_be_bytes();

    let mut tcp = etherparse::TcpHeader::new(src_port, dst_port, seq, 65535);
    tcp.acknowledgment_number = ack;
    tcp.rst = rst;
    tcp.syn = syn;
    tcp.ack = ack_flag;
    tcp.fin = fin;
    tcp.psh = psh;
    tcp.window_size = 65535;

    let ip_payload_len = tcp.header_len() as u16 + payload.len() as u16;
    let ip = Ipv4Header::new(
        ip_payload_len,
        64,
        etherparse::IpNumber::TCP,
        src_ip,
        dst_ip,
    )?;

    tcp.checksum = tcp.calc_checksum_ipv4(&ip, payload)?;

    let mut packet = Vec::with_capacity(ip.header_len() + tcp.header_len() as usize + payload.len());
    ip.write(&mut packet)?;
    tcp.write(&mut packet)?;
    packet.extend_from_slice(payload);

    tun_tx.send(packet)?;
    Ok(())
}

/// Отправить простой ACK (без payload).
pub fn send_ack(
    tun_tx: &mpsc::UnboundedSender<Vec<u8>>,
    key: &FlowKey,
    seq: u32,
    ack: u32,
) -> Result<()> {
    send_tcp_packet(
        tun_tx,
        key.dst_ip,
        key.src_ip,
        key.dst_port,
        key.src_port,
        seq,
        ack,
        false, false, true, false, false,
        &[],
    )
}
