use anyhow::Result;
use quicftp_common::protocol::{Command, Response, ResponseCode};
use quinn::Connection;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info, warn};

use crate::handler;
use crate::server::ServerState;

const STREAM_TIMEOUT: Duration = Duration::from_secs(30);
const DATA_TIMEOUT: Duration = Duration::from_secs(120);

/// A client session
pub struct Session {
    connection: Connection,
    state: Arc<ServerState>,
    username: Option<String>,
    pending_username: Option<String>,
    home_dir: Option<PathBuf>,
    current_dir: PathBuf,
}

impl Session {
    pub fn new(connection: Connection, state: Arc<ServerState>) -> Self {
        Self {
            connection,
            state,
            username: None,
            pending_username: None,
            home_dir: None,
            current_dir: PathBuf::new(),
        }
    }

    /// Run the session, processing commands from the client
    pub async fn run(mut self) -> Result<()> {
        // Send welcome banner
        let banner = Response::new(ResponseCode::ServiceReady, "欢迎使用 QuicFTP 服务器");
        self.send_response(&banner).await?;

        // Main command loop
        loop {
            let (mut send, mut recv) = match self.connection.accept_bi().await {
                Ok(streams) => streams,
                Err(quinn::ConnectionError::ApplicationClosed(_)) => {
                    debug!("客户端关闭连接");
                    return Ok(());
                }
                Err(e) => {
                    return Err(e.into());
                }
            };

            // Read command from the stream
            let cmd_bytes = recv.read_to_end(1024 * 1024).await?; // 1MB limit
            let cmd_str = String::from_utf8_lossy(&cmd_bytes);

            debug!("收到命令: {}", cmd_str);

            // Parse and handle command
            match Command::parse(&cmd_str) {
                Ok(cmd) => {
                    if matches!(cmd, Command::Quit) {
                        let resp = Response::new(
                            ResponseCode::ConnectionClosing,
                            "再见",
                        );
                        let _ = send.write_all(resp.format().as_bytes()).await;
                        let _ = send.finish();
                        return Ok(());
                    }

                    // Handle file transfer commands specially
                    match &cmd {
                        Command::Get(path) => {
                            // Download: send response on command stream, then file data on new stream
                            let resp = handler::handle_command(&cmd, &mut self).await;
                            if resp.code == ResponseCode::DataConnectionOpening as u16 {
                                // Send response on command stream
                                let _ = send.write_all(resp.format().as_bytes()).await;
                                let _ = send.finish();
                                // Explicitly drop command stream to send FIN
                                drop(send);
                                drop(recv);

                                // Send file data on a new stream
                                if let Some(file_path) = self.full_path(path) {
                                    self.send_file_data(&file_path).await?;
                                }
                                continue;
                            } else {
                                // Error response
                                let _ = send.write_all(resp.format().as_bytes()).await;
                                let _ = send.finish();
                                continue;
                            }
                        }
                        Command::Put(remote_name, _expected_size) => {
                            // Upload: send response on command stream, then receive file data on new stream
                            let resp = handler::handle_command(&cmd, &mut self).await;
                            if resp.code == ResponseCode::DataConnectionOpening as u16 {
                                // Send response on command stream
                                let _ = send.write_all(resp.format().as_bytes()).await;
                                let _ = send.finish();
                                // Explicitly drop command stream to send FIN
                                drop(send);
                                drop(recv);

                                // Receive file data on a new stream
                                if let Some(full_path) = self.full_path(remote_name) {
                                    let file_size = self.receive_file_data(&full_path).await?;
                                    debug!("文件已保存: {:?} ({} 字节)", full_path, file_size);

                                    // Send success response on a new stream
                                    let success_resp = Response::new(
                                        ResponseCode::TransferComplete,
                                        format!("上传完成: {} ({} 字节)", remote_name, file_size),
                                    );
                                    self.send_response(&success_resp).await?;
                                }
                                continue;
                            } else {
                                // Error response
                                let _ = send.write_all(resp.format().as_bytes()).await;
                                let _ = send.finish();
                                continue;
                            }
                        }
                        _ => {
                            // Regular command
                            let response = handler::handle_command(&cmd, &mut self).await;
                            let response_str = response.format();
                            debug!("发送响应: {:?}", response_str);
                            let _ = send.write_all(response_str.as_bytes()).await;
                            let _ = send.finish();
                        }
                    }
                }
                Err(e) => {
                    let response = Response::new(ResponseCode::SyntaxError, format!("无效命令: {}", e));
                    let _ = send.write_all(response.format().as_bytes()).await;
                    let _ = send.finish();
                }
            };
        }
    }

