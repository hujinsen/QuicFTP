mod client;
mod shell;

use anyhow::Result;
use clap::Parser;
use std::net::SocketAddr;
use tracing::info;

/// QuicFTP Client - A QUIC-based FTP client
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Server host
    #[arg(short = 'H', long, default_value = "localhost")]
    host: String,
    /// Server port
    #[arg(short, long, default_value = "5000")]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;

    println!("正在连接 {}:{}...", args.host, args.port);

    let mut client = client::FtpClient::connect(addr).await?;

    println!("已连接！输入 '帮助' 查看可用命令。");

    // Run interactive shell
    shell::run(&mut client).await?;

    Ok(())
}
