//! Точка входа клиента Ghost.

use anyhow::Result;
use ghost_common::ClientConfig;

#[tokio::main]
async fn main() -> Result<()> {
    let config = ClientConfig::default();
    ghost_client::run(config).await
}
