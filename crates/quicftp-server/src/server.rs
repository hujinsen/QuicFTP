use anyhow::Result;
use quicftp_common::config::ServerConfig;
use quicftp_common::tls;
use quinn::{Endpoint, ServerConfig as QuinnServerConfig};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{error, info, warn};

use crate::session::Session;

/// Server state shared across connections
pub struct ServerState {
    pub config: ServerConfig,
    pub users: Arc<tokio::sync::RwLock<quicftp_common::user::UsersFile>>,
    pub connections: Semaphore,
}

pub async fn run(config: ServerConfig) -> Result<()> {
    // Generate certificate if needed
    if config.auto_generate_cert
        && (!config.cert_path.exists() || !config.key_path.exists())
    {
        tls::generate_self_signed_cert(&config.cert_path, &config.key_path)?;
    }

    // Load TLS config
    let tls_config = tls::create_server_tls_config(&config.cert_path, &config.key_path)?;
    let server_config = QuinnServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(tls_config)?,
    ));

    // Load users
    let users = quicftp_common::user::UsersFile::load(&config.users_path)?;

    let state = Arc::new(ServerState {
        config: config.clone(),
        users: Arc::new(tokio::sync::RwLock::new(users)),
        connections: Semaphore::new(config.max_connections),
    });

    // Bind address
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    let endpoint = Endpoint::server(server_config, addr)?;

    info!("QuicFTP server listening on {}", addr);
    info!("Root directory: {:?}", config.root_dir);

    // Accept connections
    while let Some(connecting) = endpoint.accept().await {
        let state = state.clone();
        tokio::spawn(async move {
            match connecting.await {
                Ok(connection) => {
                    let remote = connection.remote_address();
                    info!("New connection from {}", remote);

                    // Acquire connection permit
                    let permit = state.connections.acquire().await.unwrap();

                    let session = Session::new(connection, state.clone());
                    if let Err(e) = session.run().await {
                        warn!("Session error for {}: {}", remote, e);
                    }

                    info!("Connection closed: {}", remote);
                    drop(permit);
                }
                Err(e) => {
                    error!("Connection failed: {}", e);
                }
            }
        });
    }

    Ok(())
}
