//! Точка входа сервера Ghost.
//!
//! Поддерживает чтение конфигурации из TOML-файла.
//! Путь к конфигу: /etc/ghost/ghost-server.conf (по умолчанию)
//! Или через аргумент: ghost-server /path/to/config.toml

use anyhow::{Context, Result};
use ghost_common::ServerConfig;
use std::env;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<()> {
    let config = load_config()?;
    ghost_server::run(config).await
}

/// Загрузить конфигурацию: из файла или default.
fn load_config() -> Result<ServerConfig> {
    let config_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "/etc/ghost/ghost-server.conf".to_string());

    let path = Path::new(&config_path);

    if path.exists() {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config: {}", config_path))?;
        let config: ServerConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config: {}", config_path))?;
        eprintln!("[ghost-server] Loaded config from {}", config_path);
        Ok(config)
    } else {
        eprintln!("[ghost-server] Config not found at {}, using defaults", config_path);
        Ok(ServerConfig::default())
    }
}
