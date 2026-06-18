mod handler;
mod server;
mod session;

use anyhow::Result;
use clap::{Parser, Subcommand};
use quicftp_common::config::ServerConfig;
use quicftp_common::user::{Permission, UsersFile};
use std::path::PathBuf;
use tracing::info;

/// QuicFTP Server - A QUIC-based FTP server
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "config/server.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the FTP server
    Serve,
    /// User management
    User {
        #[command(subcommand)]
        action: UserAction,
    },
}

#[derive(Subcommand, Debug)]
enum UserAction {
    /// Add a new user
    Add {
        /// Username
        username: String,
        /// Password
        #[arg(short, long)]
        password: String,
        /// Home directory (relative to root_dir)
        #[arg(short = 'd', long)]
        home_dir: Option<String>,
        /// Permissions (read, write)
        #[arg(long, value_delimiter = ',', default_value = "read,write")]
        permissions: Vec<String>,
    },
    /// Remove a user
    Remove {
        /// Username to remove
        username: String,
    },
    /// List all users
    List,
    /// Change user password
    Password {
        /// Username
        username: String,
        /// New password
        #[arg(short, long)]
        password: String,
    },
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

    // Load or create config
    let config = if args.config.exists() {
        info!("Loading config from: {:?}", args.config);
        ServerConfig::load(&args.config)?
    } else {
        info!("Config not found, creating default config at: {:?}", args.config);
        let config = ServerConfig::default();
        config.save(&args.config)?;
        config
    };

    match args.command {
        Some(Commands::User { action }) => {
            handle_user_command(action, &config)?;
        }
        Some(Commands::Serve) | None => {
            // Default action: start server
            // Create root directory if not exists
            std::fs::create_dir_all(&config.root_dir)?;

            // Create users file if not exists
            if !config.users_path.exists() {
                let users = UsersFile { users: Vec::new() };
                users.save(&config.users_path)?;
                info!("Created empty users file at: {:?}", config.users_path);
            }

            // Start server
            server::run(config).await?;
        }
    }

    Ok(())
}

fn handle_user_command(action: UserAction, config: &ServerConfig) -> Result<()> {
    let mut users = UsersFile::load(&config.users_path)?;

    match action {
        UserAction::Add {
            username,
            password,
            home_dir,
            permissions,
        } => {
            let home = home_dir.unwrap_or_else(|| format!("./ftp_root/{}", username));
            let perms: Vec<Permission> = permissions
                .iter()
                .map(|p| match p.as_str() {
                    "write" => Permission::Write,
                    _ => Permission::Read,
                })
                .collect();

            users.add_user(username.clone(), &password, home.clone(), perms)?;
            users.save(&config.users_path)?;
            println!("User '{}' added successfully", username);
            println!("  Home directory: {}", home);
        }
        UserAction::Remove { username } => {
            if users.remove_user(&username) {
                users.save(&config.users_path)?;
                println!("User '{}' removed successfully", username);
            } else {
                println!("User '{}' not found", username);
            }
        }
        UserAction::List => {
            if users.users.is_empty() {
                println!("No users configured");
            } else {
                println!("Configured users:");
                for user in &users.users {
                    let perms: Vec<String> = user
                        .permissions
                        .iter()
                        .map(|p| match p {
                            Permission::Read => "read".to_string(),
                            Permission::Write => "write".to_string(),
                        })
                        .collect();
                    println!("  {} (home: {}, permissions: {})", user.username, user.home_dir, perms.join(", "));
                }
            }
        }
        UserAction::Password { username, password } => {
            if let Some(user) = users.users.iter_mut().find(|u| u.username == username) {
                user.password_hash = quicftp_common::user::hash_password(&password)?;
                users.save(&config.users_path)?;
                println!("Password for '{}' updated successfully", username);
            } else {
                println!("User '{}' not found", username);
            }
        }
    }

    Ok(())
}
