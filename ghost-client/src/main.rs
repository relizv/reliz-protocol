//! Точка входа клиента Reliz Protocol.
//!
//! Запускает встроенный Web UI для управления прокси.
//! По умолчанию UI доступен на http://127.0.0.1:3000

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Инициализация логирования
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ghost_client=info".into()),
        )
        .init();

    let listen = "127.0.0.1:3000";

    println!();
    println!("  ╔══════════════════════════════════════╗");
    println!("  ║       Reliz Protocol Client           ║");
    println!("  ╠══════════════════════════════════════╣");
    println!("  ║  Web UI: http://{}       ║", listen);
    println!("  ║  SOCKS5: 127.0.0.1:10808  (after connect) ║");
    println!("  ╚══════════════════════════════════════╝");
    println!();

    ghost_client::web::serve(listen).await
}