    /// Send file data to client on a new stream
    async fn send_file_data(&self, file_path: &PathBuf) -> Result<()> {
        let (mut send, _recv) = tokio::time::timeout(
            STREAM_TIMEOUT,
            self.connection.open_bi(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("打开文件发送流超时"))??;

        let data = tokio::fs::read(file_path).await.unwrap_or_default();
        let total_size = data.len() as u64;

        // Send file size first (8 bytes)
        send.write_all(&total_size.to_be_bytes()).await?;

        // Send file data
        send.write_all(&data).await?;
        send.finish()?;

        debug!("文件已发送: {:?} ({} 字节)", file_path, total_size);
        Ok(())
    }

    /// Receive file data from client on a new stream
    async fn receive_file_data(&self, file_path: &PathBuf) -> Result<u64> {
        let (_send, mut recv) = tokio::time::timeout(
            STREAM_TIMEOUT,
            self.connection.accept_bi(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("接受文件接收流超时"))??;

        // Read file size (8 bytes)
        let mut size_buf = [0u8; 8];
        tokio::time::timeout(DATA_TIMEOUT, recv.read_exact(&mut size_buf))
            .await
            .map_err(|_| anyhow::anyhow!("读取文件大小超时"))??;
        let file_size = u64::from_be_bytes(size_buf);

        // Read file data
        let mut data = vec![0u8; file_size as usize];
        tokio::time::timeout(DATA_TIMEOUT, recv.read_exact(&mut data))
            .await
            .map_err(|_| anyhow::anyhow!("读取文件数据超时"))??;

        // Create parent directory if needed
        if let Some(parent) = file_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }

        // Write file
        tokio::fs::write(file_path, &data).await?;

        debug!("文件已接收: {:?} ({} 字节)", file_path, file_size);
        Ok(file_size)
    }

    /// Send a response on the control stream
    async fn send_response(&self, response: &Response) -> Result<()> {
        let (mut send, _recv) = tokio::time::timeout(
            STREAM_TIMEOUT,
            self.connection.open_bi(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("打开响应流超时"))??;
        send.write_all(response.format().as_bytes()).await?;
        send.finish()?;
        Ok(())
    }

    /// Check if the user is authenticated
    pub fn is_authenticated(&self) -> bool {
        self.username.is_some()
    }

    /// Get the username
    pub fn username(&self) -> Option<&str> {
        self.username.as_deref()
    }

    /// Set the authenticated user
    pub fn set_user(&mut self, username: String, home_dir: PathBuf) {
        self.username = Some(username);
        self.pending_username = None;
        self.current_dir = PathBuf::from("/");
        self.home_dir = Some(home_dir);
    }

    /// Get the pending username (from USER command)
    pub fn pending_username(&self) -> Option<&str> {
        self.pending_username.as_deref()
    }

    /// Set the pending username (from USER command)
    pub fn set_pending_username(&mut self, username: String) {
        self.pending_username = Some(username);
    }

    /// Get the home directory
    pub fn home_dir(&self) -> Option<&PathBuf> {
        self.home_dir.as_ref()
    }

    /// Get the current directory (relative)
    pub fn current_dir(&self) -> &PathBuf {
        &self.current_dir
    }

    /// Set the current directory (relative)
    pub fn set_current_dir(&mut self, dir: PathBuf) {
        self.current_dir = dir;
    }

    /// Get the full path on disk for a given relative path
    pub fn full_path(&self, relative_path: &str) -> Option<PathBuf> {
        let home = self.home_dir.as_ref()?;
        let relative = if relative_path.starts_with('/') {
            PathBuf::from(relative_path)
        } else {
            self.current_dir.join(relative_path)
        };

        // Normalize the path (resolve ..)
        let mut components = Vec::new();
        for component in relative.components() {
            match component {
                std::path::Component::ParentDir => {
                    components.pop();
                }
                std::path::Component::Normal(c) => {
                    components.push(c);
                }
                _ => {}
            }
        }

        let normalized: PathBuf = components.iter().collect();
        Some(home.join(normalized.strip_prefix("/").unwrap_or(&normalized)))
    }

    /// Get the server state
    pub fn state(&self) -> &Arc<ServerState> {
        &self.state
    }

    /// Get the QUIC connection
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}
