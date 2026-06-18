use anyhow::Result;
use quicftp_common::protocol::{Command, Response};
use quicftp_common::tls;
use quinn::{Connection, Endpoint};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::debug;

const STREAM_TIMEOUT: Duration = Duration::from_secs(30);

/// QuicFTP client
pub struct FtpClient {
    connection: Connection,
    endpoint: Endpoint,
}

impl FtpClient {
    /// Connect to a QuicFTP server
    pub async fn connect(addr: SocketAddr) -> Result<Self> {
        let tls_config = tls::create_client_tls_config()?;
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
        endpoint.set_default_client_config(quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(tls_config)?,
        )));

        let connection = endpoint.connect(addr, "localhost")?.await?;

        // Read welcome banner
        let (mut _send, mut recv) = connection.accept_bi().await?;
        let banner_bytes = recv.read_to_end(1024 * 1024).await?; // 1MB limit
        let banner = String::from_utf8_lossy(&banner_bytes);
        debug!("服务器横幅: {}", banner);

        Ok(Self {
            connection,
            endpoint,
        })
    }

    /// Send a command and receive the response
    pub async fn send_command(&self, command: &Command) -> Result<Response> {
        let (mut send, mut recv) = tokio::time::timeout(
            STREAM_TIMEOUT,
            self.connection.open_bi(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("打开命令流超时"))??;

        // Send command (use to_send_string to include actual password)
        let cmd_str = command.to_send_string();
        debug!("发送命令: {}", cmd_str);
        send.write_all(cmd_str.as_bytes()).await?;
        send.finish()?;

        // Read response
        let resp_bytes = tokio::time::timeout(
            STREAM_TIMEOUT,
            recv.read_to_end(1024 * 1024),
        )
        .await
        .map_err(|_| anyhow::anyhow!("读取响应超时"))??;
        let resp_str = String::from_utf8_lossy(&resp_bytes);
        debug!("收到响应: {}", resp_str);

        let response = Response::parse(&resp_str)?;
        Ok(response)
    }

    /// Download file from server
    pub async fn download_file(&self, remote_path: &str, local_path: &str) -> Result<u64> {
        // Send download command
        let (mut send, mut recv) = tokio::time::timeout(
            STREAM_TIMEOUT,
            self.connection.open_bi(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("打开下载命令流超时"))??;
        let cmd_str = format!("下载 {}", remote_path);
        send.write_all(cmd_str.as_bytes()).await?;
        send.finish()?;

        // Read response
        let resp_bytes = tokio::time::timeout(
            STREAM_TIMEOUT,
            recv.read_to_end(1024 * 1024),
        )
        .await
        .map_err(|_| anyhow::anyhow!("读取下载响应超时"))??;
        let resp_str = String::from_utf8_lossy(&resp_bytes);
        let response = Response::parse(&resp_str)?;

        drop(send);
        drop(recv);

        if response.code != 150 {
            return Err(anyhow::anyhow!("{}", response.message));
        }

        // Read file data from a new stream
        let (data_send, mut data_recv) = tokio::time::timeout(
            STREAM_TIMEOUT,
            self.connection.accept_bi(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("接受文件数据流超时"))??;

        // Read file size (8 bytes)
        let mut size_buf = [0u8; 8];
        tokio::time::timeout(STREAM_TIMEOUT, data_recv.read_exact(&mut size_buf))
            .await
            .map_err(|_| anyhow::anyhow!("读取文件大小超时"))??;
        let file_size = u64::from_be_bytes(size_buf);

        // Read file data
        let mut data = vec![0u8; file_size as usize];
        tokio::time::timeout(STREAM_TIMEOUT, data_recv.read_exact(&mut data))
            .await
            .map_err(|_| anyhow::anyhow!("读取文件数据超时"))??;

        drop(data_send);
        drop(data_recv);

        // Write to local file
        tokio::fs::write(local_path, &data).await?;

        debug!("文件已下载: {} ({} 字节)", local_path, file_size);
        Ok(file_size)
    }

    /// Upload file to server
    pub async fn upload_file(&self, local_path: &str, remote_name: &str) -> Result<u64> {
        // Read local file
        let data = tokio::fs::read(local_path).await?;
        let file_size = data.len() as u64;

        // Send upload command
        let (mut send, mut recv) = tokio::time::timeout(
            STREAM_TIMEOUT,
            self.connection.open_bi(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("打开上传命令流超时"))??;
        let cmd_str = format!("上传 {} {}", remote_name, file_size);
        send.write_all(cmd_str.as_bytes()).await?;
        send.finish()?;

        // Read response
        let resp_bytes = tokio::time::timeout(
            STREAM_TIMEOUT,
            recv.read_to_end(1024 * 1024),
        )
        .await
        .map_err(|_| anyhow::anyhow!("读取上传响应超时"))??;
        let resp_str = String::from_utf8_lossy(&resp_bytes);
        let response = Response::parse(&resp_str)?;

        drop(send);
        drop(recv);

        if response.code != 150 {
            return Err(anyhow::anyhow!("{}", response.message));
        }

        // Send file data on a new stream
        let (mut data_send, data_recv) = tokio::time::timeout(
            STREAM_TIMEOUT,
            self.connection.open_bi(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("打开文件数据流超时"))??;

        // Send file size (8 bytes)
        data_send.write_all(&file_size.to_be_bytes()).await?;

        // Send file data
        data_send.write_all(&data).await?;
        data_send.finish()?;

        drop(data_send);
        drop(data_recv);

        // Read success response from server
        let (resp_send, mut resp_recv) = tokio::time::timeout(
            STREAM_TIMEOUT,
            self.connection.accept_bi(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("接受上传完成响应流超时"))??;
        let resp_bytes = tokio::time::timeout(
            STREAM_TIMEOUT,
            resp_recv.read_to_end(1024 * 1024),
        )
        .await
        .map_err(|_| anyhow::anyhow!("读取上传完成响应超时"))??;
        let resp_str = String::from_utf8_lossy(&resp_bytes);
        let response = Response::parse(&resp_str)?;

        drop(resp_send);
        drop(resp_recv);

        debug!("文件已上传: {} ({} 字节)", remote_name, file_size);
        Ok(file_size)
    }

    /// Close the connection
    pub async fn close(&self) -> Result<()> {
        self.connection.close(0u32.into(), b"Client closing");
        self.endpoint.wait_idle().await;
        Ok(())
    }
}

use std::sync::Arc;
