//! Работа с файловым дескриптором TUN-интерфейса (Android VpnService).

use anyhow::{Context, Result};
use std::os::fd::RawFd;
use tokio::io::unix::AsyncFd;

/// Неблокирующая обёртка над TUN fd.
pub struct TunDevice {
    fd: RawFd,
    async_fd: AsyncFd<RawFd>,
}

impl TunDevice {
    pub fn new(fd: RawFd) -> Result<Self> {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
        if flags < 0 {
            anyhow::bail!("fcntl(F_GETFL) failed");
        }
        let res = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
        if res < 0 {
            anyhow::bail!("fcntl(F_SETFL, O_NONBLOCK) failed");
        }
        let async_fd = AsyncFd::new(fd).context("AsyncFd creation failed")?;
        Ok(Self { fd, async_fd })
    }

    pub async fn read(&self, buf: &mut [u8]) -> Result<usize> {
        loop {
            let mut guard = self.async_fd.readable().await?;
            match guard.try_io(|_| {
                let n = unsafe {
                    libc::read(
                        self.fd,
                        buf.as_mut_ptr() as *mut libc::c_void,
                        buf.len(),
                    )
                };
                if n < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            }) {
                Ok(Ok(n)) => return Ok(n),
                Ok(Err(e)) => return Err(e.into()),
                Err(_would_block) => continue,
            }
        }
    }

    pub async fn write(&self, buf: &[u8]) -> Result<usize> {
        loop {
            let mut guard = self.async_fd.writable().await?;
            match guard.try_io(|_| {
                let n = unsafe {
                    libc::write(
                        self.fd,
                        buf.as_ptr() as *const libc::c_void,
                        buf.len(),
                    )
                };
                if n < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            }) {
                Ok(Ok(n)) => return Ok(n),
                Ok(Err(e)) => return Err(e.into()),
                Err(_would_block) => continue,
            }
        }
    }
}
