//! ghost-tun: Userspace TUN → SOCKS5 ретранслятор (tun2socks) на чистом Rust.
//!
//! Архитектура:
//!   1. TUN fd (от Android VpnService) читается через `AsyncFd`
//!   2. IP-пакеты парсятся через `etherparse`
//!   3. TCP SYN → SOCKS5 CONNECT к локальному прокси (127.0.0.1:10808)
//!   4. TCP payload ↔ SOCKS5 stream (userspace TCP NAT)
//!   5. UDP → SOCKS5 UDP ASSOCIATE (TODO)

pub mod device;
pub mod socks5;
pub mod tcp_session;

use crate::device::TunDevice;
use crate::tcp_session::{run_tcp_session, FlowKey, TcpNatTable};
use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info, trace, warn};

/// Основной процессор tun2socks.
pub struct TunProcessor {
    device: Arc<TunDevice>,
    socks5_addr: SocketAddr,
    nat: Arc<Mutex<TcpNatTable>>,
}

impl TunProcessor {
    pub fn new(tun_fd: i32, socks5_addr: SocketAddr) -> Result<Self> {
        let device = Arc::new(TunDevice::new(tun_fd)?);
        Ok(Self {
            device,
            socks5_addr,
            nat: Arc::new(Mutex::new(TcpNatTable::new())),
        })
    }

    /// Запустить главный цикл обработки.
    pub async fn run(self) -> Result<()> {
        info!("Starting ghost-tun (tun2socks) -> SOCKS5 {}", self.socks5_addr);

        let (tun_tx, mut tun_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        // Task: пишем пакеты в TUN из канала
        let device = self.device.clone();
        let tun_write_task = tokio::spawn(async move {
            while let Some(packet) = tun_rx.recv().await {
                if let Err(e) = device.write(&packet).await {
                    warn!("TUN write error: {}", e);
                }
            }
        });

        // Main loop: читаем из TUN
        let mut buf = vec![0u8; 65535];
        loop {
            let n = match self.device.read(&mut buf).await {
                Ok(0) => {
                    info!("TUN fd closed");
                    break;
                }
                Ok(n) => n,
                Err(e) => {
                    error!("TUN read error: {}", e);
                    continue;
                }
            };

            if let Err(e) = self.process_packet(&buf[..n], &tun_tx).await {
                trace!("Packet processing error: {}", e);
            }
        }

        let _ = tun_write_task.await;
        Ok(())
    }

    async fn process_packet(
        &self,
        data: &[u8],
        tun_tx: &mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<()> {
        if data.len() < 20 {
            return Ok(());
        }
        let version = (data[0] >> 4) & 0x0f;
        if version != 4 {
            return Ok(()); // TODO: IPv6
        }

        let ip = etherparse::Ipv4HeaderSlice::from_slice(data)?;
        let proto = ip.protocol();
        let payload = &data[ip.slice().len()..];

        match proto {
            etherparse::IpNumber::TCP => self.handle_tcp(ip, payload, tun_tx).await,
            etherparse::IpNumber::UDP => self.handle_udp(ip, payload, tun_tx).await,
            _ => Ok(()),
        }
    }

    async fn handle_tcp(
        &self,
        ip: etherparse::Ipv4HeaderSlice<'_>,
        payload: &[u8],
        tun_tx: &mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<()> {
        let tcp = etherparse::TcpHeaderSlice::from_slice(payload)?;
        let tcp_payload = &payload[tcp.slice().len()..];

        let src_ip = ip.source();
        let dst_ip = ip.destination();
        let src_port = tcp.source_port();
        let dst_port = tcp.destination_port();

        let key = FlowKey::new(src_ip, src_port, dst_ip, dst_port);
        let syn = tcp.syn();
        let ack = tcp.ack();
        let fin = tcp.fin();
        let rst = tcp.rst();

        trace!(
            "TCP {}:{} -> {}:{} syn={} ack={} fin={} rst={} payload={}",
            pretty_ip(src_ip),
            src_port,
            pretty_ip(dst_ip),
            dst_port,
            syn,
            ack,
            fin,
            rst,
            tcp_payload.len()
        );

        if rst {
            self.nat.lock().await.remove(&key);
            return Ok(());
        }

        if syn && !ack {
            let mut nat = self.nat.lock().await;
            if nat.get(&key).is_some() {
                return Ok(());
            }

            let (data_tx, data_rx) = mpsc::unbounded_channel::<Vec<u8>>();
            nat.insert(key, data_tx);
            drop(nat);

            let target = SocketAddr::from((std::net::Ipv4Addr::from(dst_ip), dst_port));
            let socks5_addr = self.socks5_addr;
            let tun_tx = tun_tx.clone();
            let client_seq = tcp.sequence_number();

            tokio::spawn(async move {
                run_tcp_session(key, client_seq, tun_tx, data_rx, socks5_addr, target).await;
            });

            return Ok(());
        }

        let nat = self.nat.lock().await;
        if let Some(tx) = nat.get(&key) {
            if fin {
                // Клиент закрывает соединение — сигнал + удаляем из NAT
                let _ = tx.send(vec![]);
                drop(nat);
                self.nat.lock().await.remove(&key);
            } else if !tcp_payload.is_empty() {
                let _ = tx.send(tcp_payload.to_vec());
            }
        }

        Ok(())
    }

    async fn handle_udp(
        &self,
        _ip: etherparse::Ipv4HeaderSlice<'_>,
        _payload: &[u8],
        _tun_tx: &mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<()> {
        // TODO: UDP ASSOCIATE через SOCKS5
        Ok(())
    }
}

fn pretty_ip(ip: [u8; 4]) -> String {
    format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
}
