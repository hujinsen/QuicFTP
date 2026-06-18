use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server bind address
    #[serde(default = "default_host")]
    pub host: String,
    /// Server port
    #[serde(default = "default_port")]
    pub port: u16,
    /// Path to TLS certificate file (PEM)
    #[serde(default = "default_cert_path")]
    pub cert_path: PathBuf,
    /// Path to TLS private key file (PEM)
    #[serde(default = "default_key_path")]
    pub key_path: PathBuf,
    /// Path to users file
    #[serde(default = "default_users_path")]
    pub users_path: PathBuf,
    /// Root directory for FTP files
    #[serde(default = "default_root_dir")]
    pub root_dir: PathBuf,
    /// Auto-generate self-signed certificate if not found
    #[serde(default = "default_true")]
    pub auto_generate_cert: bool,
    /// Maximum concurrent connections
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    5000
}

fn default_cert_path() -> PathBuf {
    PathBuf::from("config/cert.pem")
}

fn default_key_path() -> PathBuf {
    PathBuf::from("config/key.pem")
}

fn default_users_path() -> PathBuf {
    PathBuf::from("config/users.toml")
}

fn default_root_dir() -> PathBuf {
    PathBuf::from("./ftp_root")
}

fn default_true() -> bool {
    true
}

fn default_max_connections() -> usize {
    100
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            cert_path: default_cert_path(),
            key_path: default_key_path(),
            users_path: default_users_path(),
            root_dir: default_root_dir(),
            auto_generate_cert: default_true(),
            max_connections: default_max_connections(),
        }
    }
}

impl ServerConfig {
    /// Load config from a TOML file
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: ServerConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save config to a TOML file
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }
}
