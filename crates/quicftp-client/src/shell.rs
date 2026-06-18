use anyhow::Result;
use quicftp_common::protocol::Command;
use std::io::{self, Write};
use std::path::Path;

use crate::client::FtpClient;

/// Run the interactive shell
pub async fn run(client: &mut FtpClient) -> Result<()> {
    loop {
        print!("ftp> ");
        io::stdout().flush()?;

        let mut input = String::new();
        if io::stdin().read_line(&mut input)? == 0 {
            break; // EOF
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        // Handle local commands
        match input {
            "帮助" | "help" | "?" => {
                print_help();
                continue;
            }
            "退出" | "quit" | "bye" | "exit" => {
                match client.send_command(&Command::Quit).await {
                    Ok(resp) => println!("{}", resp.message),
                    Err(_) => println!("已断开连接"),
                }
                break;
            }
            _ => {}
        }

        // Parse input
        let (cmd, args) = match input.find(' ') {
            Some(pos) => (&input[..pos], input[pos + 1..].trim()),
            None => (input, ""),
        };

        // Handle file transfer commands
        match cmd {
            "上传" | "put" => {
                if args.is_empty() {
                    eprintln!("用法: 上传 <本地文件路径>");
                    continue;
                }
                // Remove surrounding quotes if present
                let local_path = if (args.starts_with('"') && args.ends_with('"'))
                    || (args.starts_with('\'') && args.ends_with('\''))
                {
                    &args[1..args.len() - 1]
                } else {
                    args
                };
                if !Path::new(local_path).exists() {
                    eprintln!("错误: 本地文件不存在: {}", local_path);
                    continue;
                }

                let remote_name = Path::new(local_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| local_path.to_string());

                match client.upload_file(local_path, &remote_name).await {
                    Ok(size) => println!("上传完成: {} ({} 字节)", remote_name, size),
                    Err(e) => eprintln!("上传失败: {}", e),
                }
            }
            "下载" | "get" => {
                if args.is_empty() {
                    eprintln!("用法: 下载 <远程文件名>");
                    continue;
                }
                let remote_path = args;
                let local_name = Path::new(remote_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| remote_path.to_string());

                match client.download_file(remote_path, &local_name).await {
                    Ok(size) => println!("下载完成: {} ({} 字节)", local_name, size),
                    Err(e) => eprintln!("下载失败: {}", e),
                }
            }
            _ => {
                // Regular command - parse and send
                let cmd = match Command::parse(input) {
                    Ok(cmd) => cmd,
                    Err(e) => {
                        eprintln!("错误: {}", e);
                        continue;
                    }
                };

                let resp = client.send_command(&cmd).await?;
                println!("{}", resp.message);
            }
        }
    }

    // Close connection
    client.close().await?;

    Ok(())
}

fn print_help() {
    println!("可用命令:");
    println!("  用户 <用户名>        - 登录用户名");
    println!("  密码 <密码>          - 登录密码");
    println!("  列表                 - 列出目录内容");
    println!("  上传 <本地文件>      - 上传文件到服务器");
    println!("  下载 <远程文件>      - 从服务器下载文件");
    println!("  切换 <目录>          - 切换目录");
    println!("  当前目录             - 显示当前目录");
    println!("  创建目录 <目录名>    - 创建目录");
    println!("  删除目录 <目录名>    - 删除目录");
    println!("  删除 <文件名>        - 删除文件");
    println!("  重命名 <旧名> <新名> - 重命名文件");
    println!("  大小 <文件名>        - 查看文件大小");
    println!("  退出                 - 断开连接");
    println!("  帮助                 - 显示此帮助");
    println!();
    println!("也支持英文命令: USER, PASS, LIST, GET, PUT, CD, PWD, MKDIR, DEL, QUIT");
}
